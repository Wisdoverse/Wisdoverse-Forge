# Configuration Guide

This guide documents the primary configuration surfaces for the Rust-first runtime. It focuses on variables that operators and contributors change regularly. The code remains the final source of truth for defaults and validation.

## Source of Truth

- `rust/crates/core/src/config.rs` for Rust API configuration
- `rust/crates/orchestrator/src/config.rs` for Rust orchestrator configuration
- `rust/crates/api/src/mcp.rs` for internal MCP and container runtime settings
- `docker/compose.yml` and overlays for deployment wiring

## Configuration Layers

| Layer                              | Surface                                                        | Purpose                                                    |
| ---------------------------------- | -------------------------------------------------------------- | ---------------------------------------------------------- |
| Docker and Compose                 | `docker/.env`, `docker/compose*.yml`                           | Ports, passwords, profile wiring, external networks        |
| Rust API                           | standard env vars such as `PORT`, `DATABASE_URL`, `JWT_SECRET` | User-facing API and realtime service                       |
| Rust orchestrator                  | `ORCHESTRATOR_*` vars                                          | Tasks, reviews, workflows, knowledge, Temporal integration |
| Internal MCP and container runtime | `MCP_*`, `CONTAINER_*`, `AGENTFORGE_WORKSPACE_ROOT`            | Agent session execution and workflow activities            |

## Minimum Local Development Values

For the recommended Compose path, run `make bootstrap-local` to create and fill
these values in `docker/.env`:

- `POSTGRES_PASSWORD`
- `REDIS_PASSWORD`
- `JWT_SECRET`
- `MCP_TOKEN`
- `API_KEY_SALT`
- `LLM_ENCRYPTION_KEY`
- `NATS_BACKEND_PASSWORD`
- `NATS_AUTH_SERVICE_PASSWORD`
- `NATS_SYS_PASSWORD`
- `NATS_CALLOUT_ISSUER_SEED`
- `NATS_CALLOUT_ACCOUNT_SIGNING_KEY_SEED`
- `NATS_CALLOUT_XKEY_SEED`
- `NATS_CALLOUT_ISSUER_PUBLIC`
- `NATS_CALLOUT_XKEY_PUBLIC`

Common optional values for local clarity:

- `AGENTFORGE_PORT`
- `BIND_ADDRESS`
- `ORCHESTRATOR_PORT`

Initial owner creation is not controlled by an environment variable. For a fresh
database, start the web app and complete the setup/register flow in the browser.
The example environment files intentionally do not ship a default application
administrator.

## Rust API Variables

| Variable             | Default       | Required               | Purpose                                            |
| -------------------- | ------------- | ---------------------- | -------------------------------------------------- |
| `PORT`               | `4003`        | No                     | Rust API listen port                               |
| `HOST`               | `0.0.0.0`     | No                     | Rust API bind host                                 |
| `DATABASE_URL`       | none          | Yes                    | PostgreSQL connection string                       |
| `REDIS_URL`          | none          | No                     | Redis connection string                            |
| `NATS_URL`           | none          | No                     | NATS connection string                             |
| `JWT_SECRET`         | none          | Yes                    | JWT signing secret; must be at least 32 characters |
| `JWT_EXPIRY_SECONDS` | `900`         | No                     | Access token lifetime in seconds                   |
| `ENVIRONMENT`        | `development` | No                     | Runtime mode                                       |
| `LOG_LEVEL`          | `info`        | No                     | Tracing filter                                     |
| `CORS_ORIGIN`        | none          | Required in production | Allowed browser origin for production CORS         |

`NODE_ENV` may still appear in Compose or frontend tooling, but the Rust API configuration source of truth is `ENVIRONMENT`.

## Local Agent Join Variables

These control the one-command Host CLI join flow
(see [Host CLI Agent Enrollment](../runbooks/host-cli-agent-enrollment.md)).

