# Wisdoverse Forge Agent Instructions

Wisdoverse Forge is a self-hosted governed AI workbench for teams. It combines a Rust
control plane, containerized and provider-backed agent execution,
Temporal-backed workflow orchestration, real-time telemetry, and a
React/Vite/Three.js browser UI for task, run, evidence, context, and skill
workflows.

`AGENTS.md` is a symlink to this file. Keep this file as the canonical local agent
entrypoint unless the project adds an explicit generator later.

## Current Runtime Contract

- Backend ownership is Rust. The active workspace is `rust/`; do not add new
  backend behavior to legacy TypeScript server paths.
- The browser app in `src/` talks to the Rust API on `:4003` over HTTP and
  WebSocket.
- The Rust orchestrator is an active service on `:4010` and owns the
  Temporal-backed workflow runtime.
- Agent containers communicate through the sidecar, NATS, HTTP APIs, and the
  internal MCP bridge. Terminal/CLI behavior must respect this runtime path.
- Container CLI agents mount an organization/workspace-scoped projects root at
  `/workspace`. `agents.workspace_id` is the execution/access boundary;
  `agents.project_id` is only the primary project for UI context and task
  routing. Agents may work across projects inside the same workspace, but never
  across organizations.
- PostgreSQL is required. Redis and NATS clients are designed for graceful
  degradation where the code explicitly supports it.

Runtime shape:

```text
Browser (Vite dev server or static assets)
  -> Rust API / WebSocket gateway
  -> PostgreSQL / Redis / NATS / MinIO / Docker runtime
  -> Rust orchestrator -> Temporal
  -> Agent sidecar / hooks / container CLI processes
```

Data flow:

```text
Hook or sidecar producers
  -> NATS or direct Rust API
  -> Rust API / jobs persist to PostgreSQL
  -> WebSocket broadcast
  -> frontend EventBus and UI handlers
```

## Repository Map

- `rust/` - Rust workspace and default backend runtime.
- `rust/crates/core` - shared domain types, config, errors, tenant scope.
- `rust/crates/db` - SQLx pool, migrations, persisted entities.
- `rust/crates/auth` - JWT, Argon2, auth middleware.
- `rust/crates/infra` - Redis and NATS clients.
- `rust/crates/api` - Axum routes, services, repositories, WebSocket gateway,
  internal MCP bridge.
- `rust/crates/platform` - Docker management, security policy, warm pool.
- `rust/crates/jobs` - PostgreSQL task queue.
- `rust/crates/llm` - multi-provider LLM gateway.
- `rust/crates/orchestrator` - task orchestration and Temporal workflow logic.
- `rust/crates/cli` - Platform CLI support.
- `rust/bins/server` - main Rust API binary.
- `rust/bins/orchestrator` - orchestrator service binary.
- `rust/bins/sidecar` - agent container sidecar.
- `rust/bins/cli` - `agentforge` operator CLI.
- `rust/bins/buildx-plugin` - agent-container `docker buildx build` proxy.
- `src/` - React/Vite/Three.js frontend.
- `shared/` - shared TypeScript contracts and generated platform proto output.
- `hooks/` - event relay hook from agent container to sidecar.
- `docker/` - Dockerfiles, Compose files, deployment env examples.
- `tests/` - Vitest and Playwright test suites.
- `docs/` - architecture, runbooks, guides, plans, reviews.

## Naming Rules

Use the current Session -> Agent vocabulary consistently.

- `Agent` means the managed AI work actor. It may be container-backed or
  provider-backed.
- `Participant` means an A2A orchestration participant.
- `Container CLI` means the task-execution CLI inside an agent container, such as
  `claude`, `codex`, `gemini`, or `opencode`.
- `Platform CLI` means the `agentforge` operator binary from Rust.
- `cliSessionId`, `cli_session_id`, `BaseEvent.sessionId`, `session_start`,
  and `session_end` are external CLI/hook protocol names. Do not rename them.

