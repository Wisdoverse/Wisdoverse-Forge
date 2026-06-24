# ADR 0001 — Rust workspace owns backend behavior

## Status

Accepted.

## Context

Wisdoverse Forge originated with a TypeScript backend. The Rust workspace under
`rust/` was introduced as the new control plane and orchestrator. During the
transition, behavior briefly lived in both trees, which created drift,
duplicated tests, and ambiguous review ownership.

## Decision

All new backend behavior lives in the Rust workspace:

- The active control plane is `agentforge-server` (`rust/bins/server`) on port
  `:4003`.
- Workflow orchestration is `agentforge-orchestrator` (`rust/bins/orchestrator`)
  on port `:4010`.
- Legacy TypeScript server paths are frozen and do not receive new features,
  bug fixes (other than removal), or dependency upgrades. Old paths that still
  exist are read-only references for migration only.

PRs that introduce backend behavior outside the Rust workspace are rejected at
review.

## Consequences

- One language, one toolchain, one test runner (`cargo`) for backend changes.
- Backend code review concentrates on the Rust workspace; frontend reviewers
  do not need to track TS backend modules.
- Backend developers must be comfortable with Rust idioms (`Result<T, E>`,
  ownership, async/await on Tokio).
- Cross-cutting frontend/backend contracts pass through `shared/types/` and the
  generated proto output to keep TS consumers in sync.

## References

- `AGENTS.md` — "Current Runtime Contract" and "Repository Map" sections.
- `docs/architecture/overview.md` — system context and service inventory.
- `docs/runbooks/runtime-validation.md` — proofed runtime boundary.