| Variable                    | Default                           | Required | Purpose                                                                                                               |
| --------------------------- | --------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------- |
| `APP_URL`                   | none                              | No       | Public URL of this deployment; required for the join commands to be generated                                         |
| `NATS_AGENT_URL`            | none                              | No       | NATS address reachable from operator machines; must be `tls://` unless plaintext is explicitly allowed                |
| `ALLOW_PLAINTEXT_HOST_NATS` | `false`                           | No       | Permit `nats://` (plaintext) Host CLI enrollment — isolated dev/test only                                             |
| `HOST_JOIN_BINARY_BASE_URL` | this repo's GitHub latest release | No       | Where the join script downloads `agentforge-sidecar` binaries; point at an internal mirror for air-gapped deployments |

## Attachment Storage Variables

The Rust API owns attachment metadata and object access. Metadata is stored in
PostgreSQL; bytes are stored through the configured object storage provider.

| Variable                        | Default                      | Required                        | Purpose                                              |
| ------------------------------- | ---------------------------- | ------------------------------- | ---------------------------------------------------- |
| `STORAGE_PROVIDER`              | `local`                      | No                              | Attachment object provider: `local` or `minio`       |
| `STORAGE_LOCAL_PATH`            | `~/.agentforge/data/uploads` | Required for local provider     | Local object root used when `STORAGE_PROVIDER=local` |
| `STORAGE_MAX_FILE_SIZE`         | `10485760`                   | No                              | Maximum upload size in bytes                         |
| `STORAGE_MAX_FILES_PER_SESSION` | `20`                         | No                              | Per-agent attachment count guard                     |
| `STORAGE_SIGNED_URL_EXPIRY`     | `3600`                       | No                              | Signed URL expiry contract for object providers      |
| `MINIO_ENDPOINT`                | none                         | Required when provider is MinIO | S3-compatible endpoint, for example `minio:9000`     |
| `MINIO_ACCESS_KEY`              | none                         | Required when provider is MinIO | MinIO/S3 access key                                  |
| `MINIO_SECRET_KEY`              | none                         | Required when provider is MinIO | MinIO/S3 secret key                                  |
| `MINIO_BUCKET`                  | `agentforge`                 | No                              | Object bucket                                        |
| `MINIO_USE_SSL`                 | `false`                      | No                              | Use HTTPS for MinIO/S3 access                        |
| `MINIO_REGION`                  | none                         | No                              | Optional S3 region                                   |

Compose overrides the local-path default to `/var/lib/agentforge/uploads` and
mounts the `agentforge-uploads` named volume there so the Rust API can keep a
read-only root filesystem. When `STORAGE_PROVIDER=minio`, set the three required
MinIO values and start the `storage` profile if MinIO is managed by this stack.

## Rust Orchestrator Variables

| Variable                          | Default                                    | Required                          | Purpose                                                                              |
| --------------------------------- | ------------------------------------------ | --------------------------------- | ------------------------------------------------------------------------------------ |
| `ORCHESTRATOR_PORT`               | `4010`                                     | No                                | Orchestrator listen port                                                             |
| `ORCHESTRATOR_HOST`               | `0.0.0.0`                                  | No                                | Orchestrator bind host                                                               |
| `ORCHESTRATOR_DATABASE_URL`       | empty                                      | Yes in live mode                  | Orchestrator PostgreSQL connection                                                   |
| `ORCHESTRATOR_LOG_LEVEL`          | `info`                                     | No                                | Tracing filter                                                                       |
| `ORCHESTRATOR_INTERNAL_TOKEN`     | none                                       | Recommended                       | Internal auth for live workflow and service-to-service calls                         |
| `ORCHESTRATOR_JWT_SIGNING_KEY`    | none                                       | Optional                          | JWT auth mode signing key; if set, must be valid hex and decode to at least 32 bytes |
| `ORCHESTRATOR_MCP_ENDPOINT`       | `http://localhost:4003/mcp`                | No                                | Rust API MCP endpoint used by workflow activities                                    |
| `ORCHESTRATOR_MCP_TOKEN`          | empty                                      | Required when Temporal is enabled | Shared token for internal MCP calls                                                  |
| `ORCHESTRATOR_TEMPORAL_ENABLED`   | `false` in code, `true` in default Compose | No                                | Enables live Temporal runtime                                                        |
| `ORCHESTRATOR_TEMPORAL_HOST`      | `localhost:7233`                           | No                                | Temporal frontend address                                                            |
| `ORCHESTRATOR_TEMPORAL_NAMESPACE` | `orchestrator`                             | No                                | Temporal namespace                                                                   |
| `ORCHESTRATOR_OPENSEARCH_ENABLED` | `false`                                    | No                                | Enables OpenSearch-backed knowledge search                                           |
| `ORCHESTRATOR_OPENSEARCH_URL`     | `http://localhost:9200`                    | No                                | OpenSearch endpoint                                                                  |
| `ORCHESTRATOR_EMBEDDING_API_URL`  | empty                                      | Optional                          | Embedding provider endpoint                                                          |
| `ORCHESTRATOR_EMBEDDING_API_KEY`  | empty                                      | Optional                          | Embedding provider secret                                                            |
| `ORCHESTRATOR_EMBEDDING_MODEL`    | `text-embedding-3-small`                   | No                                | Embedding model name                                                                 |

