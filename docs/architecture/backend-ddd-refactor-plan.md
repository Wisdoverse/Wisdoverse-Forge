# Backend DDD Refactor Handoff

Last updated: 2026-05-23

## Current State

The backend DDD refactor is substantially complete. Previously identified
aggregate boundaries have been consolidated, route-level persistence leaks have
been moved behind service/repository boundaries, and the Rust boundary harness
now guards the route, service, repository, MCP, gateway, and system entrypoint
layers.

Current mainline state:

- PRs #211-#217 grouped repositories under aggregate-owned modules for
  context candidates, resources, credentials, orchestration, agents, users, and
  identity. Single-table repositories remain flat.
- PRs #225-#229 moved credential, configuration, LLM provider, legacy
  navigation, orchestration, event, turn, and observability projections into
  domain/service boundaries.
- PR #289 moved auth context switching out of the route layer into
  AuthService, repository methods, and domain-owned switch-context policy and
  response types.
- PR #290 kept production group routes on the existing resource-domain path and
  centralized the test-support group navigation query in the group repository.

The durable design references are:

- [DDD contract](ddd-contract.md) for the route -> service -> domain ->
  repository layering rules.
- [Aggregate catalog](aggregate-catalog.md) for repository grouping and aggregate
  ownership.
- [ADR index](../adr/README.md), especially ADR-0002 for DDD layering,
  ADR-0004 for tenant scope, ADR-0005 for aggregate grouping, and ADR-0006 for
  SQLx migration policy.
- [Threat model](../security/threat-model.md) for security-sensitive runtime
  boundaries.
- [Observability SLO runbook](../runbooks/observability-slo.md) for operating
  the refactored backend.

## Execution Rule

Treat future backend DDD work as maintenance, not a reason to keep splitting
one endpoint at a time. Before creating another refactor PR:

- verify the finding is a real layer violation, not sanctioned response envelope
  styling around domain-owned data;
- scope the change to one route family, aggregate, or boundary rule;
- keep routes limited to HTTP extraction, auth scope usage, and service calls;
- keep services responsible for repository I/O, transactions, runtime
  orchestration, and adapters;
- keep domain modules responsible for response/projection types, pure policies,
  validators, audit-event constructors, and protocol projections;
- keep repository modules grouped by aggregate when multiple tables form one
  root.

## Remaining Hygiene

Known non-blocking cleanup candidates:

- Some routes still wrap domain-owned data with the existing
  `{ ok: true, ...data }` JSON envelope. This is allowed where the surface
  already uses that contract. Promote to named domain envelopes only when a
  future change needs cross-route consistency, pagination metadata, or a shared
  response policy.
- Test-support helpers may expose repository-backed fixtures for integration
  tests. Keep them behind `#[cfg(any(test, feature = "test-support"))]` and keep
  production code free of route-level repository imports.

No new production direct-SQL or route-owned repository factory work should be
introduced. The boundary harness should fail if it returns.

## PR And Security Gate Handling

For each backend DDD PR:

1. Rebase or merge against current `origin/main` before push.
2. Run the narrow local Rust tests that match the changed boundary, then run the
   boundary harness when route/service/repository ownership changes.
3. Run clippy with warnings denied for Rust API changes.
4. Wait for GitHub CI and security gates before merging: Rust Tests, Unit Tests,
   Integration Tests, CodeQL, Dependency Audit, Dangerous Pattern Scan, Secret
   Leak Scan, and Trivy Filesystem Scan.
5. If branch policy blocks a fully green PR only for review requirements and the
   user has asked for merge handling, attempt the normal merge first and use the
   repository's admin merge path only after the normal merge is rejected by
   policy.
6. After a PR merges, delete only clean local worktrees/branches whose commits
   are merged or patch-equivalent to `origin/main`. Preserve dirty worktrees and
   branches with unique commits.

## Validation

Choose checks by changed surface. For backend DDD batches, run at least:

```bash
cd rust && cargo fmt --all
cd rust && cargo test -p agentforge-api --lib <narrow-module-filter>
cd rust && cargo test -p agentforge-api --test route_ddd_boundary_test
cd rust && cargo clippy -p agentforge-api --lib --tests -- -D warnings
git diff --check
```

For shared crates, API contracts, orchestration, auth, DB, platform security, or
runtime behavior, run:

```bash
cd rust && make ci
```

If local SQLx tests require `DATABASE_URL` and it is unavailable, document the
exact skipped command and rely on GitHub CI for DB-backed tests.

## Claude Code Prompt

```text
Continue the backend DDD refactor in Wisdoverse Forge.

Repository:
/data/agentforge/workspaces/orgs/703d9f89-c057-4bd4-8938-96373593bf50/workspaces/7fa557f2-4223-4093-8a11-9bfe22be6d18/projects/wisdoverse-forge

Read AGENTS.md first and follow it exactly. Backend ownership is Rust under
rust/. Do not add backend behavior to legacy TypeScript server paths.

The backend DDD refactor is substantially complete. Before opening another DDD
PR, read docs/architecture/ddd-contract.md,
docs/architecture/aggregate-catalog.md, docs/adr/README.md,
docs/security/threat-model.md, and docs/runbooks/observability-slo.md.

Verify that the target is a real layer violation, not sanctioned envelope
styling. If a real violation remains, scope it to one route family, aggregate,
or boundary rule; implement it in a separate worktree from current origin/main;
run focused Rust validation, the boundary harness, clippy, and diff check; push
the PR; wait for CI/security gates; and merge only after checks pass. Preserve
unrelated user changes and do not revert anything outside the batch.
```
