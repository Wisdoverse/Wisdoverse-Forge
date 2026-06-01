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

## Shipped follow-ups

- **Live WebSocket toast** for admins when an update lands or a check fails
  (`broadcast.admin.cli_image`).
- **Pruning superseded images** (`CLI_IMAGE_PRUNE_ENABLED`): each update leaves
  the previous GHCR-ref image dangling; the prune sweep reclaims that disk safely
  (see Security above).

## Deferred (follow-ups, not in this increment)

- **Manual roll**: `POST /admin/cli-images/{tool}/roll` to drain+respawn running
  agents of one tool onto the new image (operator-initiated; never automatic).
- **Warm-pool adoption**: the warm pool (`platform/pool.rs`) is currently dormant
  (not in the agent-start path); when adopted, the updater should drain+rewarm it
  on drift. Until then the `tag_image` re-point is the freshness mechanism.
