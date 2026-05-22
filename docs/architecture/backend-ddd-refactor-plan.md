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
- #240 `refactor/backend-ddd-container-credential-sweep` -> #239 branch: in
  progress; moves container credential-sync env, Container CLI credential
  injection, Git CLI credential injection, and OAuth mount cleanup out of
  `routes/containers.rs` into `AgentContainerCredentialService`.
- #241 `refactor/backend-ddd-route-boundary-check` -> #240 branch: in progress;
  adds a Rust route-boundary test so production route code cannot reintroduce
  raw SQL, production `json!` response assembly, or route-local response
  projections.
- #242 `refactor/backend-ddd-container-lifecycle-sweep` -> #241 branch: in
  progress; moves agent restart/resume Docker lifecycle orchestration out of
  `routes/agents.rs` into `AgentContainerLifecycleService`.
- #243 `refactor/backend-ddd-container-start-stop-sweep` -> #242 branch: moves
  agent container start/stop Docker orchestration, workspace directory
  preparation, container credential injection, participant registration/offline
  updates, and NATS connection revocation out of `routes/containers.rs` into
  `AgentContainerControlService`.
- #244 `refactor/backend-ddd-agent-create-workspace-sweep` -> #243 branch:
  moves agent creation workspace/project/cwd resolution out of
  `routes/agents.rs` into `AgentService`.
- #245 `refactor/backend-ddd-runtime-control-sweep` -> #244 branch: moves admin
  NATS revoke, orchestration task-update broadcasting, and pool runtime status
  reads out of route handlers into service/domain boundaries.
- #246 `refactor/backend-ddd-auth-attachment-sweep` -> #245 branch: moves
  refresh-token verification, access-token minting, password-reset delivery
  wiring, attachment upload draft assembly, and attachment download payload
  projection out of route handlers into service/domain boundaries.
- #247 `refactor/backend-ddd-context-runtime-sweep` -> #246 branch: moves
  context feature flag ownership, context/governance/analytics route gates,
  governance-audit HMAC runtime setup, orchestration context-injection runtime
  wiring, and container context-injection runtime settings into domain/service
  boundaries.
- #248 `refactor/backend-ddd-credential-runtime-sweep` -> #247 branch: moves
  CLI auth proxy provider/state-store/revoke-threshold wiring, CLI credential
  OAuth mount and system fallback key runtime config, container credential-sync
  settings, and attachment storage-limit runtime config out of routes into
  services; extends the route boundary test to block these runtime policy config
  reads from returning to production route code.
- #249 `refactor/backend-ddd-identity-runtime-factories` -> #248 branch: moves
  identity and credential route repository construction into service factories,
  moves auth refresh-cookie production policy into the user domain/service
  boundary, moves governance-audit/container runtime service wiring behind
  service constructors, and extends the route boundary test to block these
  identity repository and runtime policy reads from returning.
- #250 `refactor/backend-ddd-agent-orchestration-factories` -> #249 branch:
  moves agent, message, prompt, container-lifecycle, orchestration,
  task-context, context-preview, context-envelope, context-approval, and
  context-feature runtime/repository wiring into service factories; extends the
  route boundary test to block these agent/orchestration/context factory leaks
  from returning.
- #251 `refactor/backend-ddd-route-factory-sweep` -> #250 branch: moves the
  remaining production route-layer repository constructors into service
  factories across admin, attachment, analytics, audit, billing, dev
  environment, events/turns/inbox, feature flags, favorites, groups,
  organizations, plugins, projects, prompts, quota, resource members/profiles,
  settings, skills, teams, tiles, voice, and workspaces; upgrades the route
  boundary test to block production route repository imports and all
  `Repository::new` calls.
- #252 `refactor/backend-ddd-appstate-runtime-factories` -> #251 branch: moves
  remaining runtime-heavy service wiring out of production routes into
  `AppState` service factories, including config, encryption, LLM, Docker,
  NATS, Redis, object storage, auth callout, JWT, email, context, billing, and
  in-flight prompt dependencies; extends the route boundary test to block those
  runtime AppState reads and runtime-aware service constructors from returning.
- #253 `refactor/backend-ddd-service-sql-repositories` -> #252 branch: moves
  context resolver, context envelope, context usage analytics,
  memory/skill/context scope membership, and agent workspace SQL from services
  into repository modules and adds regression coverage that blocks production
  service raw SQL.