See `docs/architecture/glossary.md` when changing UI copy, API fields, or DB
concept names.

## Product And Documentation Standard

Treat operators as first-time users by default. New features, UI copy, CLI
commands, runbooks, and errors must start from the shortest safe path for a
non-specialist user, then move advanced implementation details into validation,
troubleshooting, or architecture sections.

- State prerequisites before commands or configuration.
- Use copy-pasteable examples with clear placeholders.
- Explain what success looks like and what the user should do next.
- Prefer product vocabulary from `docs/architecture/glossary.md` over internal
  implementation shortcuts.
- CLI work must follow `docs/guides/cli-platform-support.md`, including
  mainstream Linux, macOS, and Windows operator support expectations.

## Commands

Frontend and TypeScript:

```bash
npm run dev
npm run build
npm run fsd:check
npm run lint
npm run format:check
npm run typecheck
npm run test
npm run test:unit
npm run test:integration
npm run test:e2e
npm run proto:gen
npm run proto:check
```

Rust:

```bash
npm run server
npm run migrate
cd rust && cargo test --workspace
cd rust && make ci
cd rust && make check
cd rust && make test
cd rust && make clippy
cd rust && make fmt
cd rust && make build
cd rust && make docker
cd rust && make audit
```

Compose and deployment:

```bash
make setup
make dev
make dev-d
make dev-down
make prod
make prod-ext
make prod-ext-logs
make prod-ext-down
make build-agent-base
make build-agent-all
make update-agents
```

`make dev` starts the backend stack. Run `npm run dev` separately for the
browser app. `make prod-ext` uses `docker/compose.yml` plus
`docker/compose.external.yml` and reads external service settings from
`docker/.env`.

## Validation Expectations

Choose checks by blast radius.

- Docs-only or agent-instruction changes: run `git diff --check`.
- Frontend or shared TypeScript changes: run `npm run fsd:check`, `npm run lint`,
  `npm run format:check`, `npm run typecheck`, and the relevant Vitest project.
- Rust changes: run the narrow Rust test first, then `cd rust && make ci` when
  the change touches shared crates, API contracts, orchestration, auth, DB, or
  platform security.
- Proto changes: run `npm run proto:gen` and `npm run proto:check`; generated
  files are checked in.
- Runtime or deployment changes: run the relevant Compose target. For production
  contract work, `make prod-ext` is the standing validation path, followed by
  service health and orchestration-chain checks.
- Before PR/MR push, rebase or merge against current `origin/main` unless the
  user explicitly asks for a different base.
- E2E flake pattern: lazy-loaded route chunks (Workshop3D, settings nav,
  Timeline, anything behind `Suspense` + `lazy`) race the Playwright default
  15s action timeout on a cold CDN cache. When a click-on-an-interactive-
  element times out but a retry passes, the fix is `waitFor({ state: 'visible' })`
  on the route container before the click plus `.click({ timeout: 30000 })` —
  not a sleep, not a hard wait, and not disabling the test. See PR #218
  (3D view) and PR #222 (theme toggle).

Use `gh` for GitHub PRs and `glab` for GitLab MRs and pipeline inspection. If
CLI flags differ on this host, check `<tool> <command> --help` or use the
provider API.

For GitHub PR queue checks, prefer the low-token snapshot path:

```bash
npm run pr:summary
```

Treat `ACTION` as the only state that needs immediate fix work. Treat `WAIT` as
the stop condition for the chat: review, CI, or the merge queue is still
working, so do not repeatedly refresh status inside the conversation. For
external monitoring, schedule `npm run pr:summary:monitor`; it reuses the local
snapshot when run too soon and alerts only when a PR needs action.
Do not use `gh pr checks --watch`, `gh run watch`, shell loops, or repeated
forced refreshes from the chat unless the user explicitly asks for a live watch.
Do not lower the script's repeat-read guard below 60 seconds or save the
emergency bypass flag into reusable commands.
If a bounded local waiter is necessary, it must print only terminal output and
must be stopped before sending the final response.