## Internal MCP and Container Runtime Variables

| Variable                      | Default                                    | Purpose                                                    |
| ----------------------------- | ------------------------------------------ | ---------------------------------------------------------- |
| `MCP_ENABLED`                 | `false` in code, `true` in default Compose | Enables the Rust API internal MCP bridge                   |
| `MCP_TOKEN`                   | none                                       | Shared token for trusted MCP callers                       |
| `AGENTFORGE_WORKSPACE_ROOT`   | `/data/agentforge/workspaces`              | Root path for managed workspaces                           |
| `CONTAINER_AGENT_IMAGE`       | `agentforge-agent:latest`                  | Default agent image                                        |
| `CONTAINER_IMAGE_CLAUDE`      | falls back to default image                | Claude-specific agent image                                |
| `CONTAINER_IMAGE_OPENCODE`    | empty                                      | OpenCode-specific image override                           |
| `CONTAINER_IMAGE_CODEX`       | empty                                      | Codex-specific image override                              |
| `CONTAINER_IMAGE_GEMINI`      | empty                                      | Gemini-specific image override                             |
| `CONTAINER_ANTHROPIC_API_KEY` | empty                                      | Injected provider credential for Claude agent images       |
| `CONTAINER_OPENAI_API_KEY`    | empty                                      | Injected provider credential for Codex/OpenAI agent images |
| `CONTAINER_GOOGLE_API_KEY`    | empty                                      | Injected provider credential for Gemini agent images       |

Public releases publish pre-built `agent-base`, `agent-opencode`,
`agent-codex`, and `agent-gemini` images to
`ghcr.io/wisdoverse/wisdoverse-forge`. Run `make update-agents` to pull and tag
them locally as `agentforge-agent:<tool>`. `agent-base` follows
`GHCR_IMAGE_TAG` (`main` by default), while the redistributable CLI overlay
images use `latest`. Public releases intentionally exclude `agent-claude`; build
it locally with `make build-agent CLI_TOOL=claude` after accepting the vendor
terms, or pull it from a private registry only when your third-party terms
permit redistribution.

When `MCP_ENABLED=true`, Docker must be available to the Rust API service.

## CLI Agent Image Updater Variables

These variables control the background CLI agent-image auto-updater. When it is
enabled, a worker periodically pulls newer Container CLI overlay images so newly
spawned agents start on the current CLI. Running agents are never touched; only
the image the next spawn resolves is refreshed.

Prerequisites: set `CLI_IMAGE_AUTO_UPDATE_ENABLED=true` and make Docker available
to the Rust API service (the same requirement as `MCP_ENABLED=true`). The updater
is deployment-global and has no tenant scope, because image state is per host.