- #254 `refactor/backend-ddd-service-payload-domains` -> #253 branch: moves
  sidecar command payloads, CLI auth proxy encrypted file-map payloads, NATS
  KICK payloads, governance audit export payloads, and common configuration,
  skill, and memory JSON defaults into domain modules; expands service boundary
  coverage to block production `json!` payload construction.
- #255 `refactor/backend-ddd-protocol-boundary-hardening` -> #254 branch:
  moves NATS auth callout JWT protocol claim/header shapes and permissions into
  `domain::auth_callout`, gates the legacy group helper behind test support,
  and hardens route/service boundary tests against early test-only imports.
- #256 `refactor/backend-ddd-agent-container-policy` -> #255 branch: moves
  container-backed eligibility checks, restart/resume/stop container-id
  validation, stale container-reference errors, and start image rejection
  messaging into `domain::agent` policies.
- #257 `refactor/backend-ddd-service-protocol-dtos` -> #256 branch: moves
  auth-callout response envelopes, CLI auth token endpoint DTOs, OAuth refresh
  failure classification, and Stripe subscription/invoice snapshots into domain
  modules.
- #258 `refactor/backend-ddd-prompt-provider-policy` -> #257 branch: moves
  provider-backed prompt requirements, missing provider/model/API-key errors,
  in-flight busy conflicts, and provider build-error mapping into
  `domain::prompt`.
- #259 `refactor/backend-ddd-credential-error-policies` -> #258 branch: moves
  Git credential, Container CLI credential, CLI auth proxy token storage, and
  LLM provider encryption/test error contracts into credential/auth domain
  policies.
- #260 `refactor/backend-ddd-service-error-policies` -> #259 branch: moves
  remaining service-owned user-visible error policies across auth/default-org,
  billing active subscriptions, admin bulk delete, governance audit HMAC
  runtime keys, legacy navigation org updates, resource member email lookup,
  and orchestration parent-task mapping into domain policy helpers.
- #261 `refactor/backend-ddd-external-error-policies` -> #260 branch: moves
  external integration error and protocol policies for CLI auth proxy OAuth
  callback/state/token exchange, Stripe billing gateway/webhook/API failures,
  password-reset delivery configuration, and transactional email
  SMTP/recipient contracts into domain helpers.

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

### Batch 1: Remaining Route Factory And Projection Sweep

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
and close concrete gaps found by the audit. After #252, the next large sweep
should audit the remaining service layer for pure-domain candidates: inline
policy decisions, ad hoc validation, response/adaptor logic still living beside
repository I/O, and aggregate repository grouping drift.

PR #253 starts with production service SQL rather than
route-sized slices. Move direct tenant-scoped SQL for context resolver, context
envelope, context usage analytics, memory/skill/context scope membership, and
agent workspace mount resolution out of services and into repository modules,
then guard production services against reintroducing direct SQL. After that
lands, continue with aggregate-specific policy/adapter drift rather than another
small route sweep.

PR #254 follows with production service protocol/payload construction. Move
sidecar command payloads, CLI auth proxy encrypted file-map payloads, NATS KICK
payloads, governance audit export payloads, and common configuration/skill/
memory JSON defaults into domain modules; extend the service boundary test so
production services cannot reintroduce raw JSON payload construction.

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
- #240 container credential sweep, stacked on #239, currently moving container
  credential injection and cleanup orchestration out of `routes/containers.rs`.
- #241 route DDD boundary check, stacked on #240, currently adding automated
  regression coverage for route-layer boundary rules.
- #242 container lifecycle sweep, stacked on #241, currently moving agent
  restart/resume Docker lifecycle orchestration out of `routes/agents.rs`.
- #243 container start/stop sweep, stacked on #242, currently moving agent
  container start/stop orchestration out of `routes/containers.rs`.
- #244 agent creation workspace sweep, stacked on #243, currently moving agent
  creation workspace/project/cwd resolution out of `routes/agents.rs`.
- #245 runtime control sweep, stacked on #244, currently moving runtime control
  side effects out of admin, orchestration, and pool routes.
- #246 auth and attachment sweep, stacked on #245, currently moving refresh
  session policy, password-reset delivery wiring, and attachment upload/download
  payload rules out of auth and attachment routes.
- #247 context runtime sweep, stacked on #246, currently moving context feature
  flags, route feature gates, governance audit runtime HMAC setup, orchestration
  context-injection runtime wiring, and container context settings into
  domain/service boundaries.
