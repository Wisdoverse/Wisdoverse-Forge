# Third-Party CLI Image Policy

Wisdoverse Forge agent images can include third-party Container CLI tools. Public
release images must include only tools whose package license permits public
redistribution under standard open-source terms.

## Public GHCR Image Set

The public GitHub Container Registry release publishes:

- `agent-base`
- `agent-opencode`
- `agent-codex`
- `agent-gemini`

These images are built with pinned CLI package versions resolved during release
and include BuildKit SBOM and provenance attestations.

## Claude Code

Public releases do not publish `agent-claude`. The Claude Code package license
points to Anthropic legal terms instead of a standard open-source
redistribution license.

Operators who need Claude Code should build it locally after accepting the
applicable vendor terms:

```bash
make build-agent CLI_TOOL=claude
```

Private deployments may publish a Claude image to an internal registry only when
their Anthropic terms permit that redistribution:

```bash
make update-agents AGENT_REGISTRY=registry.example.com/wisdoverse/forge AGENT_TOOLS="claude opencode codex gemini"
```

## Release Guardrails

- Keep `@anthropic-ai/claude-code` out of the public GitHub Actions release
  matrix unless legal approval explicitly confirms public redistribution rights.
- Keep public release notes clear that third-party CLI names and trademarks
  belong to their respective owners.
- Keep SBOM/provenance enabled for public agent images.

## Image Prune Safety On Shared Hosts

Operators who turn on `CLI_IMAGE_PRUNE_ENABLED` (default `false`) reclaim disk
from superseded agent overlay images. Prune is safe to run on a Docker host
shared by other stacks because it is narrow by construction, not by
configuration. It never runs a global image prune, and it cannot remove an image
another stack still relies on.

Before enabling prune, confirm:

- `CLI_IMAGE_AUTO_UPDATE_ENABLED=true` — prune runs inside the updater sweep and
  is a no-op when the updater is off.
- The deployment owns the `agent-<tool>` overlays it pulls from `AGENT_REGISTRY`.
  Prune only ever targets those.

What prune removes, and the guard that enforces it:

- It removes only a **dangling** image (no repo tags), so a tagged image any
  stack could resolve is never a candidate.
- The image's repo digest must name one of **our own** pollable-tool overlays.
  The match is exact-repo equality, not a prefix, so a third-party image that
  merely shares a registry path prefix is left alone.
- **No container — running or stopped — may reference it.** This guard surfaces
  on the status report as `skippedInUse`: before any Docker call, an image
  whose id is in the set referenced by an existing container is skipped. As
  defense-in-depth, removal still goes through Docker with `force=false` and
  `noprune=true`, so a Docker-side `409 Conflict` (still tagged, has a child
  layer, or in use) is also treated as leave-it and recorded as
  `skippedConflict`.

These checks live in `is_prunable_agent_image()`; all must hold for an image to
be removed. Prune is image-level only and never touches a container's lifecycle.

Success looks like a clean sweep summary on the admin status report
(`GET /api/v1/admin/cli-images`, admin-only). The `prune` block reports
`enabled`, `lastRunUnix` (populated once a sweep has run), `scanned`,
`removed`, `skippedInUse`, `skippedConflict`, `errors`, and `lastError`. A
non-zero `skippedInUse` or `skippedConflict` is expected and healthy on a
shared host — it is the safety guard declining to remove an image another
container still references, not a failure. Prune is best-effort: any error is
counted and logged, the summary still records, and the next sweep retries.

## Operator-Initiated Roll Scope

The operator roll (`POST /api/v1/admin/cli-images/{tool}/roll`) respawns idle
agents of one Container CLI onto the freshly pulled overlay. It is an
administrative operation with two access boundaries an operator should
understand before using it.

**It is admin-gated.** The route requires an owner or admin role through the auth
middleware, the same gate as other `/admin` agent operations. The tool must also
be rollable: `RollToolPolicy::ensure_rollable` rejects `claude` and unknown tools
with `422` because they have no public registry image to roll onto.

**Each agent is rolled under its own persisted tenant scope — never a fabricated
or cross-organization one.** The roll does not invent elevated access. For every
target agent it reads the agent's real organization, user, and workspace from the
stored row and reconstructs that agent's own `TenantScope`, then performs the
roll through the existing tenant-scoped stop and start primitives. A roll is a
`stop` (removes the container, clears `container_id`) followed by a `start`
(recreates from the resolved, now-updated image). Because it reuses those
primitives, every per-organization invariant they enforce still holds: an agent
in organization A is stopped and started only within organization A's scope, and
the admin endpoint cannot move work or containers across organizations.

Only **idle or offline** agents are rolled. A `working` agent is intentionally
left alone and reported as `skipped_busy`, because rolling it would interrupt
in-flight work and risk a redelivered assignment double-executing against the
fresh container (the dedup write-ahead log is destroyed with the old container).

Concurrency and runtime guards keep the blast radius bounded:

- A single-flight guard (`RollGuard`) allows one roll per tool at a time; a
  concurrent same-tool roll returns `409`. The slot frees on drop regardless of
  outcome.
- If the container runtime is unavailable on the deployment and there is at least
  one non-busy (idle/offline) agent to roll, the whole roll returns `503` once,
  rather than emitting one identical per-agent error per agent.

A roll can leave an agent down. When a respawn fails after a confirmed stop
(`stopped: true` in the per-agent result), that agent is stopped and must be
restarted through the normal control path. When the stop itself did not complete
(`stopped: false`), the post-condition is unconfirmed — stop is not atomic
(stop → remove → clear), so the operator should check the Agents view rather than
assume a clean state. For the full result shape and operator runbook, see
`docs/guides/cli-image-auto-update.md`.
