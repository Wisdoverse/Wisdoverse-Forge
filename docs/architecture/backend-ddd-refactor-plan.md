# Backend DDD Refactor Handoff

Last updated: 2026-05-20

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

The main branch is expected to contain merge commit
`93d40eb7433952c6c85f304a27d67372f2864ff9` or newer before continuing.

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

### Batch 1: Navigation And Resource Tree

Move legacy navigation organization/team/project tree behavior out of routes in
one batch:

- domain owns the legacy frontend response contract;
- repository owns all SQL;
- service owns permission checks, validation drafts, default workspace lookup,
  and default group creation;
- route only extracts HTTP data and calls the service.

### Batch 2: Orchestration And Task/Run Surfaces

Target route families around task orchestration, run state, evidence, timeline,
and event projection. These are high-value because they cross API, jobs,
orchestrator, and WebSocket behavior.

### Batch 3: Context, Skill, Resource, And Identity Aggregates

Target existing aggregate repository groups:

- `repositories/context_candidate/`
- `repositories/skill/`
- `repositories/resource/`
- `repositories/identity/`
- `repositories/agent/`
- `repositories/user/`

For each aggregate, finish route-to-service/domain cleanup in one PR per
aggregate family, not one file at a time.

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

Create a separate worktree from origin/main, implement the next large backend
DDD batch, run focused Rust validation plus clippy, push a PR, wait for CI, and
merge only after checks pass. Preserve unrelated user changes and do not revert
anything outside the batch.
```
