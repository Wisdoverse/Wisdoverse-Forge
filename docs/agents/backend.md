# Backend And Runtime

Read this guide for Rust, APIs, data, auth, security, and agent execution.
This project permits the latest stable Rust release for toolchain upgrades.

## Runtime

The browser uses the Rust API on port `4003` through HTTP and WebSocket.
The Rust orchestrator owns Temporal workflows on port `4010`.
Agent containers use the sidecar, NATS, HTTP APIs, and the internal MCP bridge.
Their `/workspace` mount contains projects scoped to one organization and workspace.
`agents.workspace_id` controls access across projects in that workspace.
`agents.project_id` provides primary UI context and task routing.

- Keep new backend behavior out of legacy TypeScript server paths.
- Preserve this runtime path for terminal and Container CLI behavior.
- Use the [architecture overview](../architecture/overview.md) for service ownership and event flow.

## API And Data

- Register new HTTP, WebSocket, and MCP routes in `rust/crates/api/src/router.rs`.
- Require authentication through `rust/crates/api/src/middleware.rs`.
- Exempt only intentionally public infrastructure endpoints, such as `/health`.
- Require `&TenantScope` in repository methods for tenant data.
- Constrain tenant queries by organization.
- Construct tenant scope only in auth middleware.
- Where already used, preserve the `{ ok: true/false, ...data }` response format.

The API uses `route -> service -> domain -> repository` boundaries.

| Layer | Ownership |
| --- | --- |
| Domain | `Serialize` response types, projections, pure policies, audit-event constructors, and protocol projections independent of SQLx rows |
| Service | Repository I/O, transactions, `From<RepositoryRow>` adapters, and domain type re-exports through `pub use` |
| Route | HTTP handling and domain types imported through services |
| Repository | Persistence operations within aggregate boundaries |

- When multiple tables form one root, group their repositories by aggregate.
- Add tables for an existing aggregate as submodules of that aggregate.
- Re-export these submodules from the aggregate's `mod.rs`.
- Keep repositories for single tables flat.
- For a new API module, register its modules and route.
- Add auth and tenant tests for that module.
- Use domain response types through service re-exports.

| Error layer | Mechanism |
| --- | --- |
| Domain | `thiserror` |
| Infrastructure | `anyhow` context |
| HTTP | `AppError::IntoResponse` |

- Keep internal error details out of client responses.
- Use typed errors and explicit HTTP mappings in handlers.
- Keep `clippy::unwrap_used` denied in handler code.

## Database And Queue

- Add migrations under `rust/crates/db/migrations/`.
- Add a corrective migration instead of editing a migration already applied in production.
- When production drift requires tolerance, make the migration idempotent.
- When adopting legacy tables, keep a schema-contract test for fresh databases and production.
- For database changes, update entity structs and repository queries.
- Do tests for tenant boundaries in those changes.
- Use `FOR UPDATE SKIP LOCKED` for PostgreSQL queue claims.
- Treat `pg_notify` as a wake-up signal only.
- Keep a polling fallback for queue work.

## Security

- Authenticate WebSocket connections with the JWT from `?token=`.
- Validate WebSocket origins against configured CORS origins.
- Reject arbitrary origins.
- Preserve NATS credentials, callout validation, and pub/sub permissions for each agent.
- Connect sidecars with the agent identity.
- Follow the [NATS auth runbook](../runbooks/nats-auth.md).
- Keep container security checks in `rust/crates/platform/src/security.rs`.
- Block privileged mode, host PID, Docker socket mounts, and missing resource limits.
- Keep defensive overrides in container creation.
- Apply `#[serde(skip_serializing)]` to sensitive fields.

Sensitive fields include password hashes, API keys, encrypted tokens, nonces, Stripe IDs, and equivalent secret material.

- Require `LLM_ENCRYPTION_KEY` in production.
- Keep provider secrets, encrypted payloads, and decrypted content out of logs.
- Use the existing `dev@example.com` account for test and manual login flows.
- Do not create throwaway debug accounts.

## Diagnose Before Editing

- For recent events, inspect the `events` table in descending time order.
- Correlate these events with WebSocket broadcast logs.
- For production configuration, follow the [workflow guide](workflow.md#compose-and-agent-images).
- For configuration details, read the [configuration guide](../guides/configuration.md).