| Variable                              | Default                               | Purpose                                                                                      |
| ------------------------------------- | ------------------------------------- | -------------------------------------------------------------------------------------------- |
| `CLI_IMAGE_AUTO_UPDATE_ENABLED`       | `false`                               | Enables the background CLI agent-image auto-updater                                          |
| `CLI_IMAGE_AUTO_UPDATE_INTERVAL_SECS` | `900`                                 | Registry poll interval in seconds (15 min); clamped to a 60-second minimum                   |
| `CLI_IMAGE_PRUNE_ENABLED`             | `false`                               | Prunes superseded dangling agent overlays after each sweep; only runs when auto-update is on |
| `AGENT_REGISTRY`                      | `ghcr.io/wisdoverse/wisdoverse-forge` | Registry base the updater pulls overlays from, as `${AGENT_REGISTRY}/agent-<tool>:<tag>`     |
| `AGENT_CLI_IMAGE_TAG`                 | `latest`                              | Image tag the updater tracks, used as the `<tag>` in the remote ref above                    |

Success looks like newly spawned agents picking up the current CLI overlay
without an operator running `make update-agents` by hand. Confirm status at the
admin-only `GET /api/v1/admin/cli-images` endpoint, which surfaces the resolved
`registry`, `imageTag`, and `pollIntervalSecs`, plus per-tool digests and prune
counters (JSON is camelCase).
`claude` is excluded from the poll set because it has no public registry image.

When `CLI_IMAGE_PRUNE_ENABLED=true`, the prune pass runs inside the updater loop
and is image-level only: it removes solely the deployment's own dangling agent
overlays that no running or stopped container references, and never touches
containers. See `docs/guides/cli-image-auto-update.md` for the full operator
guide, including the operator-initiated image roll.

## Compose-Level Deployment Variables

| Variable                     | Typical Use                                                                                                                  |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `AGENTFORGE_PORT`            | Host port mapping for the Rust API                                                                                           |
| `AGENTFORGE_HOST_PORT`       | Optional host-only override for the Rust API published port; the container still listens on `4003`                           |
| `ORCHESTRATOR_PORT`          | Host port mapping for the Rust orchestrator                                                                                  |
| `BIND_ADDRESS`               | Bind scope for published ports                                                                                               |
| `CONTAINER_NAME_PREFIX`      | Optional Compose container-name prefix for running an isolated second stack on the same host                                 |
| `APP_HOST`                   | Public host served by Caddy in the self-contained `prod` profile                                                             |
| `HTTP_PORT` / `HTTPS_PORT`   | Public Caddy port mappings for the `prod` profile; non-443 HTTPS ports are included in generated `APP_URL` and `CORS_ORIGIN` |
| `POSTGRES_PASSWORD`          | Internal PostgreSQL password for dev or prod profiles                                                                        |
| `REDIS_PASSWORD`             | Internal Redis password for dev or prod profiles                                                                             |
| `NATS_BACKEND_PASSWORD`      | Backend NATS user password                                                                                                   |
| `NATS_AUTH_SERVICE_PASSWORD` | NATS auth callout service password                                                                                           |
| `NATS_SYS_PASSWORD`          | SYS-account password for monitoring and targeted KICK                                                                        |
| `NATS_CALLOUT_*`             | Issuer, account-signing, and XKey material for NATS auth callout                                                             |
| `MCP_TOKEN`                  | Shared token used by the Rust API and orchestrator                                                                           |
| `EXTERNAL_NETWORK`           | External Docker network name for the `external` profile                                                                      |
| `STORAGE_PROVIDER`           | Attachment object storage provider                                                                                           |
| `STORAGE_LOCAL_PATH`         | Writable mount path for local attachment storage                                                                             |
| `MINIO_*`                    | MinIO/S3 settings when using the `storage` profile                                                                           |

## Guidance

- Keep `MCP_TOKEN`, `ORCHESTRATOR_MCP_TOKEN`, and `ORCHESTRATOR_INTERNAL_TOKEN` aligned in trusted deployments.
- Use separate database URLs or logical databases for the Rust API and orchestrator domains.
- Keep `STORAGE_LOCAL_PATH` aligned with the Compose volume mount when using local attachment storage.
- Treat all secrets as deployment-managed values; do not hardcode them in scripts or examples.
- If you add or rename a runtime variable, update this guide and the corresponding startup or deployment doc in the same change.
