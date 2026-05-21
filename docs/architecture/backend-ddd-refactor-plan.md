# Backend DDD Refactor Handoff

Last updated: 2026-05-21

## Current State

The backend DDD refactor is in progress. The goal is not complete yet.

Merged PRs:

- #225 moved credential response projections into `rust/crates/api/src/domain/credential.rs`.
- #226 moved runtime and gateway settings projections and validation into
  `rust/crates/api/src/domain/configuration.rs` and service boundaries.
- #227 moved LLM provider configuration orchestration out of routes into
  `rust/crates/api/src/services/llm_provider.rs`,
  `rust/crates/api/src/repositories/user/llm_config.rs`, and
  `rust/crates/api/src/domain/credential.rs`.
- #228 moved legacy navigation behavior into DDD layers.

The main branch is expected to contain merge commit
`364d5a2` (`refactor: move legacy navigation into ddd layers (#228)`) or newer
before continuing.

Current stacked PRs:

- #229 `refactor/backend-ddd-orchestration-events` -> `main`: moved
  orchestration, event, turn, and observability response projections into
  domain modules. GitHub CI was green when checked; merge was blocked only by
  review policy.
- #230 `refactor/backend-ddd-admin-console` -> #229 branch: moved admin agent
  projections and response assembly into domain/service layers.
- #231 `refactor/backend-ddd-skill-resource-contracts` -> #230 branch: moved
  skill/resource response contracts behind domain helpers and service re-exports.
- #232 `refactor/backend-ddd-tenant-resource-crud` -> #231 branch: moved
  organization, workspace, team, project, and group CRUD response contracts,
  permission orchestration, default project-group creation, and project-scoped
  group SQL out of routes.
- #233 `refactor/backend-ddd-config-governance` -> #232 branch: moved feature
  flag, settings, quota, license, audit, billing, and governance-audit response
  contracts, projections, typed inputs, and export orchestration into
  domain/service boundaries.
- #234 `refactor/backend-ddd-identity-access` -> #233 branch: moved user, API
  key, SSH key, Git credential, Container CLI credential, and CLI auth proxy
  response contracts, permission checks, token encryption, provider resolution,
  and legacy upsert defaults into domain/service boundaries.

## Execution Rule

Do not continue with tiny single-function or single-response slices. Continue in
larger, coherent batches that migrate a full route family or aggregate boundary
at once.

Each batch should produce one PR with:

- route handlers reduced to HTTP extraction, auth scope usage, and service calls;
- service modules owning orchestration, transactions, repository I/O, and adapters;
- domain modules owning response/projection types, pure policies, validators, and
  audit-event or protocol projection constructors;
- repository modules grouped by DDD aggregate where multiple tables form one root;
- focused tests for domain policy and service behavior plus existing contract or
  SQLx tests that protect API behavior.

## Next Efficient Batches

Do not pick a one-endpoint cleanup. Pick one of these larger batches and finish
the route family end to end.

### Batch 1: Agent Execution Runtime

Target the high-value runtime path in one PR:

- `routes/agents.rs`
- `routes/containers.rs`
- `routes/pools.rs`
- `routes/dev_environments.rs`

Move response contracts, status projections, command/control policy, and runtime
state mapping into domain/service modules. Keep Docker/container side effects and
pool orchestration in services or platform-facing adapters, not routes.

### Batch 2: Collaboration And Knowledge Surfaces

Target user-facing content/workflow surfaces as a larger aggregate batch:

- `routes/context.rs`
- `routes/memory.rs`
- `routes/prompts.rs`
- `routes/plugins.rs`
- `routes/attachments.rs`
- `routes/favorites.rs`
- `routes/tiles.rs`
- `routes/inbox.rs`

Move projection types, response helpers, tenant/user policy, and repository
adapters into the owning aggregate. If the batch becomes too large, split by
aggregate family, not by individual endpoint.

## Validation

Choose checks by changed surface. For backend DDD batches, run at least:

```bash
cd rust && cargo fmt --all
cd rust && cargo test -p agentforge-api --lib <narrow-module-filter>
cd rust && cargo clippy -p agentforge-api --lib --tests -- -D warnings
git diff --check
```

When the batch touches shared crates, API contracts, orchestration, auth, DB, or
platform security, run:

```bash
cd rust && make ci
```

If local SQLx tests require `DATABASE_URL` and it is not available, document the
exact skipped command and rely on GitHub CI for DB-backed tests.

## Claude Code Prompt

```text
Continue the backend DDD refactor in Wisdoverse Forge.

Repository:
/data/agentforge/workspaces/orgs/703d9f89-c057-4bd4-8938-96373593bf50/workspaces/7fa557f2-4223-4093-8a11-9bfe22be6d18/projects/wisdoverse-forge

Read AGENTS.md first and follow it exactly. Backend ownership is Rust under
rust/. Do not add backend behavior to legacy TypeScript server paths.

Work efficiently. Do not create tiny single-function or single-response PRs.
Pick a full coherent route family or aggregate boundary and complete it in one
batch. Keep route -> service -> domain -> repository:

- routes: HTTP extraction, auth scope usage, and service calls only;
- services: repository I/O, transactions, orchestration, and adapters;
- domain: pure policies, validators, response/projection types, and audit or
  protocol constructors;
- repositories: tenant-scoped SQL grouped by DDD aggregate.

Current merged state:
- PR #225 credential response projections merged.
- PR #226 settings/configuration projections and validation merged.
- PR #227 LLM provider config service/repository/domain split merged at
  93d40eb7433952c6c85f304a27d67372f2864ff9.
- PR #228 legacy navigation DDD split merged at 364d5a2.

Current open stack:
- #229 orchestration/events/turn projections, base main.
- #230 admin console projections, stacked on #229.
- #231 skill/resource response contracts, stacked on #230.
- #232 tenant resource CRUD, stacked on #231.
- #233 product configuration and governance, stacked on #232.
- #234 identity and access surfaces, stacked on #233.

Before starting a new PR, inspect the current state of #229-#234. If they have
not landed yet, stack the next branch on #234. If they have landed, branch from
updated origin/main.

Create a separate worktree, implement the next large backend DDD batch, run
focused Rust validation plus clippy, push a PR, wait for CI, and merge only
after checks pass. Preserve unrelated user changes and do not revert anything
outside the batch.
```