- #248 credential runtime sweep, stacked on #247, currently moving CLI auth
  proxy provider/state-store/revoke-threshold wiring, CLI credential OAuth mount
  and system fallback key runtime config, container credential-sync settings,
  and attachment storage-limit runtime config into service boundaries.
- #249 identity runtime factory sweep, stacked on #248, currently moving
  identity/credential repository construction, auth refresh-cookie production
  policy, governance-audit runtime policy, and container control runtime wiring
  into service/domain factories.
- #250 agent and orchestration runtime factory sweep, stacked on #249, currently
  moving agent, message, prompt, container lifecycle, orchestration,
  task-context, context-preview, context-envelope, context-approval, and
  context-feature runtime/repository wiring into service factories and extending
  route-boundary regression coverage for those leaks.
- #251 route factory sweep, stacked on #250, currently moving remaining
  production route repository constructors into service factories and upgrading
  route-boundary regression coverage to block repository imports and any
  production route `Repository::new` call.
- #252 AppState runtime factory sweep, stacked on #251, currently moving
  runtime-heavy service wiring out of route modules into AppState factories and
  upgrading route-boundary regression coverage to block direct runtime AppState
  dependency reads from route code.
- #253 service SQL repository sweep, stacked on #252, moves context
  resolver, context envelope, context usage analytics, memory/skill/context
  scope membership, and agent workspace SQL from services into repository
  modules and adds regression coverage that blocks production service raw SQL.
- #254 service payload domain sweep, stacked on #253, moves production service
  JSON/protocol payload construction into domain modules and expands service
  boundary coverage to block production `json!` payload construction.
- #255 protocol boundary hardening sweep, stacked on #254, moves NATS auth
  callout JWT protocol claim/header shapes and permissions into
  `domain::auth_callout`, keeps the signing service focused on cryptographic
  orchestration, gates the legacy group test-only helper behind
  test-support, and fixes route/service boundary tests so early `#[cfg(test)]`
  imports no longer hide later production boundary leaks.
- #256 agent container policy sweep, stacked on #255, moves container-backed
  eligibility checks, restart/resume/stop container-id validation, stale
  container-reference errors, and start image rejection messaging into
  `domain::agent` policies so container services stay focused on Docker,
  repository, credential, and orchestration side effects.
- #257 service protocol DTO sweep, stacked on #256, moves auth-callout
  response envelopes, CLI auth token endpoint DTOs, OAuth refresh failure
  classification, and Stripe subscription/invoice snapshots into domain
  modules so services retain external I/O, signing, encryption, and
  persistence orchestration instead of owning protocol shapes.
- #258 prompt provider policy sweep, stacked on #257, moves provider-backed
  prompt requirements, missing provider/model/API-key errors, in-flight busy
  conflicts, and provider build-error mapping into `domain::prompt`, keeping
  prompt services focused on repositories, LLM factory calls, SSE streaming,
  and cancellation orchestration.
- #259 credential error policy sweep, stacked on #258, moves Git credential,
  Container CLI credential, CLI auth proxy token storage, and LLM provider
  encryption/test error contracts into domain policies so credential services
  keep encryption, repository, OAuth, and provider I/O orchestration instead of
  owning user-visible policy strings.
- #260 service error policy sweep, stacked on #259, moves remaining service-owned
  user-visible error policy contracts across auth/default-org, billing active
  subscriptions, admin bulk delete, governance audit HMAC runtime keys, legacy
  navigation org updates, resource member email lookup, and orchestration
  parent-task mapping into domain helpers.
- #261 external integration error policy sweep, stacked on #260, moves CLI auth
  proxy OAuth callback/state/token exchange errors, Stripe billing
  gateway/webhook/API errors, password-reset delivery configuration errors, and
  transactional email SMTP/recipient contracts into domain policies.

Before starting a new PR, inspect the current state of #229-#261. If they have
not landed yet, stack the next branch on the latest open DDD branch. If they
have landed, branch from updated origin/main.

Create a separate worktree, implement the next large backend DDD batch, run
focused Rust validation plus clippy, push a PR, wait for CI, and merge only
after checks pass. Prefer broad aggregate/service-family sweeps over small
single-service cleanup: scan all production services for the same class of
remaining DDD drift, move the full set in one branch, and add or extend the
boundary regression test that prevents the class from returning. Preserve
unrelated user changes and do not revert anything outside the batch.
```