## Backend Contracts

- New HTTP, WebSocket, and MCP routes must be registered behind the auth
  middleware in `rust/crates/api/src/router.rs` and
  `rust/crates/api/src/middleware.rs` unless the endpoint is intentionally
  public infrastructure, such as `/health`.
- Tenant-scoped repository methods must accept `&TenantScope` and constrain
  queries by organization. Do not construct tenant scope outside auth middleware.
- Keep the route -> service -> domain -> repository split in `rust/crates/api/src/`.
  `domain/` owns `Serialize`-derived response/projection types, pure business
  policies, audit-event constructors, and protocol projections that are
  independent of SQLx rows. Services own repository I/O, transactions, and
  `From<RepositoryRow>` adapters; routes consume domain types through services
  via `pub use` re-exports.
- Repositories are grouped by DDD aggregate where multiple tables form one root
  (`repositories/agent/`, `context_candidate/`, `credential/`, `identity/`,
  `orchestration/`, `resource/`, `skill/`, `user/`). Single-table repos stay
  flat. New tables that belong to an existing aggregate should be added as
  submodules and re-exported from the aggregate's `mod.rs`.
- Keep API responses in the `{ ok: true/false, ...data }` style where that
  surface already uses it.
- Rust error handling follows the 3-layer pattern: domain errors with
  `thiserror`, infrastructure context with `anyhow`, and HTTP mapping through
  `AppError::IntoResponse`. Do not leak internal errors to clients.
- `clippy::unwrap_used` is denied in handler code. Prefer typed errors and
  explicit HTTP mappings.
- DB migrations live under `rust/crates/db/migrations/`. Make migrations
  idempotent when they must tolerate existing production drift.
- Do not edit SQLx migrations that have already run in production; add a new
  corrective migration instead. Otherwise checksum validation can block deploys.
  When Rust adopts legacy tables, keep a schema-contract test so fresh test
  databases and production do not drift into a split-schema state.
- PostgreSQL queue code uses `FOR UPDATE SKIP LOCKED`; `pg_notify` is a wake-up
  signal only and must have a polling fallback.

## Security Contracts

- WebSocket auth uses JWT from `?token=` and origin validation against configured
  CORS origins. Do not accept arbitrary origins.
- NATS auth uses per-agent credentials and callout validation. Sidecars connect
  as the agent identity and receive scoped pub/sub permissions. Do not bypass the
  per-agent isolation model.
- Rust container security validation in `platform/security.rs` must continue to
  block privileged mode, host PID, docker socket mounts, and missing resource
  limits. Defense-in-depth overrides should stay in container creation.
- Sensitive fields must use `#[serde(skip_serializing)]`, including password
  hashes, API keys, encrypted tokens, nonces, Stripe IDs, and equivalent secret
  material.
- LLM gateway encryption requires `LLM_ENCRYPTION_KEY` in production. Avoid
  logging provider secrets, encrypted payloads, or decrypted content.
- Test and manual login flows should use the existing `dev@example.com` account.
  Do not create throwaway debug accounts.
- This is a public repository. Never write private hostnames, internal domain
  names, staging URLs, internal GitLab/infrastructure URLs, real operator email
  addresses, or any other organization-identifying information into code,
  configuration, commit messages, PR descriptions, issue bodies, documentation,
  agent instructions, or memory files. Use placeholder domains
  (`staging.example.com`, `gitlab.example.com`) and `dev@example.com` in all
  artifacts that leave this machine. When referencing deployment targets in
  workflow files, read them from repository secrets, not inline literals.

## Frontend Contracts

- The active React app is `src/app`. It must follow strict Feature-Sliced Design
  boundaries: `app -> pages -> widgets -> features -> entities -> shared`.
  Imports may only point downward through this layer order. A feature may import
  its own files plus `entities` and `shared`; cross-feature imports must be
  promoted to `widgets`, `entities`, or `shared` first.
