# Storage Architecture

This document describes the primary storage systems in the current Rust-first runtime.

## Storage Inventory

| Store                           | Scope                                              | Primary Users                                                        |
| ------------------------------- | -------------------------------------------------- | -------------------------------------------------------------------- |
| Application PostgreSQL          | Main product data                                  | Rust API, user/admin domains, agent/event data                       |
| Orchestrator PostgreSQL         | Orchestration domain data                          | Rust orchestrator tasks, reviews, teams, workflows, audit, knowledge |
| Attachment object storage       | Uploaded file bytes                                | Rust API attachment service, agent prompt/file workflows             |
| Redis                           | Optional cache and coordination                    | Rust API and supporting services                                     |
| NATS                            | Event transport                                    | Runtime producers, Rust jobs consumers, realtime paths               |
| Docker volumes / workspace root | Agent workspaces and runtime files                 | MCP-backed agent execution                                           |
| Browser local storage           | Auth access token, cached user, and local UI state | Frontend                                                             |

## PostgreSQL Domains

Exact schemas should be taken from migrations and entity definitions, not from hand-maintained table snapshots.

| Domain                       | Source of Truth                                                             |
| ---------------------------- | --------------------------------------------------------------------------- |
| Rust API data model          | `rust/crates/db/migrations/` and `rust/crates/db/src/entities.rs`           |
| Rust orchestrator data model | `rust/crates/orchestrator/migrations/` and orchestrator repositories/models |

## Redis

Redis is optional in the Rust stack. When configured, it is used for cache and coordination features. If it is unavailable, the system should degrade without blocking the entire platform.

## NATS

NATS is the event transport backbone for runtime event publication and consumption. It is not the system of record; PostgreSQL remains the durable source for persisted domain state.

## Attachment Object Storage

Attachment metadata lives in Application PostgreSQL. File bytes are stored
through the Rust API object-storage client:

- `STORAGE_PROVIDER=local` stores bytes under `STORAGE_LOCAL_PATH`. Compose
  mounts the `agentforge-uploads` named volume at that path for production
  profiles so the API root filesystem can remain read-only.
- `STORAGE_PROVIDER=s3` stores bytes in the configured S3-compatible bucket.
  RustFS is the managed Compose service; configure `S3_ENDPOINT`,
  `S3_ACCESS_KEY`, `S3_SECRET_KEY`, `S3_BUCKET`, and `S3_REGION`.
- `STORAGE_PROVIDER=minio` and `MINIO_*` remain compatibility aliases for
  existing deployments. When both naming schemes are set, `S3_*` takes
  precedence; new configurations should use only `S3_*`.

The Rust API uses its existing S3 client with path-style requests. Its API
proxies downloads so authorization and tenant checks remain in the application
layer; browsers and agents do not connect directly to S3/RustFS.

See the [RustFS migration guide](../guides/rustfs-migration.md) before moving
existing MinIO objects. MinIO and RustFS data volumes are separate and cannot
be exchanged as raw disks.

## Workspace Storage

Agent execution uses `AGENTFORGE_WORKSPACE_ROOT` as the managed workspace root.
Container CLI agents mount the selected workspace's projects root at
`/workspace`. `agents.workspace_id` is the filesystem access boundary, while
`agents.project_id` is the primary project context for task routing and UI
ownership. Tool-specific agent images and injected provider credentials are
configured through `CONTAINER_*` environment variables.

## Browser Storage

The frontend stores the access JWT in `af:auth:access` and the cached user's
email, organization ID, and role in `af:auth:user` in browser local storage.
The refresh token is held separately in the `af_rt` cookie, which is `HttpOnly`
and `SameSite=Strict`. Refresh requests issue a new access token and retain the
existing refresh cookie. Browser local storage is readable by page JavaScript;
it is not the source of truth for backend state.

## Guidance

- Use migrations and repositories as the canonical schema documentation.
- Do not add new default-path schema changes to legacy TypeScript migration trees.
- If a change introduces or retires a storage dependency, update this document and the deployment/configuration guides in the same change.
