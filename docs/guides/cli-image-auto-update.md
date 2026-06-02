# CLI agent-image auto-update

Keeps a running deployment's Container CLI agent images current with the
overlays the `watch-cli-versions.yml` GitHub workflow publishes to GHCR — without
an operator manually running `make update-agents`.

## Why

`watch-cli-versions.yml` rebuilds + pushes `agent-<tool>:latest` (codex, gemini,
opencode) to GHCR every 6 hours when the upstream CLI publishes a new npm
version. A self-hosted deployment, however, keeps whatever image it last pulled:
new agents spawn from the stale local image until someone pulls a fresh one. This
feature closes that gap with a **deployment-side poller** — firewall-safe, no
inbound GitHub→deployment reachability, no CI→deploy coupling.

## What it does (when enabled)

A background worker (`CliImageUpdater`) periodically, for each public CLI tool
(**never `claude`** — it has no public image, built locally under license):

1. Asks the registry for the current manifest digest of
   `${AGENT_REGISTRY}/agent-<tool>:${AGENT_CLI_IMAGE_TAG}` — **without pulling**
   (daemon-side `GET /distribution/<image>/json`).
2. Compares it to the locally-pulled digest of the same ref.
3. On drift: `docker pull` the new image, then **re-tag** it to
   `agentforge-agent:<tool>` — the exact ref the container-start resolver
   (`AgentContainerImagePolicy::resolve`) produces — so the **next** spawned
   agent uses the new CLI.

> If you pin `CONTAINER_IMAGE_<TOOL>` to a custom registry ref, that ref is
> _not_ auto-updated (a pin is treated as an explicit opt-out); the updater only
> manages the `agentforge-agent:<tool>` convention ref.

**Running agents are never interrupted** — only the image the next spawn resolves
is refreshed. The policy is auto-pull + notify, never auto-roll.

## Enable

```bash
# docker/.env
CLI_IMAGE_AUTO_UPDATE_ENABLED=true          # default false
CLI_IMAGE_AUTO_UPDATE_INTERVAL_SECS=900      # default 900 (15 min)
CLI_IMAGE_PRUNE_ENABLED=false                # default false; requires AUTO_UPDATE on (prune runs inside the update check)
AGENT_CLI_IMAGE_TAG=latest                   # overlay tag to track
# AGENT_REGISTRY already set (default ghcr.io/wisdoverse/wisdoverse-forge)
```

Requires a reachable Docker daemon (the same socket the server already uses to
spawn agents). If `CLI_IMAGE_AUTO_UPDATE_ENABLED=true` but no daemon is
available, the worker logs a warning and does nothing. Public overlays pull
anonymously; for a private registry, the host's ambient `docker login` credential
is reused (no token is plumbed into the Rust process).

## Observe

- **Admin status API**: `GET /api/v1/admin/cli-images` (admin-gated) returns a
  per-tool report — `state` (`pending` | `up_to_date` | `updated` | `failed`),
  the local and remote manifest digests, last-checked / last-updated timestamps,
  the last error, and `agentsWithContainer` (a rough per-tool live-container
  count; it does NOT assert which digest each container booted from). The report
  also echoes `autoUpdateEnabled`, `pollIntervalSecs`, `registry`, and
  `imageTag` (JSON is camelCase). Every pollable tool appears even before the
  first tick (as `pending`); `claude` is never listed.

  ```bash
  curl -s -H "Authorization: Bearer $ADMIN_JWT" \
    http://localhost:4003/api/v1/admin/cli-images | jq .
  ```

- **Prune status**: the same report carries a `prune` object — `enabled`,
  `lastRunUnix`, `scanned`, `removed`, `skippedInUse`, `skippedConflict`,
  `errors`, `lastError` — and the admin panel renders an "Old image cleanup"
  summary. When `CLI_IMAGE_PRUNE_ENABLED=false` (default) it reports `enabled:
false` and nothing is removed.
- Log: `cli agent image updated tool=codex from=sha256:… to=sha256:…` (warn).
- Metrics: `agentforge_cli_image_pull_total{tool,result=success|skipped|failed}`,
  `agentforge_cli_image_drift_detected_total{tool}`,
  `agentforge_cli_image_pull_duration_seconds{tool}`,
  `agentforge_cli_image_pruned_total`.

## Security

