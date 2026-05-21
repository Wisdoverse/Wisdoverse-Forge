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
- #235 `refactor/backend-ddd-agent-runtime` -> #234 branch: moved agent,
  container, pool, and development-environment response contracts, permission
  projections, restart lifecycle policy, and runtime status helpers into
  domain/service boundaries.
- #236 `refactor/backend-ddd-collaboration-knowledge` -> #235 branch: moved
  context, memory, prompt, plugin, attachment, favorite, tile, and inbox
  response contracts and inbox repository/service/projection boundaries into
  domain/service layers.
- #237 `refactor/backend-ddd-communication-session` -> #236 branch: moved
  auth/session response projections, context-switch membership checks and token
  issuance, analytics summaries, and voice status/response projections into
  domain/service/repository boundaries.
- #238 `refactor/backend-ddd-boundary-sweep` -> #237 branch: in progress; moves
  remaining agent message pagination and container/participant persistence
  coordination out of route handlers into domain/service boundaries.
- #239 `refactor/backend-ddd-runtime-orchestration-sweep` -> #238 branch: in
  progress; moves agent prompt runtime orchestration, sidecar command dispatch,
  provider prompt stream construction, and in-flight prompt cancellation out of
  route handlers into `AgentPromptService`.

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

### Batch 1: Completion Sweep And Boundary Enforcement

Target backend DDD completion, not another one-endpoint migration:

- audit every `rust/crates/api/src/routes/*.rs` production handler for direct
  repository construction, raw SQL, route-local response/projection structs, and
  production `json!` response construction;
- move any remaining production leaks into the owning domain/service/repository
  boundary;
- add or extend a lightweight boundary check so CI can prevent regression where
  a route reintroduces response assembly or direct SQL orchestration;
- keep request-default `json!({})` values and route tests only when they are
  clearly not production response construction.

Use the existing stacked PRs as the current working baseline. This batch should
prove whether the backend route surface now follows the intended DDD boundary
and close concrete gaps found by the audit. Continue #239 by preferring grouped
production runtime orchestration leaks over test-only fixture cleanup.

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
- #235 agent execution runtime surfaces, stacked on #234.
- #236 collaboration and knowledge surfaces, stacked on #235.
- #237 communication, analytics, and session surfaces, stacked on #236.
- #238 boundary sweep, stacked on #237, currently moving remaining production
  route persistence leaks into service/domain boundaries.
- #239 runtime orchestration sweep, stacked on #238, currently moving prompt
  dispatch/stream orchestration out of `routes/agents.rs`.

Before starting a new PR, inspect the current state of #229-#239. If they have
not landed yet, stack the next branch on #239. If they have landed, branch from
updated origin/main.

Create a separate worktree, implement the next large backend DDD batch, run
focused Rust validation plus clippy, push a PR, wait for CI, and merge only
after checks pass. Preserve unrelated user changes and do not revert anything
outside the batch.
```
