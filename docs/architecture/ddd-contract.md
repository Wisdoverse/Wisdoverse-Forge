# Backend DDD Layer Contract

This document is the concrete rulebook for the four-layer split that
[ADR 0002](../adr/0002-ddd-layering.md) establishes. It is intended to be
useful in code review: every section ends with a "must" / "must not" list and
points at a representative file in the current codebase.

Scope: the API crate at `rust/crates/api`. The orchestrator and platform crates
follow the same general direction but have their own internal layering that is
out of scope for this document.

## Layers at a Glance

```text
HTTP request
   |
   v
+------------------------+
| routes/<topic>.rs      |  HTTP extraction, scope, service call, response
+------------------------+
            |
            v
+------------------------+
| services/<topic>.rs    |  Orchestration, transactions, repo I/O,
|                        |  cross-aggregate composition
+------------------------+
        /        \
       v          v
+--------------+  +------------------------+
| domain/      |  | repositories/<topic>/  |
| <topic>.rs   |  | (or <topic>.rs)        |
| projections, |  | tenant-scoped SQL,
| validators,  |  | entity persistence
| policies     |  |
+--------------+  +------------------------+
                              |
                              v
                       PostgreSQL
```

Import direction is strictly downward. Routes may import services and domain.
Services may import domain and repositories. Repositories may import domain
(for projection types) and `agentforge_core` / `agentforge_db`. Domain depends
only on `agentforge_core`, `agentforge_db::entities` (read-only), and standard
crates (`serde`, `chrono`, `uuid`, …).

## `routes/`

### Owns

- HTTP method, path, status code selection.
- Request DTO deserialization (`#[derive(Deserialize)]` request structs).
- Auth scope extraction via the `AuthUser` extractor.
- One service call per logical operation.
- Translation of the typed service result into a JSON response wrapped in
  `{ "ok": true, … }`.

### Must Not

- Talk to SQLx directly. Importing `sqlx::query`, `sqlx::query_as`,
  `sqlx::PgPool`, or holding `state.pool` is a layer violation.
- Define inline response shape structs (`#[derive(Serialize)] struct
XResponse`). Promote to `domain/<topic>.rs`.
- Encode business rules ("a project requires a workspace"). Promote to a
  domain value type or service policy.
- Construct `TenantScope` by hand or modify one.
- Use `.unwrap()` or `.expect()` on fallible operations. Use `?` against
  `AppResult`. `clippy::unwrap_used` is denied in handler code.

### Returns

`axum::response::Response`, `axum::Json<T>` where `T: Serialize`, or
`agentforge_core::AppResult<Json<T>>`.

### Example

`rust/crates/api/src/routes/auth.rs::switch_context` after PR #289 — the entire
handler is a request parse, an axes constructor call, a service call, a cookie
header build, and a domain response. ~25 lines including error mapping.

### Anti-Pattern

A 78-line `validate_switch_context_axes` helper that runs three SQL queries
against `&state.pool` from inside `routes/auth.rs`. This was the pre-#289 shape
and is now disallowed.

## `services/`

### Owns

- Orchestration of multiple repository calls.
- Transaction boundaries (`pool.begin().await?` / `tx.commit().await?`).
- `From<Entity>` and `From<RepositoryRow>` adapters that map persistence
  shapes to domain projections.
- Side effects: NATS publishes, MCP bridge calls, email sends, audit-event
  emission, Temporal signals.
- Validation that requires looking at the database (uniqueness, cross-axis
  authorization, FK existence).
- Re-exports of domain types so routes can `use crate::services::<topic>::T`
  rather than `use crate::domain::<topic>::T`. This keeps the route's import
  block focused on the service boundary.

### Must Not

- Run SQL directly. Always go through a repository method.
- Construct HTTP responses or return `axum::Response` types. Return domain
  or entity types.
- Return `serde_json::Value`. The wire shape is the route's job.

### Returns

`AppResult<DomainType>` or `AppResult<()>`.

### Example

`rust/crates/api/src/services/auth.rs::AuthService::switch_context` — orchestrates
four repositories (`OrganizationRepository`, `TeamRepository`,
`WorkspaceRepository`, `ProjectRepository`) and the JWT manager to mint a new
token pair. Returns `SwitchContextResult`, a typed struct from
`domain::auth`.

### Anti-Pattern

A service method that builds a `serde_json::Value` and returns it. Routes then
have nothing to map and reviewers cannot see the wire shape at the service
boundary.

## `domain/`

### Owns

- Validated value types (`UserEmail::parse`, `UserPassword::parse`,
  `SwitchContextAxes::new`, `LicenseKey::parse`). The constructor is the
  policy gate.
