# Architecture Decision Records

This directory captures architecturally significant decisions for Wisdoverse
Forge. ADRs are the durable record of _why_ the system looks the way it does;
day-to-day truth still lives in the code, `AGENTS.md`, and the active docs
under `architecture/`, `runbooks/`, and `guides/`.

## Format

Each ADR uses a lightweight MADR layout:

- `Status` — one of `Proposed`, `Accepted`, `Deprecated`, or `Superseded by
ADR-NNNN`.
- `Context` — the forces in play and the problem to solve.
- `Decision` — the chosen approach, stated as a present-tense rule.
- `Consequences` — what becomes easier, harder, or constrained.
- `References` — links to code, runbooks, PRs, or external standards.

## Numbering

Files are `NNNN-kebab-title.md`, allocated sequentially with leading zeros.
Numbers are never reused. A superseded ADR keeps its number and adds a
`Superseded by ADR-NNNN` line at the top while staying readable as history.

## Status Lifecycle

```text
Proposed -> Accepted -> Deprecated
                    \-> Superseded by ADR-NNNN
```

`Proposed` ADRs may be merged for review when they describe a decision the team
wants to consider but has not yet adopted. Move to `Accepted` once the decision
is the rule the codebase follows; move to `Deprecated` or `Superseded` only
when a replacement is in place and the old rule is no longer enforced.

## When to Write an ADR

Write one when the change:

- Establishes or reverses a cross-cutting architectural rule (layering,
  ownership, transport, schema policy).
- Restricts how new code may be written (e.g. "no new TS backend paths").
- Has consequences a future contributor would need explained.

Routine refactors, bug fixes, and dependency bumps do not need ADRs. The PR
description and `CHANGELOG` are the right home for those.

## Index

| ADR                                        | Status   | Title                                     |
| ------------------------------------------ | -------- | ----------------------------------------- |
| [0001](0001-rust-backend-ownership.md)     | Accepted | Rust workspace owns backend behavior      |
| [0002](0002-ddd-layering.md)               | Accepted | DDD layering for the API crate            |
| [0003](0003-three-layer-error-handling.md) | Accepted | Three-layer error handling in Rust        |
| [0004](0004-tenant-scope-pattern.md)       | Accepted | TenantScope guards tenant-scoped queries  |
| [0005](0005-aggregate-grouping.md)         | Accepted | Repository aggregate grouping convention  |
| [0006](0006-sqlx-migration-policy.md)      | Accepted | SQLx migration policy                     |
| [0007](0007-frontend-fsd-layering.md)      | Accepted | Frontend Feature-Sliced Design boundaries |