- Do not add active frontend code outside `src/app`. If behavior is needed from
  a retired root-level frontend path, move or adapt it under the correct
  `src/app` FSD layer before using it.
- Keep shared frontend utilities, global contexts, generated-independent API
  clients, and cross-slice stores under `src/app/shared`. Keep domain API/types
  under `src/app/entities`, user workflows under `src/app/features`, composed
  route-level surfaces under `src/app/widgets` or `src/app/pages`, and app
  wiring only under routes/layouts/providers.
- Run `npm run fsd:check` for any frontend change. The check must stay in
  `npm run lint` so CI rejects boundary regressions.
- Keep frontend API clients, WebSocket message handling, and Rust serializers in
  sync with `shared/types/`.
- Keep WebSocket dispatch under `src/app/hooks` and feature-specific realtime
  reducers under the owning `src/app/features/*/model` slice.
- When adding a view surface, compose it through `src/app/routes`,
  `src/app/pages`, or `src/app/widgets` instead of reintroducing a root-level
  view registry.
- For terminal or Container CLI UI work, verify the full browser -> Rust API ->
  agent container path, not only isolated frontend state.
- The sound system uses Tone.js synthesis; do not add audio files unless the
  product requirement explicitly changes.

## Docker And Agent Images

- Compose source of truth is `docker/compose.yml`; environment-specific files are
  thin overrides.
- Profiles include `dev`, `prod`, `external`, `tools`, `backup`, `storage`, and
  `casdoor`.
- Run `make setup` before first local Compose use to create required external
  Docker networks and OAuth mount permissions.
- Agent images use a two-layer model: `docker/Dockerfile.agent-base` for system
  dependencies, sidecar, and platform CLIs; `docker/Dockerfile.agent` for the
  selected Container CLI overlay.
- Rebuild the base image after sidecar, system dependency, or platform CLI
  changes. Use `make build-agent-all` for all supported Container CLIs.

## Common Change Recipes

- Add backend API module: create repository, service, route, and (where the
  surface owns a serializable response shape) domain modules in
  `rust/crates/api/src/`; register `mod.rs` entries; mount the route in the
  router; add auth/tenant tests. Place the response type in `domain/<topic>`
  and have the service import + `pub use` re-export it so routes consume the
  domain type through the service path.
- Add DB field or table: add a SQL migration, update DB entity structs, adjust
  repository queries, and cover tenant boundaries.
- Add CLI-facing protocol type: update Rust/TS shared contracts, regenerate
  proto output if applicable, and verify frontend consumers.
- Debug recent events: inspect the `events` table ordered by newest first and
  correlate with WebSocket broadcast logs.
- Debug prod-ext: inspect `docker/.env`, run `make prod-ext`, then check API,
  orchestrator, NATS, Temporal, and service logs before changing code.

## Workflow

- Protect user work. Inspect `git status --short --branch` before editing,
  committing, rebasing, or pushing.
- Use a separate `git worktree` for MR-sized branch work or whenever another
  session may touch the main checkout. Base it on the current target branch
  such as `origin/main`, and keep the primary checkout clean.
- Keep commits narrowly scoped to the traced blocker or requested change.
- Do not revert unrelated dirty files. If unrelated local edits exist, leave them
  alone and stage only the intended paths.
- Prefer current source files, tests, Compose files, and live command output over
  stale docs or old memory.
- For PR/MR work, push the branch and create/update the change request with
  concrete validation evidence.
- Playwright MCP browser tools must not run as root because Chromium refuses to
  launch without `--no-sandbox`.

## Useful References

- `README.md` - current project overview and local startup.
- `CONTRIBUTING.md` - branch and contribution expectations.
- `docs/architecture/overview.md` - runtime topology.
- `docs/guides/configuration.md` - runtime configuration.
- `docs/guides/deployment.md` - deployment topology.
- `docs/runbooks/nats-auth.md` - NATS auth callout model.
- `docs/security/dependency-policy.md` - dependency security policy.
