# Runtime Validation

This runbook records the current README/SPEC runtime boundary that has been
proved against the Rust-first implementation. Use it when checking whether the
engineering preview is runnable, and update it whenever a README-visible
capability moves in or out of the proofed boundary.

## Contract Summary

The proofed contract is taken from `README.md`, `SPEC.md`, and
`docs/architecture/overview.md`:

- Browser UI talks to the Rust API on `:4003` over HTTP and `/ws`.
- The Rust API owns auth, tenant scope, agent lifecycle, persisted work state,
  WebSocket fanout, jobs integration, and the internal MCP bridge.
- The Rust orchestrator runs on `:4010`, persists its own workflow domain, and
  starts the Temporal worker when Temporal is enabled.
- Temporal runs the live workflow runtime on `:7233`; the UI is on `:8233`.
- PostgreSQL is required. The existing `prod-ext` evidence below predates this
  RustFS migration and does not validate RustFS deployment or data cutover.
  Redis, NATS, Docker, and Temporal are part of that runtime path.
- Container CLI task execution flows through sidecar, NATS, Rust API jobs,
  persisted task/run/evidence state, and browser-visible task surfaces.

## RustFS storage checks

Use an isolated RustFS service and disposable bucket, with `STORAGE_PROVIDER=s3`,
`S3_ENDPOINT`, `S3_ACCESS_KEY`, `S3_SECRET_KEY`, and `S3_BUCKET` configured in the
test shell. The API integration test also requires a disposable PostgreSQL
instance through `DATABASE_URL`. Wait for RustFS `/health/ready` before running:

```bash
cargo test --manifest-path rust/Cargo.toml -p agentforge-infra object_storage::tests --locked -- --include-ignored
cargo test --manifest-path rust/Cargo.toml -p agentforge-api --test attachment_object_storage_test --locked -- --include-ignored
```

These checks create and reopen a bucket, verify object bytes and content type,
delete objects, read attachment rows with legacy `minio` metadata, and reject
access from another organization. They do not prove a production data migration
or the full `prod-ext` orchestration chain. Follow the
[migration guide](../guides/rustfs-migration.md) for cutover and rollback checks.

## Maintenance repository settings: local proof

