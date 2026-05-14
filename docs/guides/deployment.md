# Deployment Guide

This guide describes the supported Docker Compose deployment models for the current Rust-first runtime.

## Current Default Runtime

The default backend deployment path contains these services:

- `agentforge-server` (`agentforge-server`) on `:4003`
- `agentforge-frontend` (`agentforge-frontend`) serving the built SPA inside the `prod` profile
- `orchestrator` (`agentforge-orchestrator`) on `:4010`
- `temporal` on `:7233` with UI on `:8233`
- PostgreSQL, Redis, and NATS backing services

Legacy `agentforge`, `platform-runtime`, `agentforge-mcp`, and `orchestrator-legacy` helper paths are not part of the default runtime.

In the self-contained `prod` profile, Caddy terminates HTTPS and proxies
browser routes and static assets to the `agentforge-frontend` artifact service.
In the `external` profile,
frontend assets are still deployed by your web tier or deployment automation.

## Prerequisites

- Docker and Docker Compose v2
- Make
- A populated `docker/.env`
- For external mode, reachable external databases and networks

## One-Time Setup

For a first local self-host trial, use the bootstrap target:

```bash
make bootstrap-local
```

It creates `docker/.env` when missing and fills the local secrets required by
the default Compose stack.

For production or shared environments, review and manage `docker/.env`
explicitly:

```bash
cp docker/.env.example docker/.env
make setup
```

At minimum, configure these values in `docker/.env`:

- `POSTGRES_PASSWORD`
- `REDIS_PASSWORD`
- `JWT_SECRET`
- NATS callout variables listed in [NATS Auth Runbook](../runbooks/nats-auth.md)
- `STORAGE_PROVIDER` and `STORAGE_LOCAL_PATH` when the defaults are not acceptable

## Profiles

| Profile    | Purpose                                                        |
| ---------- | -------------------------------------------------------------- |
| `dev`      | Local backend development stack                                |
| `prod`     | Self-contained production stack with internal backing services |
| `external` | Rust services attached to externally managed infrastructure    |
| `tools`    | Adminer and Redis Commander                                    |
| `backup`   | Scheduled PostgreSQL backup helper                             |
| `storage`  | MinIO object storage                                           |
| `casdoor`  | Casdoor identity provider integration                          |

## Service Inventory

| Service               | Default Port   | Role                                        |
| --------------------- | -------------- | ------------------------------------------- |
| `agentforge-server`   | `4003`         | Rust API and realtime gateway               |
| `agentforge-frontend` | internal       | Built SPA artifact server in `prod`         |
| `orchestrator`        | `4010`         | Rust orchestrator and workflow API          |
| `temporal`            | `7233`, `8233` | Workflow engine and UI                      |
| `db`                  | `5432`         | Application PostgreSQL                      |
| `orchestrator-db`     | internal       | Orchestrator PostgreSQL                     |
| `redis`               | `6379`         | Cache and coordination                      |
| `nats`                | `4222`, `8222` | Event transport                             |
| `caddy`               | `80`, `443`    | Reverse proxy and automatic HTTPS in `prod` |

## Development Deployment

```bash
make dev
```

This starts the backend platform for local development. Run the frontend separately:

```bash
npm run dev
```

Common companion commands:

```bash
make dev-d
make dev-tools
make dev-down
make dev-logs
```

## Self-Contained Production

For a public domain, point DNS at this host and pass the domain through
`DOMAIN`. Use the prebuilt-image path for a first VPS because it avoids a local
Rust/frontend build:

```bash
make quickstart-selfhost-pull DOMAIN=forge.example.com
```

For a private localhost trial, omit `DOMAIN`:

```bash
make quickstart-selfhost-pull
```

Caddy obtains and renews public HTTPS certificates automatically when DNS points
to the host and ports `80` and `443` are reachable. Local/private trials use
Caddy's internal local CA and may show a browser warning unless that CA is
trusted on the client machine.

If the host already uses `80` or `443`, pass alternate public ports. The
bootstrap writes the matching `APP_URL` and `CORS_ORIGIN`:

```bash
make quickstart-selfhost-pull DOMAIN=localhost HTTP_PORT=18080 HTTPS_PORT=18443
```

Open `https://localhost:18443` for that trial.

This starts the Rust API, frontend artifact service, Caddy, PostgreSQL, Redis,
NATS, Temporal, and the orchestrator, then checks the final Caddy HTTPS URL plus
API readiness through the public ingress.

Before treating a build as beginner-ready, run the self-host audit. The default
audit is non-destructive: it verifies the beginner Make targets, fresh env
bootstrap, generated production secrets, Compose config, Caddy config, and that
the self-host bootstrap does not depend on the local Node/npm development path:

```bash
make beginner-audit
```

For release or VPS validation, include the optional checks that pull the public
images, start an isolated localhost production stack, probe the live ingress,
and exercise a real Provider + Prompt agent:

```bash
make beginner-audit DOMAIN=forge.example.com BEGINNER_AUDIT_FLAGS="--pull-images --local-smoke --live"

BEGINNER_ORIGIN_IP=203.0.113.10 \
make beginner-audit DOMAIN=forge.example.com BEGINNER_AUDIT_FLAGS="--live"

BASE_URL=https://forge.example.com \
E2E_EMAIL=dev@example.com \
E2E_PASSWORD=... \
BEGINNER_PROVIDER=openrouter \
BEGINNER_MODEL=openai/gpt-4o-mini \
BEGINNER_API_KEY=... \
make beginner-audit DOMAIN=forge.example.com BEGINNER_AUDIT_FLAGS="--provider"

BASE_URL=https://forge.example.com \
E2E_EMAIL=dev@example.com \
E2E_PASSWORD=... \
BEGINNER_USE_EXISTING_PROVIDER=1 \
make beginner-audit DOMAIN=forge.example.com BEGINNER_AUDIT_FLAGS="--provider"
```

`BEGINNER_ORIGIN_IP` is optional. Use it when DNS points through a CDN but you
also need to prove the source VPS answers on `:80` and `:443` with the
production domain as Host/SNI.
`BEGINNER_USE_EXISTING_PROVIDER=1` is useful after a user has already added and
successfully tested a provider in Settings; it reuses that stored provider to
verify the real Provider + Prompt path without re-entering the API key.
For local keyless inference, run Ollama separately, set `OLLAMA_BASE_URL` to a
URL reachable from the `agentforge-server` container, add the `ollama` provider
in Settings without an API key, then test it there before running the existing
provider audit.

Use `make quickstart-selfhost` instead when you intentionally want to build the
server and frontend images from source on the host.

To run the same flow step-by-step:

```bash
make bootstrap-selfhost DOMAIN=forge.example.com
make selfhost-check DOMAIN=forge.example.com
make prod-pull
make selfhost-health DOMAIN=forge.example.com
```

Optional variants:

```bash
make prod-backup
make prod-storage
make prod-casdoor
make prod-down
make prod-logs
make selfhost-health
```

This mode is intended for environments where PostgreSQL, Redis, Temporal, and
the HTTPS reverse proxy are managed inside the Compose stack.

For local attachment storage, Compose mounts the `agentforge-uploads` named
volume at `${STORAGE_LOCAL_PATH:-/var/lib/agentforge/uploads}` inside the Rust
API container. Keep that mount in place when hardening the service with a
read-only root filesystem.

If attachments should live in MinIO instead, set `STORAGE_PROVIDER=minio` plus
`MINIO_ENDPOINT`, `MINIO_ACCESS_KEY`, and `MINIO_SECRET_KEY`, then start the
storage profile:

```bash
make prod-storage
```

## External-Service Production

```bash
make prod-ext
```

Use this mode when PostgreSQL, Redis, or supporting network boundaries are managed outside the stack. In this profile, Compose still starts the Rust API, Rust orchestrator, Temporal, and NATS, but the application database, orchestrator database, and Redis can remain externally managed.

Before startup, ensure:

- `DATABASE_URL` points at the intended Wisdoverse Forge application database. This can be an existing legacy database; the current Rust migrations are written to run idempotently in place.
- `ORCHESTRATOR_DATABASE_URL` points at the intended orchestrator database.
- `REDIS_URL` points at the intended external Redis instance.
- `EXTERNAL_NETWORK` is a Docker network shared with any external containers or gateways the stack must reach.
- `STORAGE_PROVIDER=local` has a writable named volume at `STORAGE_LOCAL_PATH`, or `STORAGE_PROVIDER=minio` has valid MinIO/S3 credentials.

Companion commands:

```bash
make prod-ext-down
make prod-ext-logs
```

## Frontend Deployment

For `make prod`, Compose builds and runs the `agentforge-frontend` artifact
service automatically.

For the `external` profile or a custom web tier, build the frontend artifact
with:

```bash
npm run build
```

Publish `dist/` through your web tier. The repository also includes deployment helpers such as `docker/deploy-frontend.sh` for environments that copy static assets into a reverse-proxy web root.

## Health Verification

After deployment, verify:

```bash
curl http://localhost:4003/health
curl http://localhost:4010/health
```

In environments that expose Temporal UI, also confirm `http://localhost:8233` is reachable.

For the external profile, also verify internal workflow and event wiring:

```bash
docker exec agentforge-temporal temporal operator cluster health --address temporal-internal:7233
docker exec agentforge-nats wget -qO- http://127.0.0.1:8222/connz?subs=1
```

`temporal-internal` should report `SERVING`, and `connz` should show at least one Rust client when the API has connected to NATS.

## Migrations

Use the Rust server migration entry point when needed:

```bash
npm run migrate
```

In containerized deployments, `make migrate` runs the migration command inside the Rust server container.

## When to Update This Document

Update this guide whenever you change:

- Compose profiles or service inventory,
- default ports or published endpoints,
- runtime ownership between services,
- frontend deployment expectations,
- required environment variables for deployment.