Image-level ops only (pull / registry-inspect / local-inspect / tag / remove) —
they never create a container, build a `HostConfig`, or touch
`platform/security.rs`, so the container-creation defense-in-depth is unchanged.
No new Docker mount, no new secret surface for public images.

**Prune is shared-host safe by construction.** It NEVER runs a global
`docker image prune` or a label/name glob removal. It removes only an image
whose content id is NOT in the set of images referenced by ANY container
(`list_containers(all=true)`), with `force=false` + `noprune=true` so a shared
parent layer is never cascade-deleted, and only when the image is DANGLING and a
repo digest names one of our own pollable-tool GHCR overlays
(`<registry>/agent-<tool>`). The base image and other stacks' images can never
match. A 409 conflict is treated as "leave it", not an error.

## Operator-initiated roll (staging-gated)

`POST /api/v1/admin/cli-images/{tool}/roll` (admin-gated) drains + respawns the
running container agents of one tool onto the freshly re-tagged image — for when
you want existing agents on the new CLI, not just new ones. Unlike the
auto-updater this DOES touch running agents, so it is operator-initiated and
never `claude`. A roll of one agent = `stop` (removes its container, clears the
container id) then `start` (recreates it from the resolved, now-updated image).

Safety:

- **Idle-only**: an agent in the `working` state is SKIPPED (reported as
  `skippedBusy`). Rolling a busy agent would interrupt its work and, because the
  sidecar's dedup WAL is container-local and destroyed with the container, risk a
  redelivered assignment double-executing. `status` is a best-effort signal, so
  **soak this on staging before enabling in production.**
- **Own scope**: each agent is rolled within its own persisted org/user/workspace
  (the existing tenant-scoped `stop`/`start` enforce every per-org invariant); no
  privilege is fabricated.
- **Single-flight**: a second concurrent roll of the same tool returns `409`.
- **Authorization note**: this uses the same admin gate as the other destructive
  cross-tenant admin endpoints (e.g. `DELETE /admin/agents/{id}`). A
  platform-admin vs org-admin distinction is a separate, surface-wide hardening.

Status codes:

| Code  | When                                                                                                                                                                                                                                                                            |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `200` | Roll completed — all-succeeded, mixed, or an empty/all-skipped no-op (per-agent outcome is in `results`).                                                                                                                                                                       |
| `503` | The container runtime (Docker) is unavailable on this deployment **and** there is at least one non-busy (idle/offline) agent to roll. Returned **once** for the whole roll, not N identical per-agent errors. When nothing was rollable, you get the empty `200` no-op instead. |
| `422` | Tool is `claude` or unknown (not in the pollable set).                                                                                                                                                                                                                          |
| `409` | A roll of the same tool is already in progress.                                                                                                                                                                                                                                 |

Result shape: `{ tool, total, succeeded, failed, skippedBusy, results: [{ agentId,
ok, stopped, error? }] }`. `total` counts every agent considered (rolled +
skipped). Each `results` entry carries `ok` and a `stopped` boolean; `error` is a
client-safe message (full errors are logged server-side). On a per-agent failure
`stopped` tells the operator exactly how far the roll got:

- **Respawn failed** (`ok: false`, `stopped: true`): the container was confirmed
  stopped + removed but the respawn errored, so **the agent is now down**. Restart
  it from the Agents view.
- **Stop did not complete** (`ok: false`, `stopped: false`): the stop itself
  errored, so the post-condition is **UNCONFIRMED**. `stop` is not atomic
  (stop → remove → clear container id), so the agent may still be running on the
  old image **or** may have been brought partway down — either way a clean stop
  was not confirmed. **Check the Agents view** to see its real state.

## Shipped follow-ups

- **Live WebSocket toast** for admins when an update lands or a check fails
  (`broadcast.admin.cli_image`).
- **Pruning superseded images** (`CLI_IMAGE_PRUNE_ENABLED`): each update leaves
  the previous GHCR-ref image dangling; the prune sweep reclaims that disk safely
  (see Security above).
- **Operator-initiated roll**: see above (staging-gated).

## Deferred (follow-ups, not in this increment)

- **Warm-pool adoption**: the warm pool (`platform/pool.rs`) is currently dormant
  (not in the agent-start path); when adopted, the updater should drain+rewarm it
  on drift. Until then the `tag_image` re-point is the freshness mechanism.