Validated on 2026-10-01 at source revision `6378a1c4`, building on the repository
preflight in `db517d76`; implementation: [PR #1195](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/1195).
Environment: Linux, Node.js 24.19.0, Chromium 151, disposable PostgreSQL 17,
local Vite app and the compiled Rust API. Redis, NATS and orchestration were
outside this check. GitHub responses came from a local HTTP test service using
only synthetic installation credentials, `example-org/example-repo`, a
`release/stable` default branch and a 40-character starting revision.

Passed from the repository root:

```bash
npm run lint
npm run format:check
npm run typecheck
npm run test:unit
npm run build
node scripts/check-secret-scan.mjs
node --test scripts/__tests__/check-secret-scan.test.mjs
# Requires a running local app/API, real dev@example.com login via E2E_PASSWORD,
# and local BASE_URL/E2E_API_BASE_URL/PLAYWRIGHT_CHROMIUM_PATH overrides if needed.
npm run test:e2e -- maintenance-repository.spec.ts
```

When the browser app uses an API port other than `4003`, set the optional
`E2E_BROWSER_API_PORT` to that port; global setup stores it as `agentforge-port`
in browser local storage so requests use the independent API instead of a shared
service. `E2E_API_BASE_URL` separately sets the base URL for Node-side setup
requests, such as registration. It does not configure the browser API port.
The port is inherited through the normal global-setup storage state. Tests
that skip global setup or replace that storage state do not inherit it.

- Unit suite: 204 files, 2,812 tests; secret-scanner regression suite: 8 tests.
- The two browser cases passed with a connected administrator, a non-admin
  organization owner, and inaccessible GitHub repository metadata (six case
  executions). Application authentication and Rust API responses were not
  replaced by browser mocks.
- Desktop keyboard refresh displayed the exact repository, default branch,
  full starting revision and four independent permission flags. Mobile
  settings navigation and the 390-pixel content layout passed.
- An additional disposable-database rehearsal revoked `users.is_admin` while
  the page was open. Refresh returned HTTP 403 and removed the previous
  repository details; reloading showed guidance without the check control.
- Malformed responses, deadlines, cancellation, partial permissions and late
  account responses are covered by the focused UI/API unit tests linked in
  [Self-Fix Loop](../guides/self-fix-loop.md#validation-boundary).

This proves the local settings path only. It does not establish a real GitHub
installation, CI/branch-protection compatibility, production deployment,
or pilot adoption. The hostname blocklist was unset locally; its behavior was
tested using synthetic domains. Build/tooling checks retained their existing
chunk-size and configuration deprecation warnings.

## Maintenance intake and trace: local API proof

Validated on 2026-10-01 at source revision `57c18e1d`, building on the repository
setup change in [PR #1195](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/1195).
Environment: Linux, Node.js 24.19.0 and disposable PostgreSQL 17. The integration
test drives the real Rust router, JWT middleware and repositories with signed
test JWTs and a local GitHub HTTP mock. It uses synthetic installation
credentials, `dev@example.com`, and the placeholder repository `acme/widgets`.
The full workspace run required reclaiming approximately 9 GB of obsolete
generated test binaries after the shared build cache exhausted its disk space.

Passed from the repository root, with a disposable `DATABASE_URL` for Rust tests:

```bash
cd rust
cargo test -p agentforge-api domain::maintenance::tests --lib
cargo test -p agentforge-api --test maintenance_requests_route_test
cargo test -p agentforge-api --test route_ddd_boundary_test
make ci
cd ..
npm run lint
npm run format:check
npm run typecheck
npm run test:unit
node scripts/check-secret-scan.mjs
git diff --check
```

- Domain contracts: seven tests; architecture boundaries: eleven tests.
- Full Rust workspace: 2,823 tests passed, seven existing tests ignored, zero
  failures; formatting and clippy passed. Dependency audit completed with four
  existing warnings allowed by repository policy.
- The HTTP/database rehearsal covers eight concurrent submissions producing
  exactly one task/source link, same-source reuse and organization separation,
  foreign-destination refusal and cross-tenant foreign-key enforcement.
- A forced source-write failure rolls back the task insert. New tasks remain
  unassigned in backlog with no orchestration delivery. Permission revoked while
  reading the provider prevents task creation despite an existing signed JWT.
- Trace responses preserve intake/source heads while reporting updated merged
  states and moved source/produced heads, retain execution identifiers and the
  later rebuild base, and explicitly report unavailable provider observations.
  Existing-source replay still works during a provider outage; older tasks with
  no source record return `data: null`.
- All 101 SQL migration files match the SHA256 manifest. New migrations are
  also included in the embedded Rust migration source list.
- Shared-contract validation: 204 unit-test files, 2,812 tests. Public artifact
  scanning found no credential leaks in the checked tree.

This proves the deliberate intake and trace API against a disposable database
and a synthetic provider. Browser submission and trace proof is recorded in the
following section. This API validation does not prove webhook intake, a real
GitHub App installation, actual agent execution, production migration or pilot
acceptance. The existing merge and CI gates are unchanged; PR state observations
do not establish passing checks or approval.
See the [API guide](../guides/self-fix-loop.md#submit-and-trace-a-maintenance-source-api)
for prerequisites, retry behavior and the interrupted-index recovery boundary.

## Maintenance browser workflow: local proof

Validated on 2026-10-01 against frontend source tree
`2091ef19adf155de4ea23fce539584100b6af1b6` and Rust API revision `ba4ab478`.
Environment: Linux, Node.js 24.19.0, Chromium 151, Rust 1.98.1, disposable PostgreSQL 18,
local frontend and compiled API, and a local GitHub provider substitute. The
browser used the existing `dev@example.com` account through the real login flow.
The provider substitute made no real GitHub writes; no agent task was executed.

The four Playwright scenarios passed:

- A platform administrator used keyboard activation to refresh repository
  settings.
- Settings navigation remained usable at a narrow viewport.
- A maintenance request was submitted, then resubmitted under the same stable
  source. The retry returned the original task, retained its title and brief, and
  displayed its original source in the trace.
- At a narrow viewport, a PR 42 source displayed its submitted head (40 `a`
  characters), then reported the changed head (40 `c` characters) after refresh.
  When the provider returned 503, the trace displayed unavailable state,
  retained the submitted source revision, and exposed no merge action.

The run used existing disposable local records for an active project, task
place, organization and platform-admin account. Supply these environment values
locally; do not commit their values:

- `E2E_PASSWORD` for the existing `dev@example.com` account.
- `BASE_URL` pointing to the local frontend only.
- `E2E_MAINTENANCE_PROJECT_ID`, `E2E_MAINTENANCE_ORG_ID` and
  `E2E_MAINTENANCE_TEAM_ID` for the disposable local records.
- `E2E_MAINTENANCE_PROVIDER_CONTROL` pointing to the localhost-only test
  provider control endpoint, which exposes `/test/state`.
- `PLAYWRIGHT_CHROMIUM_PATH` for the local Chromium executable.

Run the browser cases with:

```bash
npm run test:e2e -- maintenance-repository.spec.ts \
  maintenance-workflow.spec.ts
```

Set `BASE_URL` to localhost for this run. The maintenance-workflow spec rejects
non-loopback frontend and provider-control hosts. Without the maintenance
fixture IDs, both workflow mutation tests skip; without the provider-control
endpoint, the PR-observation mutation test also skips. This keeps an
unconfigured run from writing to a deployed environment.

The related Vitest suite passed 68 tests. FSD checks, lint, formatting,
typecheck, production build and the secret scan also passed. The Rust API
subtree `f984009f192d05ea9e8284ada29c6e3724d9d8e3` and API revision `ba4ab478`
are the backend baseline for this browser run. The earlier 2,823-test Rust
workspace and full `make ci` evidence applies only to that backend revision; it
does not validate the subsequent transaction-ownership refactor below.

The frontend checks were:

```bash
npm run fsd:check
npm run lint
npm run format:check
npm run typecheck
npm run test:unit -- tests/unit/app/MaintenanceWorkflow.test.tsx \
  tests/unit/shared/maintenanceApi.test.ts \
  tests/unit/app/TaskDocumentPage.test.tsx \
  tests/unit/app/MaintenanceRepositorySection.test.tsx
npm run build
node scripts/check-secret-scan.mjs
```

This is local UI/API workflow proof only. It does not establish real GitHub
writes, agent execution, production migration, production readiness or pilot
acceptance. A real operator and pilot path using the selected repository and
its branch protection remains pending.

## Maintenance delivery: local proof

Validated on 2026-10-01 against Rust source tree
`663652a915eeac2afa94fea7d4dc3e99ee55be75`, frontend app tree
`66aa5cde013eb2ba5fdb482f472bb426cbbc3426`, shared-contract tree
`39ea3d877e1446d6b84cf87ceca1b62818bb45f7`, and tests tree
`b37130072a7c867d924552c4c5bac8e4ac82ba3f`. This supersedes the earlier
local checks for the combined maintenance implementation in
[PR #1196](https://github.com/Wisdoverse/Wisdoverse-Forge/pull/1196).

Environment: Linux, Rust 1.98.1, Node.js 24.19.0, Chromium 151 and disposable
PostgreSQL 18. The browser used real `dev@example.com` login, the rebuilt Rust
API and PostgreSQL. A loopback GitHub substitute supplied repository, PR-head
and check observations. Two explicitly synthetic finished-run records used
`codex` and `claude` labels and captured image metadata. No vendor CLI ran,
no real GitHub write occurred, and synthetic verdicts are not pilot acceptance.

The required Rust `make ci` passed: 2,828 tests, zero failures, seven existing
ignores, formatting and workspace/all-target Clippy. Dependency audit completed
with four existing policy-allowed warnings (`event-listener` unsoundness and
the yanked `chacha20`/two `spin` versions). Narrow domain tests and the real
HTTP/PostgreSQL delivery test also passed. That integration test covers:

- Authentication, tenant isolation, live administrator revocation during
  provider I/O, version/run ownership and acceptance after head drift/outage.
- Eight concurrent identical submissions returning one immutable report,
  conflicting key reuse, idempotent verdict/handoff replay and queued-work
  handoff refusal. A human verdict with a failing check cannot bypass the
  existing merge gate.
- Visible retry caps/backoff, retained report identity and snapshot after
  explicit run deletion, and comparison conditions for two synthetic CLI
  records. Raw artifact retention was not exercised.
- A 107-task submission cohort including failed, canceled and unreviewed work;
  non-overlapping 100/7 detail pages with unchanged full-cohort denominators;
  unknown minutes, stale verdicts and distinct review activity in the period.

Six real-login Playwright scenarios passed in 25.4 seconds. They cover keyboard
repository refresh, narrow repository navigation, source submission/replay,
source-head drift/outage, narrow-screen report/verdict/effort/handoff recording,
and comparison followed by changed-head/outage acceptance refusal. The delivery
scenario records six human minutes and a separately reported 12-minute
baseline only as synthetic form data; it asserts no approval/merge request.

The related Vitest suites passed 70 tests. After the browser identified field
label/help-text and select-label issues, the final seven delivery UI tests,
affected-file lint, formatting and typecheck passed again. FSD, full lint
(including beginner-copy/metrics/protocol guards), production build, secret
scan, migration manifest and `git diff --check` also passed.

To reproduce, first prepare a disposable local database, active project/task
place and administrator account, the local frontend/rebuilt API and a
loopback-only GitHub test provider. Use the fixture variables listed in the
earlier browser proof, plus `E2E_MAINTENANCE_DELIVERY_TASK_ID` and
`E2E_MAINTENANCE_DELIVERY_OTHER_TASK_ID` for two deliberately provisioned,
equivalent finished-run tasks. These tests write reports and human decisions;
do not point them at pilot or production records. They skip without explicit
fixtures and reject non-loopback frontend/provider-control hosts.

```bash
umask 022
DATABASE_URL='<disposable PostgreSQL URL>' make -C rust ci
npm run test:unit -- tests/unit/shared/maintenanceApi.test.ts \
  tests/unit/shared/maintenanceDeliveryApi.test.ts \
  tests/unit/app/MaintenanceWorkflow.test.tsx \
  tests/unit/app/MaintenanceDelivery.test.tsx \
  tests/unit/app/MaintenanceRepositorySection.test.tsx
npm run test:e2e -- maintenance-repository.spec.ts \
  maintenance-workflow.spec.ts maintenance-delivery.spec.ts
```

Apply migrations through 104 before using the current maintenance delivery
schema. Migration 102
builds `idx_task_runs_org_task_id` concurrently outside a transaction. If the
build is interrupted, inspect `pg_index.indisvalid` for this named index and
remove it only if invalid before retrying; `IF NOT EXISTS` cannot repair an
invalid index. Migration 103 creates the append-only delivery records with
tenant/run ownership constraints. Run deletion clears only the live run link.
Migration 104 corrects migration 101's maintenance-request task foreign key
without editing that immutable migration, so explicit task deletion also
removes its maintenance request. This source-record change does not establish
whole-organization deletion behavior. The 2026-10-01 validation below exercised
migrations through 103 only; it does not verify migration 104 or an organization
purge. At that historical revision, its 103 migration files matched the
committed manifest and embedded migration list; this says nothing about the
current migration 104.

This proves the local record/review workflow for the revision recorded above.
The current working changes have separate validation recorded below; do not
infer those results from this historical browser proof.

Optional product evaluation remains separate: teams may measure four-week
repeat use, effort and quality across a consenting cohort. It does not block
engineering implementation or merge. See the
[Maintenance delivery guide](../guides/maintenance-delivery.md) for the
operator workflow and the [Product Validation Guide](../guides/product-validation.md)
for optional evaluation methods.

## Current Engineering Validation (2026-10-02)

The source was recovered into an isolated checkout after the host restart.
Current code evidence is revision
`9de10c760feea270b1a89d633ba472c5d67d3c46`, based on current
`origin/main` `8f5f9a69831ded1aa8cf604d600f11e7a2bbc3a1` and including PR #1196.
The recovered source matched the intended changes from the earlier checkout,
including its browser login port configuration. The runtime API and sidecar
binaries were built at `352cb460cd0108fd56209da430bc8df433dea234`; their
backend source is unchanged at the current code revision. Using Node.js
24.20.0 and dependencies
installed with `npm ci`, full lint (including FSD, copy, metrics and protocol
checks), typecheck, format check, production build, and all 2,879 tests across
208 unit-test files passed, including the 69 focused `BoardView`/`TaskCard`
tests.

At the same working tree, all eight maintenance browser scenarios passed in
21.3 seconds against the local Rust API and disposable database. This includes
real task-priority PATCH plus persisted GET readback, and an aborted background
GET that retained the loaded cards before a successful manual retry. These
results cover the local browser-to-API board path.

A separate sidecar browser scenario passed in 21.3 seconds: browser request,
Rust API/outbox, authenticated encrypted per-agent NATS callout, native sidecar,
a deterministic fake `codex` Container CLI executable, result worker,
database/object storage, and browser artifact readback. This validates only
that bounded local path; it does not validate a vendor CLI, agent container,
or Temporal workflow.

A controlled native-host model run passed one browser case in 10.3 seconds
(13.8 seconds total): browser task creation, per-agent NATS authentication,
sidecar, `codex` 0.160.0 with `gpt-6-luna`, result worker, database and object
storage, and browser readback of one artifact. The initial attempt timed out
before calling the model; the controlled retry completed successfully. This
proves one host CLI/model path, not execution in an agent container, Temporal
workflow execution, or comparison across two CLIs.

Earlier on 2026-10-02, before the host restart, all 104 migrations applied on
disposable disk-backed PostgreSQL 17.11. The migration 104 contract was exercised with existing source, report, decision and handoff
records: the original task deletion failed with FK error `23503`; migration 104
was applied twice without losing records; a cross-tenant rewrite failed with
`23503`; and task deletion removed the four records. A pre-104 custom-format
`pg_dump` restored into a separate database with one row in each table and the
original `NO ACTION` foreign keys. The manifest SHA for migration 104 matched
its SQL file.

The migration 104 regression also passed on disposable PostgreSQL 18.6 in an
isolated container (one test, zero failures, 7.26 seconds). The first attempt
could not reach its test database because the internal network had not
published its loopback port. After connecting the task-owned container to a
separate task-owned bridge, the real schema-upgrade and deletion contract ran
successfully. Its database storage was ephemeral memory; this does not prove
production storage durability.

The compiled `maintenance_source_upgrade_and_explicit_deletion` Rust regression
passed again at the recovered code revision against a separate PostgreSQL
17.11 cluster on an ephemeral memory filesystem (one test, zero failures). An
earlier disk-backed invocation was terminated during SQLx database cleanup
after the host checkpoint stalled (`jbd2_log_wait_commit`); these memory-backed
runs do not establish disk durability. The earlier disk-backed migration and
backup/restore checks above are separate evidence. Nine GitHub API boundary
Rust tests and 15 self-fix policy tests also passed at the recovered revision.
All 104 migration checksums matched their committed manifest. An isolated
NATS 2.12.7 instance accepted an authenticated backend JetStream request and
rejected an anonymous connection. The per-agent callout and execution paths are
covered separately by the browser runs above.
An earlier `cargo audit` invocation exited 0 and reported zero vulnerabilities,
with four warnings:
`event-listener` 5.4.1 (`RUSTSEC-2026-0221`, unsound), `chacha20` 0.10.0
(yanked), and `spin` 0.9.8 and 0.10.0 (yanked). The recovered checkout has not
yet completed its audit. Full `make ci` was interrupted after cache cleanup
removed its in-progress build directory. In the current retry, formatting and
Clippy passed. Workspace tests then failed in
`context_approval_flow_test::approving_memory_candidate_creates_governed_memory_once`
with HTTP 500; an isolated diagnostic run is in progress. Full `make ci` and
its audit are not recorded as passed. The focused Platform CLI health/version
test passed (one test). The compiled Linux CLI also passed real `health -o json`
and `version -o json` commands against the API: readiness checks were retained,
and the absent server version was reported as `(unknown)` without stderr.
Execution in an agent container, the Temporal-backed workflow, production
migration/runtime validation and comparison across supported CLIs remain
pending.
These engineering gates remain required; pilot adoption is optional and does
not gate implementation or merge. All databases, records and the backup were
disposable test data; these checks do not validate production data or artifacts.

## Backend transaction ownership follow-up

The subsequent backend change moves ownership of the cross-aggregate
maintenance transaction into the service. The service locks and verifies the
platform-admin role, locks the source before checking for an existing request,
validates the destination, then creates the task and source record in the same
transaction. The maintenance-request repository now provides SQL primitives;
the Group repository owns the active-destination query and scoped lock. The
frontend app source remains `2091ef19adf155de4ea23fce539584100b6af1b6`; the
browser evidence above records that app against the earlier compiled API.

The following results apply to the updated backend subtree
`3729e710994317301be4b10b7cb766a26c65e266` and are separate from the earlier
API-baseline evidence above:

- Targeted transaction, concurrency, rollback and tenant-scope tests:
  12 passed: the real HTTP/PostgreSQL maintenance-intake test and 11 architecture
  boundary tests (`maintenance_requests_route_test` and
  `route_ddd_boundary_test`).
- Full Rust CI on the updated backend subtree passed. Reproduce it from the
  repository root with a disposable PostgreSQL instance in `DATABASE_URL`:

  ```bash
  umask 022
  cd rust && make ci
  ```

  The existing clone-secret test requires that file-permission mask.
  Result: 2,823 passed, 0 failed and 7 ignored; `cargo fmt` and `cargo clippy`
  passed. `cargo audit` reported four policy-allowed existing warnings:
  `event-listener` 5.4.1 (RUSTSEC-2026-0221, unsound), `chacha20` 0.10.0
  (yanked), and `spin` 0.9.8 and 0.10.0 (yanked).
- Four local Playwright browser scenarios against the newly compiled API:
  passed in 50.5 seconds. This is a local browser/API check, not a real GitHub
  write, agent execution, production migration or pilot acceptance.

## Existing deployment proof

Last validated on 2026-05-13 from the repository root using `make prod-ext`.

### Stack Health

Run:

```bash
make prod-ext
docker ps --filter 'name=agentforge-'
curl -fsS http://127.0.0.1:4003/health
curl -fsS http://127.0.0.1:4003/api/health
curl -fsS http://127.0.0.1:4010/health
docker exec agentforge-temporal temporal operator cluster health --address temporal-internal:7233
docker exec agentforge-nats wget -qO- http://localhost:8222/healthz
```

Expected evidence:

- `agentforge-server`, `agentforge-orchestrator`, `agentforge-temporal`, and
  `agentforge-nats` are `healthy`.
- API `/health` returns `{"ok":true,"status":"healthy"}`.
- API `/api/health` returns `status:"ready"` with `database`, `docker`,
  `nats`, and `redis` checks true.
- Orchestrator `/health` returns `{"status":"healthy"}`.
- Temporal cluster health is `SERVING`.
- NATS monitoring health is `{"status":"ok"}`.

### Orchestrator Schema Contract

The orchestrator uses its own database URL and SQLx migration history. The
legacy integer-key orchestrator tables are preserved under
`legacy_orchestrator` and replaced with UUID-key Rust-owned tables.

Run:

```bash
cd rust
DATABASE_URL=postgres://agentforge:devpassword@127.0.0.1:45432/agentforge \
  cargo test -p agentforge-orchestrator --test schema_contract
```

Expected evidence:

- `fresh_schema_matches_rust_owned_uuid_contract` passes.
- `legacy_integer_schema_is_preserved_and_replaced_with_uuid_tables` passes.
- In a migrated external orchestrator DB, `_sqlx_migrations` includes
  `8 | adopt legacy integer schema`, public workflow/task/review/knowledge/audit
  key columns are `uuid`, and legacy rows remain under
  `legacy_orchestrator.*_legacy_int`.

### Temporal Workflow

Use the orchestrator internal token and tenant headers to create a one-node
gate workflow through `POST /api/v1/workflows`, then run it through
`POST /api/v1/workflows/{id}/run`.

Expected evidence:

- Workflow creation returns a UUID workflow id and one node.
- Run returns `status:"running"`, a Temporal workflow id formatted as
  `orchestrator-<workflow-id>`, and a non-empty Temporal run id.
- Polling `GET /api/v1/workflows/{id}/status` reaches `status:"completed"`.
- The gate node reaches `status:"completed"` with output
  `{ "passed": true, "reason": "all dependencies completed successfully" }`.

### WebSocket Event Fanout

Use a real login token from `POST /api/v1/auth/login`, connect to
`ws://127.0.0.1:4003/ws?token=<token>` with the configured production Origin,
then publish JSON to `broadcast.<org-id>` through NATS backend credentials.

Expected evidence:

- The WebSocket connection upgrades with the real JWT.
- A JSON payload published to the tenant broadcast subject is received by the
  WebSocket client unchanged.

### Browser To Sidecar Task Path

Run the focused Playwright proof against the local Vite UI and `prod-ext`
backend:

```bash
cargo build -p agentforge-sidecar
npm run dev:client -- --host 127.0.0.1 --port 4002

BASE_URL=http://127.0.0.1:4002 \
ORCHESTRATION_REAL_E2E=1 \
ORCHESTRATION_REAL_E2E_CLEANUP_AUTH=1 \
E2E_EMAIL=dev@example.com \
E2E_PASSWORD=DevPass123! \
E2E_DATABASE_URL=<api-database-url-with-host-127.0.0.1> \
NATS_PORT=4222 \
npx playwright test --config tests/e2e/playwright.config.ts \
  tests/e2e/specs/orchestration-real-task.spec.ts --project chromium
```

Expected evidence:

- The test logs in with the real auth endpoint.
- It seeds a workspace, team, project, group, agent, participant, and scoped
  token through `POST /api/v1/auth/switch-context`.
- The browser creates an assigned task from the task board.
- The local `agentforge-sidecar` subscribes to the NATS assignment subject,
  executes the configured Container CLI path, reports completion, and the task
  reaches `completed`.
- A page reload shows the completed task and evidence marker.

### LLM Provider Connection Test

Run the focused Rust route proof with a test database URL:

```bash
cd rust
DATABASE_URL=<api-database-url-with-host-127.0.0.1> \
  cargo test -p agentforge-api routes::llm_providers::tests
```

Expected evidence:

- The provider settings test endpoint decrypts the stored user API key through
  the Rust encryption key path.
- The route builds a provider instance through the shared LLM gateway factory.
- The successful test returns `ok: true` with the provider and model.
- Error payloads redact upstream provider bodies and never echo API keys.

### Instruction Image To Model (staging, per Container CLI)

This is the live half of the instruction-image pipeline. The hermetic half
(upload → object store → vision gate → workspace materializer) is covered by
unit and integration tests in CI; what CI cannot prove is that a running
Container CLI actually delivers the image to a live model. Run this check
**once per release train for each vision-capable Container CLI** (`claude`,
`codex`, `gemini`). It is not a per-PR gate.

Prerequisites:

- A deployed staging stack (`make prod-ext` target, for example
  `https://staging.example.com`) with at least one running agent per Container
  CLI you are checking, each configured with a vision-capable model.
- A login that can create tasks for those agents (use the standing
  `dev@example.com` staging account).
- A test image containing a nonce no model could guess. Generate one locally:

```bash
NONCE=$(uuidgen | cut -c1-8)
echo "staging-image-check ${NONCE}" | convert -pointsize 32 label:@- /tmp/image-check.png
echo "Nonce: ${NONCE}"
```

Steps, per Container CLI:

1. Log in to staging and open the task composer for an agent running that CLI.
2. Attach `/tmp/image-check.png` as an instruction image. The task composer
   only offers the attachment control for container agents whose Container CLI
   reports image input capability (`claude`/`codex`/`gemini`) — if the control
   is missing, check the agent's runtime kind and CLI first; that is the gate
   working as designed. A vision-capable model remains your responsibility as
   a prerequisite: the UI gate proves CLI capability, not the model.
3. Dispatch a task whose instruction is exactly: "Reply with the text that
   appears in the attached image."
4. Watch the run output in the task detail view.

Expected evidence:

- The run output contains your nonce string. That proves the image crossed the
  full path (browser upload → API → object store → materializer → container →
  CLI → model) — a model cannot transcribe a nonce it never received.
- If the output describes being unable to see an image, or invents different
  text, the CLI-to-model delivery is broken for that Container CLI even though
  the materialized file exists in `/workspace`. File it against the CLI overlay
  (see the `docker/Dockerfile.agent` layer for that tool), not the server.

Record the release version, date, per-CLI pass/fail, and the nonce in the
release-train notes. The Gemini path was first verified this way manually
(the provider request carried an `inlineData` image part); this check keeps
that guarantee standing for every CLI on every train.

## Preview Boundaries

These surfaces are intentionally outside the proofed runtime boundary until
they have implementation and validation evidence:

| Surface                         | Current state                       | Required next step                                                                          |
| ------------------------------- | ----------------------------------- | ------------------------------------------------------------------------------------------- |
| `GET /api/v1/agents/:id/git`    | Returns an empty placeholder shape. | Implement real git status collection or keep it documented as unavailable.                  |
| `POST /api/v1/voice/transcribe` | Stub route.                         | Wire a real provider-backed transcription path and tests, or remove it from active UI/docs. |

Do not broaden README claims until this runbook contains a command that proves
the capability end to end.
