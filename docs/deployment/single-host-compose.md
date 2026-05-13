# Single-Host Docker-Compose Deploy

This document describes the deployment topology that `scripts/deploy.sh`
implements: one Linux host, Docker Compose, an externally-managed PostgreSQL
database, and an nginx reverse proxy in front of the SPA + Rust API.

It is one valid topology. Operators running on Kubernetes, Nomad, hosted
Docker, plain rsync, or a managed PaaS should write their own deploy
entry-point and reuse the validators (`scripts/validate-deploy-nats-env.sh`,
`scripts/check-production-env.sh`). See [Other topologies](#other-topologies)
below.

## When to use this script

`scripts/deploy.sh` is the right tool when all of the following are true:

- You run the stack on a single host (no clustering, no load-balanced N-of-M
  rolling deploy).
- You use Docker Compose with an external service profile.
- You manage PostgreSQL outside the compose file (e.g. AWS RDS, a managed
  Postgres, or another container stack on the same host).
- nginx (or another HTTP server) serves the SPA from a file path on disk and
  proxies the Rust API + WebSocket traffic to `localhost:${AGENTFORGE_PORT}`.
- You accept manual rollback on health-check failure (the script exits 1 and
  leaves the stack in its degraded state for the operator to inspect).

If any of those does not hold, treat this script as a reference and adapt or
replace it.

## Runtime contract

`scripts/deploy.sh` reads its configuration from environment variables. CI/CD
pipelines should set the variables explicitly (in GitLab/GitHub project
settings, Vault, or the deploy bundle). Operators running the script by hand
should set them in `docker/.env`.

### Required

| Variable  | Notes                                                                                                                                                                                                 |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `WEBROOT` | Absolute path nginx serves the SPA from. Required for production. Staging warns and falls back to `/opt/agentforge/www` for backwards compatibility; the fallback will be removed in a later release. |

### Optional (with documented defaults)

| Variable                 | Default                                    | Notes                                                                    |
| ------------------------ | ------------------------------------------ | ------------------------------------------------------------------------ |
| `AGENT_REGISTRY`         | _unset_                                    | Registry to pull agent images from. Unset = use local images only.       |
| `AGENT_TOOLS`            | `claude opencode codex gemini`             | Space-separated agent CLI list.                                          |
| `REQUIRED_AGENT_TOOL`    | `claude`                                   | Must be present after pull or deploy fails.                              |
| `AGENTFORGE_NETWORKS`    | `agentforge-agents external-network`       | Space-separated docker networks created (idempotently) before bring-up.  |
| `COMPOSE_FILES_OVERRIDE` | `-f compose.yml -f compose.external.yml`   | Override the compose file list.                                          |
| `COMPOSE_PROFILE`        | `external`                                 | Compose profile passed via `--profile`.                                  |
| `COMPOSE_PROJECT_NAME`   | `wisdoverse-forge`                         | Compose project name shown by Docker and used as the container prefix.   |
| `COMPOSE_SERVICE_NAME`   | `agentforge-server`                        | Service name used for migrate-only and audit lookups.                    |
| `FRONTEND_IMAGE`         | `agentforge-frontend:${IMAGE_TAG:-latest}` | Local docker image holding `/app/dist/`.                                 |
| `FRONTEND_DEPLOY_MODE`   | `symlink`                                  | See [Frontend deploy modes](#frontend-deploy-modes).                     |
| `WEBROOT_OWNER_UID`      | `1000`                                     | Owner UID for written files.                                             |
| `WEBROOT_OWNER_GID`      | `1000`                                     | Owner GID for written files.                                             |
| `KEEP_RELEASES`          | `5`                                        | How many release dirs to retain in symlink mode.                         |
| `AGENT_PULL_RETRIES`     | `3`                                        | Retry attempts for agent pulls.                                          |
| `AGENT_PULL_BACKOFF`     | `5`                                        | Seconds between retries.                                                 |
| `HEALTH_PATH`            | `/api/health`                              | Health endpoint path.                                                    |
| `HEALTH_RETRIES`         | `10`                                       | Curl retry count for health probe.                                       |
| `HEALTH_BACKOFF`         | `3`                                        | Seconds between health retries.                                          |
| `AGENTFORGE_PORT`        | `4003`                                     | Port for the health probe (`localhost:$PORT`).                           |
| `BUNDLE_REQUIRED_FILES`  | `nats.conf seccomp/agentforge-agent.json`  | Bind-mount source files that must exist under `docker/` before bring-up. |

## Frontend deploy modes

`FRONTEND_DEPLOY_MODE` controls how the dist files reach `WEBROOT`.

### `symlink` (default)

1. Extract `/app/dist/` from `agentforge-frontend:${IMAGE_TAG:-latest}` into a
   timestamped release directory next to the swap target
   (`$(dirname "$target")/releases/$RELEASE_TS`).
2. Atomically swap the target path to point at the new release via `ln -sfn`
   plus `mv -T` (a single `rename(2)` syscall). If `WEBROOT` is itself a
   symlink, the deploy preserves that alias and swaps the symlink's target
   instead.
3. Retain the most recent `KEEP_RELEASES` release directories.

This mode requires nginx to follow symlinks. If your `nginx.conf` contains
`disable_symlinks if_not_owner` (or similar), either remove it for `WEBROOT`
or use `rsync` mode.

### `rsync`

1. Extract `/app/dist/` into a tempdir.
2. `rsync -a --delete` the tempdir into the existing `WEBROOT` directory.
3. `chown -R $WEBROOT_OWNER_UID:$WEBROOT_OWNER_GID $WEBROOT`.

Use this mode when nginx refuses symlinks, or when `WEBROOT` is managed by a
panel UI (e.g. 1Panel, Plesk, BT) that expects the path to remain a real
directory it owns.

The trade-off: `rsync` is **not atomic**. Browsers loading the SPA mid-deploy
may see a few hundred milliseconds where index.html points at not-yet-copied
asset hashes. For most internal staging deployments this is acceptable; for
production, prefer `symlink` mode if the nginx config allows it.

## nginx requirements

See `examples/nginx/agentforge.conf` for an annotated reference config. The
key requirements are:

- `root` (or `alias`) points at `WEBROOT`.
- For `symlink` mode: nginx must follow symlinks. Default behaviour follows
  them; only set `disable_symlinks` deliberately.
- WebSocket support: `proxy_pass http://127.0.0.1:${AGENTFORGE_PORT};` with
  `Upgrade` / `Connection` headers and a long `proxy_read_timeout`.
- API path proxy: `/api/` forwarded to `localhost:${AGENTFORGE_PORT}` with
  forwarded-for headers.
- SPA fallback: requests for unknown paths return `/index.html` so the React
  router can handle client-side routing.
- HTTPS termination: TLS certificates configured at the nginx layer (the Rust
  API binds plain HTTP on `127.0.0.1`).

## Migration note for legacy staging hosts

Earlier versions of `scripts/deploy.sh` hard-coded `WEBROOT=/opt/agentforge/www`
for staging. Hosts that have not yet set `WEBROOT` in `docker/.env` continue
to deploy to that path with a warning logged on every run; production already
fails fast.

To migrate:

1. Set `WEBROOT` in `docker/.env` on the staging host to the path nginx
   actually serves the SPA from (use `nginx -T | grep root` to confirm).
2. Re-run the deploy. The warning disappears.

The fallback is scheduled for removal once all staging hosts are migrated.

## Image registry

Container images are published to GitHub Container Registry on every push to
`main` and on every `v*` tag. Authenticated pulls work for any GitHub account
once the package visibility is set to public on the project's GHCR settings
page.

| Image                                                               | Built from                     | Notes                                                                     |
| ------------------------------------------------------------------- | ------------------------------ | ------------------------------------------------------------------------- |
| `ghcr.io/wisdoverse/wisdoverse-forge`                               | `docker/Dockerfile`            | SPA frontend image (used by `scripts/deploy.sh` to extract `/app/dist/`). |
| `ghcr.io/wisdoverse/wisdoverse-forge/server`                        | `rust/Dockerfile`              | Main API binary.                                                          |
| `ghcr.io/wisdoverse/wisdoverse-forge/orchestrator`                  | `rust/Dockerfile.orchestrator` | Temporal workflow runner.                                                 |
| `ghcr.io/wisdoverse/wisdoverse-forge/sidecar`                       | `rust/Dockerfile.sidecar`      | Per-agent container sidecar.                                              |
| `ghcr.io/wisdoverse/wisdoverse-forge/agent-base`                    | `docker/Dockerfile.agent-base` | Shared base for agent containers.                                         |
| `ghcr.io/wisdoverse/wisdoverse-forge/agent-{opencode,codex,gemini}` | `docker/Dockerfile.agent`      | Per-CLI overlay images.                                                   |

The Claude CLI image is intentionally **not** published in public GHCR
because the package license points to Anthropic terms rather than a
standard open-source redistribution license. Operators can build it locally
with `make build-agent-all` or push it to a private registry whose terms
permit redistribution.

### Tag scheme

| Trigger             | Tags applied                                      |
| ------------------- | ------------------------------------------------- |
| Push to `main`      | `:main`, `:edge`, `:sha-<short>`                  |
| Push tag `v0.2.0`   | `:0.2.0`, `:0.2`, `:0`, `:latest`, `:sha-<short>` |
| `workflow_dispatch` | `:sha-<short>` only (use for ad-hoc rebuilds)     |

### Pulling for `scripts/deploy.sh`

Set the registry-image env vars to point at GHCR:

```bash
# In docker/.env or in your deploy environment:
AGENT_REGISTRY=ghcr.io/wisdoverse/wisdoverse-forge
```

Then invoke the deploy with the desired tag:

```bash
# Pinned to a release tag
bash scripts/deploy.sh staging 0.2.0 ghcr.io/wisdoverse/wisdoverse-forge

# Tracking main (bleeding edge — use only for non-production environments)
bash scripts/deploy.sh staging main ghcr.io/wisdoverse/wisdoverse-forge
```

Authentication: GHCR requires `docker login ghcr.io` with a personal access
token that has `read:packages` scope, even for public images on some
runners. Configure this once on each deploy host before running the script.

## Observability + promotion gate

`ops/prometheus/alerts.yml` ships drop-in alert rules covering the SLOs the
platform should defend in production: 5xx rate >1% / 5m, multi-window error
budget burn (14.4× over 1h+5m), p95 latency >1s / 10m, p99 >3s / 10m,
orchestrator backlog age >60s, NATS pending messages >1000, container
restart loop, staging health-check failure, and resident-memory >80%.
Mount the file into Prometheus via `rule_files:` and let your existing
Alertmanager routes wire severities (`page` / `ticket`) to PagerDuty,
Slack, or email.

`ops/grafana/dashboards/agentforge-overview.json` is the matching overview
dashboard — request rate per route, 5xx rate, latency p50/p95/p99,
orchestrator lag, NATS pending, server resident memory, and a
container-restart stat panel. Import via Grafana → Dashboards → New →
Import → upload JSON.

`scripts/staging-soak.sh` runs a configurable health-probe loop and exits 0
only when the soak window stays clean.
`.github/workflows/promote-to-production.yml` wraps the script as a
human-in-the-loop `workflow_dispatch` gate: pick a target URL and soak
duration, the workflow probes for the configured window, prints a
green-soak summary on success. Operators then dispatch the existing
`Release` workflow with the same tag knowing the build was actually
exercised before going live. For the full 24h enterprise window, run the
script from a long-lived runner / systemd unit instead of a single GitHub
Actions job (the GitHub-hosted runner ceiling is 6h per job).

## Other topologies

The deploy script intentionally targets one specific shape. For other
topologies, the validators are reusable:

- `scripts/validate-deploy-nats-env.sh` — verifies NATS rollout-flag
  credentials are present when the matching feature flags are on. Safe to
  call from any deploy entry-point that uses the same env-var names.
- `scripts/check-production-env.sh` — verifies a production-grade environment
  config. Safe to call before applying any production deployment.

Suggested adapter strategies:

- **Kubernetes**: render the same env vars into a Deployment/Job manifest;
  let the cluster handle rolling restart and frontend serving (e.g. via an
  ingress + a separate static-asset server like nginx-ingress + a CDN, or via
  a `frontend-artifact` initContainer that populates an `emptyDir`).
- **Hosted Docker (Render, Fly, Railway)**: build the frontend image, push to
  the platform's registry, configure a static-asset bucket for the SPA, and
  point health checks at `/api/health`.
- **Plain rsync to a static host**: build dist locally (`npm run build`),
  rsync to the host, restart the API service via systemd or a similar
  supervisor.

In each case, run the relevant validator before applying changes so missing
env vars fail fast with a descriptive error.