- Pagination policies (`UserListPage::new`, `ResourceListPage::new` that
  clamp limits and floor offsets).
- Response/projection shapes that the wire format depends on (`#[derive(
Serialize)]` with explicit `#[serde(rename_all = "camelCase")]`).
- Audit-event constructors and protocol projections.
- Pure policy logic that does not reach repositories.

### Must Not

- Be async. No `async fn` lives in domain.
- Touch the database. No SQLx imports, no pool references.
- Construct HTTP responses.
- Depend on `tokio` (other than via std-compatible primitives).

### Returns

Plain values, `AppResult<DomainType>`, or `Result<DomainType, ErrorKind>`.

### Example

`rust/crates/api/src/domain/auth.rs::SwitchContextAxes::new` — constructor
that enforces the cross-axis invariant (project requires workspace) by
returning `AppResult<Self>`. Tested without spinning up an Axum router.

### Anti-Pattern

A domain module that defines an `async fn` to load a row through a repository.
The async coupling forces every domain unit test to be a `tokio::test` and
hides where the real I/O happens.

## `repositories/`

### Owns

- All SQL. Every `sqlx::query`, `sqlx::query_as`, `sqlx::query_scalar`,
  `sqlx::query_as_unchecked`, and transaction acquisition lives here.
- Entity persistence (`agentforge_db::entities::*`) and the typed query
  surface.
- Tenant scope enforcement: methods that take `&TenantScope` add the
  organization filter inside the SQL.
- Lightweight projections that are SQL-shaped (e.g. `LegacyGroupSummary` with
  `#[derive(FromRow)]`, defined in a repository row type and mapped into
  `GroupRepository::list_canonical_for_project`).

### Must Not

- Encode business rules beyond "row exists" / "row does not exist". Policy
  decisions about _what_ should happen belong in services and domain.
- Build HTTP responses or `serde_json::Value`.
- Hold or pass `TenantScope` through to non-tenant-scoped methods. A pre-auth
  method takes raw `Uuid`s and explains why in its doc comment.

### Aggregate Layout

- Multi-table aggregates: `repositories/<aggregate>/` directory with one
  submodule per concept and a `mod.rs` that re-exports the repository types.
- Single-table aggregates: a flat file `repositories/<topic>.rs`.

See [ADR 0005](../adr/0005-aggregate-grouping.md) and
[aggregate-catalog.md](aggregate-catalog.md) for the full list.

### Example

`rust/crates/api/src/repositories/identity/team.rs::TeamRepository::is_user_member`
— takes raw `Uuid`s (it is called pre-tenant-scope from `AuthService`), runs a
single `EXISTS` query, and returns `AppResult<bool>`. The doc comment states
why `TenantScope` is bypassed.

### Anti-Pattern

A repository method that returns `serde_json::Value` so the service "doesn't
have to map it." This blurs the layer boundary and forces every consumer to
re-parse the shape.

## Testing Expectations

Each layer has a different test profile:

| Layer      | Tests in                             | Asserts                                                                          |
| ---------- | ------------------------------------ | -------------------------------------------------------------------------------- |
| Domain     | `#[cfg(test)] mod tests` in the file | Validator boundaries, projection serialization, policy invariants.               |
| Service    | `#[cfg(test)] mod tests` in the file | Orchestration logic when it has logic; pure helpers; SQLx tests live separately. |
| Repository | `tests/*_test.rs` against a live PG  | Tenant isolation, constraint behavior, idempotency, soft-delete filters.         |
| Route      | `#[cfg(test)] mod tests` in the file | Request DTO deserialization, response shape JSON, cookie/header rendering.       |

Domain tests are the cheapest and run on every `cargo test --lib`.
Repository tests require `DATABASE_URL`; CI runs them, local devs can skip
them when iterating on domain.

## Refactoring an Existing Endpoint

When promoting an endpoint into this contract:

1. Read the route handler end to end. List every `sqlx::*`, every inline
   `Serialize` struct, every helper function that takes `&PgPool`.
2. Each `Serialize` struct moves to `domain/<topic>.rs`. Each helper function
   moves to `services/<topic>.rs` or its repository.
3. Each direct SQL call becomes a new repository method that takes
   `&TenantScope` (unless explicitly pre-auth).
4. The service exposes one method that the route calls. The service returns
   a domain type, not `serde_json::Value`.
5. The route now does: parse, call service, map result to JSON envelope.
6. Run `cargo clippy -p agentforge-api --lib --tests -- -D warnings` to
   catch collapsible-if and unwrap regressions introduced by the
   simplification.

See PR #289 (auth context switch) and PR #290 (groups project-scoped list) for
worked examples.
