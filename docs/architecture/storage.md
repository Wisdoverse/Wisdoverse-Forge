# Storage Architecture

This document describes the primary storage systems in the current Rust-first runtime.

## Storage Inventory

| Store                           | Scope                               | Primary Users                                                        |
| ------------------------------- | ----------------------------------- | -------------------------------------------------------------------- |
| Application PostgreSQL          | Main product data                   | Rust API, user/admin domains, agent/event data                       |
| Orchestrator PostgreSQL         | Orchestration domain data           | Rust orchestrator tasks, reviews, teams, workflows, audit, knowledge |
| Attachment object storage       | Uploaded file bytes                 | Rust API attachment service, agent prompt/file workflows             |
| Redis                           | Optional cache and coordination     | Rust API and supporting services                                     |
| NATS                            | Event transport                     | Runtime producers, Rust jobs consumers, realtime paths               |
| Docker volumes / workspace root | Agent workspaces and runtime files  | MCP-backed agent execution                                           |
| Browser local storage           | User preferences and local UI state | Frontend                                                             |

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
- `STORAGE_PROVIDER=minio` stores bytes in the configured MinIO/S3-compatible
  bucket and requires `MINIO_ENDPOINT`, `MINIO_ACCESS_KEY`, and
  `MINIO_SECRET_KEY`.

Downloads are proxied by the Rust API so authorization and tenant checks remain
in the application layer.

## Workspace Storage

Agent execution uses `AGENTFORGE_WORKSPACE_ROOT` as the managed workspace root.
Container CLI agents mount the selected workspace's projects root at
`/workspace`. `agents.workspace_id` is the filesystem access boundary, while
`agents.project_id` is the primary project context for task routing and UI
ownership. Tool-specific agent images and injected provider credentials are
configured through `CONTAINER_*` environment variables.

## Browser Storage

The frontend may keep local presentation state, preferences, and view selections in browser storage. This data is user-local and is not the source of truth for backend state.

## Guidance

- Use migrations and repositories as the canonical schema documentation.
- Do not add new default-path schema changes to legacy TypeScript migration trees.
- If a change introduces or retires a storage dependency, update this document and the deployment/configuration guides in the same change.
