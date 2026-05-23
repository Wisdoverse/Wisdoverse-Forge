# ADR 0002 — DDD layering for the API crate

## Status

Accepted.

## Context

The API crate (`rust/crates/api`) accumulated business logic, SQL, and response
shape construction inside route handlers. This made handlers long, untestable
without HTTP plumbing, and unable to reuse domain rules across endpoints. The
recently merged refactor series (PRs #210–#228, #289, #290) extracted aggregate
families one at a time and demonstrated the target layering works at scale.

## Decision

The API crate is organized as four layers, with imports flowing one direction:

```text
routes -> services -> domain
                  \-> repositories -> domain
```

Each layer has a single responsibility:

- **`routes/`** — HTTP only. Extract request data (`Path`, `Query`, `Json`,
  `State`), pull the auth scope, call a service, map domain types into the
  response. No SQL, no validation logic, no business policy, no inline
  projection structs. The handler exists to translate between HTTP and the
  service surface.
- **`services/`** — Orchestration. Owns repository I/O, transaction boundaries,
  cross-aggregate composition, fan-out to infrastructure (NATS, MCP bridge,
  email), and `From<RepositoryRow>` adapters. Services return domain types,
  not `serde_json::Value`.
- **`domain/`** — Pure business policy and projections. Validated value types
  (`UserEmail::parse`, `SwitchContextAxes::new`), audit-event constructors,
  protocol projections, `Serialize`-derived response shapes, and domain
  errors. No async, no SQL, no transport.
- **`repositories/`** — Tenant-scoped SQL. Each method takes `&TenantScope`
  unless it is explicitly documented as a pre-auth query (see
  [ADR 0004](0004-tenant-scope-pattern.md)). Repositories return entities or
  domain projections.

Domain types may be re-exported by services via `pub use crate::domain::<topic>`
so that routes can consume them through the service path without depending
directly on `domain::`.

## Consequences

- Handlers stay short (the auth context-switch handler shrank from 86 lines
  to 25 after #289). Reviewers can see the full HTTP surface at a glance.
- Domain rules are unit-testable without spinning up Axum or a database.
- Cross-aggregate workflows (e.g. session context switch) live in services,
  not buried in route helpers, so they show up in the service catalog.
- Every new endpoint costs one route handler, one service method, and at most
  one domain type or repository method. The four-file diff is the indicator
  the layering is being applied.
- `clippy::unwrap_used` is denied in handler code; surface area for panics
  shrinks.

## References

- `docs/architecture/ddd-contract.md` — concrete per-layer rules and
  anti-patterns.
- `docs/architecture/aggregate-catalog.md` — current aggregates and their
  module paths.
- `docs/architecture/backend-ddd-refactor-plan.md` — in-flight migration
  status.
- `AGENTS.md` — "Backend Contracts" section.
- PR series #210–#228, #289, #290 — concrete examples of the refactor.
