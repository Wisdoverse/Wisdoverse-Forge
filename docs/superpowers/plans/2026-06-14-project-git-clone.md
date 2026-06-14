# Project Git Clone — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a project be created with an optional git repository that the platform clones — server-orchestrated, in an ephemeral least-privilege container — into the project's workspace directory, with a durable, recoverable, visible clone status.

**Architecture:** One-shot clone driven by the existing Postgres job queue + transactional outbox; executed by a disposable minimal `agentforge-clone` container that mounts only a per-clone staging dir on a restricted network and is reaped deterministically; the control plane holds state in a `project_clone_attempts` table and never runs git or touches repo content. Atomic same-filesystem rename publishes the result into `/workspace/<dir>`; the agent owns it thereafter.

**Tech Stack:** Rust (axum/sqlx/tokio, crates: db, jobs, platform, api), Postgres, Docker, React/Vite (FSD, src/app), Tone-free.

**Spec:** `docs/superpowers/specs/2026-06-14-project-git-clone-design.md`

**Conventions for every milestone:**
- TDD: narrow failing test → minimal impl → green → commit.
- Migration numbering is NOT hardcoded: at execution time, use the next free number against current `origin/main` (068 is the next free as of writing, but rebase first — another branch may land it). Every new migration needs: the `.sql` file, a `MANIFEST.sha256` entry, and an `include_str!` entry in `rust/crates/db/src/lib.rs`/`pool.rs` (guard tests enforce this).
- Migrations are idempotent (`IF NOT EXISTS`, `ADD COLUMN IF NOT EXISTS`); never edit an already-run migration — add a corrective one.
- Validation gate per milestone: narrow Rust test, then `cd rust && make ci` when shared crates/API/DB/platform are touched; `npm run fsd:check && npm run lint && npm run typecheck && npm run test:unit` for frontend; `git diff --check` for docs.
- **Codex review gate** at the end of every milestone (`codex:codex-rescue`, inline findings); fold P1/P2 before moving on, matching the self-fix-loop cadence.
- Tenant scope: every new repository method takes `&TenantScope` and constrains by organization. No `unwrap()` in handlers.
- Secrets: `#[serde(skip_serializing)]` on any credential field; never log a token or repo URL with embedded creds.

---

## File Structure

Created:
- `rust/crates/db/migrations/0NN_project_clone.sql` — projects additive cols + `project_clone_attempts` + `job_queue` partial unique index.
- `rust/crates/api/src/domain/project_clone.rs` — `CloneStatus`, `CloneErrorClass`, `WorkspaceDirName`, `CloneAttempt` projection, state-transition policy (pure).
- `rust/crates/api/src/repositories/project/clone_attempt.rs` — attempt aggregate repo (submodule of the `project` aggregate).
- `rust/crates/api/src/services/project_clone.rs` — create-side orchestration (allocate dir, insert attempt + outbox), status projection, retry, immutability rule.
- `rust/crates/jobs/src/project_clone_worker.rs` — dequeue + state machine + container run + atomic rename + reconciler sweep + metrics/audit.
- `docker/Dockerfile.clone` — minimal `agentforge-clone` image.
- `docker/scripts/clone-entrypoint.sh` — container contract (cred mount → git clone into staging → result file → exit code).
- `docker/scripts/lib/git-credentials.sh` — git cred/config logic factored out of `agent-entrypoint.sh` and shared.
- `rust/crates/platform/src/clone_runtime.rs` — ephemeral clone container lifecycle (restricted network, staging mount, labels, timeout, wait, reap, sweep).
- `src/app/features/manage-project/ui/CloneStatusBadge.tsx` — status badge + retry.
- Tests alongside each (`#[cfg(test)]` modules + `rust/crates/api/tests/*.rs` integration + `tests/` FE).

Modified:
- `rust/crates/db/src/entities.rs` — `Project` gains `workspace_dir_name`, `clone_status`; add `ProjectCloneAttempt` struct.
- `rust/crates/api/src/domain/resource.rs` — harden `ProjectRepositoryUrl::parse` (https-only, host present); add `ResourceSlugPolicy` filesystem-safe guarantee used by the legacy-navigation path.
- `rust/crates/api/src/services/project.rs` + `repositories/project.rs` + `repositories/resource/navigation.rs` — transactional create across both surfaces; workspace-ownership check.
- `rust/crates/api/src/services/git_credential.rs` + `repositories/credential/git.rs` — host-matched single-credential resolution.
- `rust/crates/api/src/routes/projects.rs` (+ teams projects route) — accept repo URL, status in projection, retry endpoint.
- `rust/crates/jobs/src/orchestration_outbox_publisher.rs` (or a sibling) — route `project_clone` outbox rows into `job_queue`.
- `docker/scripts/agent-entrypoint.sh` — source the factored `lib/git-credentials.sh` (no behavior change).
- `docker/Dockerfile.agent-base` — install the shared lib; `Makefile` — `build-clone` target.
- `src/app/entities/project/model/types.ts` + `api/projectApi.ts`, `features/manage-project/ui/CreateProjectForm.tsx`, `pages/settings/ui/ProjectsSection.tsx`, `layouts/sidebar/ProjectTree.tsx`, WS dispatch hook.
- `shared/types/` — clone status contract kept in sync with the Rust serializer.

---

## Milestone M0 — Migrations + entities + schema-contract

**Files:**
- Create: `rust/crates/db/migrations/0NN_project_clone.sql`
- Modify: `rust/crates/db/migrations/MANIFEST.sha256`, `rust/crates/db/src/lib.rs` (or `pool.rs` include_str! list), `rust/crates/db/src/entities.rs`
- Test: `rust/crates/db/tests/schema_contract.rs` (or the existing schema-contract test module)

- [ ] **Step 1 — Write the migration.** `projects`: `ADD COLUMN IF NOT EXISTS workspace_dir_name TEXT`, backfill `= slug` for existing rows, then `SET NOT NULL`; `ADD COLUMN IF NOT EXISTS clone_status TEXT NOT NULL DEFAULT 'none'` with `CHECK (clone_status IN ('none','queued','cloning','ready','failed'))`; `CREATE UNIQUE INDEX IF NOT EXISTS uq_projects_workspace_dir ON projects(workspace_id, workspace_dir_name) WHERE deleted_at IS NULL`. New table `project_clone_attempts` exactly per spec §5.2 (FKs to organization/workspace/project, `UNIQUE(project_id, attempt)`, index `(status, lease_expires_at)`). `CREATE UNIQUE INDEX IF NOT EXISTS idx_job_queue_unique_key ON job_queue(unique_key) WHERE unique_key IS NOT NULL` (the index `queue::enqueue` already assumes).
- [ ] **Step 2 — Register the migration.** Append the file's sha to `MANIFEST.sha256` and add the `include_str!` entry; run the manifest/guard test: `cd rust && cargo test -p agentforge-db manifest -- --nocapture` → PASS.
- [ ] **Step 3 — Add entity structs.** In `entities.rs`, add `workspace_dir_name: String` + `clone_status: String` to `Project`; add `ProjectCloneAttempt` (`FromRow`, fields per §5.2; `credential_id`/error fields `Option`). Update every `Project { .. }` struct literal in api/domain/jobs tests (grep `Project \{` — entity-literal fan-out trap; verify `cargo test -p agentforge-api --lib --no-run`).
- [ ] **Step 4 — Schema-contract test.** Assert the new columns/table/indexes exist so a fresh test DB and production cannot drift. Run `cd rust && cargo test -p agentforge-db schema_contract` → PASS.
- [ ] **Step 5 — Gate.** `cd rust && make ci`. **Commit:** `feat(db): project clone attempts table + projects clone columns + job_queue unique index`.
- [ ] **Step 6 — Codex review** of the migration + entities (idempotency, CHECK vocab matches the spec state machine, index correctness, no edit to a run migration).

---

## Milestone M1 — Domain: clone types, slug/path policy, URL hardening

**Files:**
- Create: `rust/crates/api/src/domain/project_clone.rs`
- Modify: `rust/crates/api/src/domain/resource.rs`, `rust/crates/api/src/domain/mod.rs`
- Test: inline `#[cfg(test)]` in each.

- [ ] **Step 1 — `CloneStatus` enum** (`none|queued|cloning|ready|failed`) with `Serialize`/`as_str`/`FromStr`, and a pure `fn next(self, event) -> Result<CloneStatus, IllegalTransition>` encoding the spec §7 state machine. Test every legal + a few illegal transitions first (fail), then implement (green).
- [ ] **Step 2 — `WorkspaceDirName::derive(name) -> WorkspaceDirName`**: lowercase, `[a-z0-9-]` only, collapse repeats, trim leading/trailing `-`, length cap (e.g. 64), reject reserved (`.`, `..`, empty, `.git`). `fn resolve_under(&self, root: &Path) -> Result<PathBuf, PathEscape>` canonicalizes and asserts containment in `root`. Tests first: traversal inputs (`../x`, `a/b`, `..`), unicode, empties → rejected; `resolve_under` rejects a symlink/`..` escape.
- [ ] **Step 3 — Harden `ProjectRepositoryUrl::parse`**: require `https://` scheme (reject `http`/`git`/`ssh`/`file`), require a non-empty host, length cap. Tests first: `http://`, `file://`, `ssh://`, hostless, `https://h/r` (ok). (Egress/DNS defense lives in M4; this is defense-in-depth.)
- [ ] **Step 4 — `CloneErrorClass`** (`auth|not_found|network|timeout|too_large|internal`) + `fn redact(raw: &str) -> String` stripping `https://...@`, token-looking substrings, and truncating. Tests first: a URL with `user:token@`, a 10 KB blob → redacted + capped.
- [ ] **Step 5 — Gate + Commit:** `cargo test -p agentforge-api --lib domain::project_clone domain::resource` → PASS. `feat(domain): clone status state machine, filesystem-safe workspace dir, https-only repo url, error redaction`.
- [ ] **Step 6 — Codex review** (transition completeness, path-escape proof, redaction false-negatives).

---

## Milestone M2 — Transactional create + outbox enqueue

**Files:**
- Modify: `rust/crates/api/src/services/project.rs`, `repositories/project.rs`, `repositories/resource/navigation.rs`, `services/project_clone.rs` (new), `jobs/src/orchestration_outbox_publisher.rs` (route project_clone), migration for an outbox row kind if needed.
- Test: `rust/crates/api/tests/project_create_clone_tx.rs` (`#[sqlx::test]`).

- [ ] **Step 1 — Workspace ownership check.** Add `require_workspace_in_org(scope, workspace_id)` (mirror `repositories/agent/workspace.rs` ownership pattern). Test: a workspace from another org → `AppError::Forbidden`.
- [ ] **Step 2 — One create transaction.** Refactor `ProjectService::create` and the legacy-navigation create to a shared `tx`-taking path: validate workspace∈org + team/permission → `WorkspaceDirName::derive` + lock-allocate (insert relying on the `uq_projects_workspace_dir` unique index; on conflict, suffix `-2`, `-3`…) → insert project (`clone_status = repo? 'queued' : 'none'`) → default group → if repo: insert `project_clone_attempts` (attempt 1, `queued`) + insert an outbox row (`kind = project_clone`, payload `{project_id, attempt}`) → commit. The legacy-navigation draft MUST stop preserving caller slugs verbatim — route through `WorkspaceDirName`/`ResourceSlugPolicy`.
- [ ] **Step 3 — Outbox → job_queue.** Extend the outbox publisher to map a `project_clone` outbox row to `queue::enqueue(pool, "project_clone", payload, prio, None, Some("project_clone:<project_id>:<attempt>"), max_attempts)`. Test: an outbox row becomes exactly one `job_queue` row with that `unique_key`; a duplicate publish is a no-op.
- [ ] **Step 4 — Tests (`#[sqlx::test]`).** create-without-repo → `clone_status='none'`, no attempt/job. create-with-repo → project + attempt(`queued`) + one job, all in one committed tx; a forced failure after the project insert rolls back the whole tuple (no zombie). Run per the `reference_sqlx_test_local_db` recipe.
- [ ] **Step 5 — Gate + Commit:** `cd rust && make ci`. `feat(api): transactional project-with-repo create + clone-job outbox enqueue`.
- [ ] **Step 6 — Codex review** (tx boundary really atomic incl. the legacy path; unique-dir allocation race; outbox idempotency; no double create surface left unrouted).

---

## Milestone M3 — `agentforge-clone` image + entrypoint + shared cred lib

**Files:**
- Create: `docker/scripts/lib/git-credentials.sh`, `docker/scripts/clone-entrypoint.sh`, `docker/Dockerfile.clone`
- Modify: `docker/scripts/agent-entrypoint.sh` (source the lib), `docker/Dockerfile.agent-base` (ship the lib), `Makefile` (`build-clone`)
- Test: a `bats`/shell harness or a `docker run` smoke in M4's tests.

- [ ] **Step 1 — Factor cred/git-config.** Extract the GitHub/GitLab token + git-config + known_hosts logic from `agent-entrypoint.sh:~268-464` into `lib/git-credentials.sh` as functions (`configure_git_credentials`, `configure_known_hosts`). `agent-entrypoint.sh` sources it; verify all four agent images still inject creds (reuse the deployed-image checks used for the harness PRs). No behavior change.
- [ ] **Step 2 — `clone-entrypoint.sh` contract.** Inputs via env + a tmpfs secret mount: `CLONE_URL`, `CLONE_DEST` (the mounted staging path), `CLONE_PROVIDER`, credential file at `/run/secrets/git-credential` (mode 0400). Configure a one-shot git credential helper that reads the mounted file (NOT env). `git clone --no-recurse-submodules <url> "$CLONE_DEST/repo"`; on success write `$CLONE_DEST/.clone-result.json` = `{branch, head_sha, bytes}` (from `git rev-parse`, `git symbolic-ref`, `du -sb`); `exit 0`. On failure print stderr (worker redacts) and `exit 1`. No secret ever echoed; `set -o pipefail` here (unlike the agent entrypoint) and explicit error logging.
- [ ] **Step 3 — `Dockerfile.clone`.** `FROM` a slim base; install only `git ca-certificates openssh-client tini`; copy `lib/git-credentials.sh` + `clone-entrypoint.sh`; create the same `agent` UID/GID as `Dockerfile.agent-base`; `ENTRYPOINT ["/usr/bin/tini","--","/clone-entrypoint.sh"]`; run as `agent`.
- [ ] **Step 4 — `make build-clone`** target → `agentforge-clone:latest`. Build it.
- [ ] **Step 5 — Smoke.** `docker run` the image against a tiny public repo into a temp mount; assert `repo/.git` exists + `.clone-result.json` has a `head_sha`; assert the image has no node/python/docker (`! command -v node`).
- [ ] **Step 6 — Gate + Commit:** `feat(docker): minimal agentforge-clone image + shared git-credential lib`.
- [ ] **Step 7 — Codex review** (no secret in env/layers/logs; pipefail correctness; UID/GID match; minimal surface; agent-entrypoint refactor is behavior-preserving).

---

## Milestone M4 — Platform clone runtime (restricted, reaped)

**Files:**
- Create: `rust/crates/platform/src/clone_runtime.rs`
- Modify: `rust/crates/platform/src/lib.rs`, reuse `security.rs`/`container.rs`
- Test: inline + a gated integration behind a `docker`-available guard.

- [ ] **Step 1 — `CloneRunSpec`** (image, `repo_url`, `provider`, staging host path, credential bytes, timeout, labels) + `CloneRunOutcome` (`Ready{branch,head_sha,bytes}` | `Failed{class,stderr_tail}` | `Timeout`).
- [ ] **Step 2 — `run_clone(spec) -> CloneRunOutcome`.** Create the container via the platform Docker wrapper with: mount ONLY `spec.staging_host_path → /staging` (no projects-root mount); a tmpfs secret mount delivering the credential to `/run/secrets/git-credential`; a **restricted network** (a dedicated egress-filtered docker network, no access to the agents/internal networks); `security.rs` limits (no privileged, no host PID, no docker socket, cpu/mem/pids set, plus a clone-specific wall-clock + disk guard); a deterministic name `agentforge-clone-<attempt_id>` and label `agentforge.project_clone=<attempt_id>`. `docker start` → `docker wait` with `tokio::time::timeout`; on timeout, force-remove + return `Timeout`. Inspect exit code; read `/staging/.clone-result.json` for the success payload. ALWAYS force-remove the container in a `finally`-style guard (creds live in it).
- [ ] **Step 3 — `sweep_orphans()`** lists containers with label `agentforge.project_clone=*` older than the timeout and force-removes them (worker-crash recovery). Test the label filter parsing with a fake docker client.
- [ ] **Step 4 — Tests.** Unit-test spec→container-config (mount set is exactly `[/staging]`; network is the restricted one; limits present; label set) with a mock docker client (mirror `mcp_docker_runtime` test style). Gate the real-docker e2e behind availability.
- [ ] **Step 5 — Gate + Commit:** `cd rust && make ci`. `feat(platform): ephemeral clone container runtime with restricted egress, staging-only mount, deterministic reaping`.
- [ ] **Step 6 — Codex review** (mount scope can't see siblings; egress really blocks RFC1918/metadata; container always reaped on every path incl. panic; timeout enforced; no creds in logs).

---

## Milestone M5 — `project_clone` worker + reconciler

**Files:**
- Create: `rust/crates/jobs/src/project_clone_worker.rs`
- Modify: `rust/crates/jobs/src/lib.rs`, wire into the worker registry / server bin startup.
- Test: `rust/crates/api/tests/project_clone_worker.rs` (`#[sqlx::test]` + mock runtime).

- [ ] **Step 1 — Worker dequeue + transition.** On a `project_clone` job: load the attempt; set `cloning` + `projects.clone_status='cloning'` + a `lease_expires_at`; emit a `clone.started` event. Resolve the host-matched credential (M6 dependency: use `git_credential` host-match) — materialize bytes only here.
- [ ] **Step 2 — Run + publish.** Create the per-clone staging dir under `<projects_root>/.clone-staging/<attempt_id>` (same filesystem as the projects root — assert via `stat` dev id). Call `platform::run_clone`. On `Ready`: `rename(2)` staging-repo → `<projects_root>/<workspace_dir_name>` (atomic, same-fs); set attempt+project `ready`, write `branch/head_sha/bytes/duration`; emit `clone.ready` + audit + metrics. On `Failed/Timeout`: redact+classify, set `failed`, remove staging, schedule a bounded retry (new attempt row + outbox) with backoff; emit `clone.failed`.
- [ ] **Step 3 — Reconciler sweep** (periodic task): any `cloning` attempt with an expired lease → `platform::sweep_orphans` for its container, mark `failed`, retry if attempts remain; any `queued` attempt with no live job re-enqueues. This is the polling fallback (pg_notify is wake-up only).
- [ ] **Step 4 — Tests.** With a mock clone runtime: happy path → `ready` + the dir is renamed into place; failure → `failed` + staging removed + a retry attempt enqueued; a simulated worker crash (lease expired, status stuck `cloning`) → reconciler recovers to `failed`/retry. Atomic-rename test asserts no partial dir on injected failure.
- [ ] **Step 5 — Gate + Commit:** `cd rust && make ci`. `feat(jobs): project clone worker + reconciler with atomic publish, retry, metrics, audit`.
- [ ] **Step 6 — Codex review** (no stuck status reachable; rename is same-fs + atomic; staging always cleaned; retry can't loop unbounded; metrics/audit names; secret lifetime minimal).

---

## Milestone M6 — API surface (create, status, retry, credential host-match)

**Files:**
- Modify: `rust/crates/api/src/services/git_credential.rs`, `repositories/credential/git.rs`, `routes/projects.rs` (+ teams projects route), `services/project.rs` (immutability), domain projection.
- Test: `rust/crates/api/tests/project_clone_api.rs`.

- [ ] **Step 1 — Host-matched credential resolution.** Add `resolve_for_host(scope, host) -> Option<GitCredential>` selecting exactly one credential whose host matches (not "latest per provider"). Tests: two creds for different hosts → the right one; unknown host → `None`.
- [ ] **Step 2 — Create accepts repo URL** on the active `/teams/:teamId/projects` route + DTO; validated by the hardened `ProjectRepositoryUrl`. Projection returns `clone_status` + a latest-attempt summary (`status, error_class, error_message, branch, head_sha, updated_at`).
- [ ] **Step 3 — Retry endpoint** `POST /projects/:id/clone/retry`: owner/manager only; allowed only from `failed`; creates a new attempt + outbox row; returns the new attempt. Immutability: reject `repository_url` change once an attempt is `queued|cloning|ready` in `ProjectService::update`.
- [ ] **Step 4 — Tests.** auth/tenant (cross-org create/retry → Forbidden); create-with-repo returns `queued`; retry from `ready` → Conflict; repo-url change after clone → rejected; projection shape.
- [ ] **Step 5 — Gate + Commit:** `cd rust && make ci`. `feat(api): repo-url on create, clone status projection, retry endpoint, url immutability`.
- [ ] **Step 6 — Codex review** (host-match can't pick a foreign-org cred; retry state guard; projection doesn't leak raw errors/creds; route behind auth middleware + tenant scope).

---

## Milestone M7 — Frontend (create field, status badge, realtime)

**Files:**
- Modify: `src/app/entities/project/model/types.ts`, `api/projectApi.ts`, `features/manage-project/ui/CreateProjectForm.tsx`, `pages/settings/ui/ProjectsSection.tsx`, `layouts/sidebar/ProjectTree.tsx`, the WS dispatch hook under `src/app/hooks`, `shared/types/`.
- Create: `src/app/features/manage-project/ui/CloneStatusBadge.tsx`
- Test: `tests/` Vitest for the reducer + the form validation.

- [ ] **Step 1 — Types/DTO sync.** Add `cloneStatus` + `cloneSummary` to `NavProject`; add optional `repositoryUrl` to `CreateProjectInput`; mirror in `shared/types/`. Keep in lockstep with the Rust serializer (field-name parity test).
- [ ] **Step 2 — Create form.** Add an optional "Git repository URL" input (validate in the submit handler + `setError` banner + scroll-to-top — the RHF silent-validation rule: never rely on `register(..., {required})`). Show the derived path `/workspace/<dir>` read-only; never accept a host path.
- [ ] **Step 3 — `CloneStatusBadge`** renders `queued|cloning|ready|failed` with the redacted error + a Retry button (calls the M6 endpoint, owner/manager gated). Place it in `ProjectTree` + project detail.
- [ ] **Step 4 — Realtime + fallback.** Handle the `clone.*` WS event in the owning feature `model` slice with an idempotent reducer; on mount/refresh, fetch the project list so status recovers if a socket message was missed.
- [ ] **Step 5 — Tests.** Reducer idempotency (same event twice → one state); form rejects a bad URL with a visible banner (no silent dead-click); `grep "required: true" src/app` stays empty.
- [ ] **Step 6 — Gate + Commit:** `npm run fsd:check && npm run lint && npm run typecheck && npm run test:unit`. `feat(web): project create git URL + clone status badge + realtime`.
- [ ] **Step 7 — Codex review** (FSD boundaries; no localStorage; reducer idempotency; validation not silent; surface matches the active route).

---

## Milestone M8 — Security + integration + e2e + docs

**Files:**
- Create: `rust/crates/api/tests/project_clone_security.rs`, docs under `docs/guides/` + `docs/architecture/glossary.md` entries.
- Modify: `Makefile`/CI to build `agentforge-clone` where agent images are built; `docs/guides/configuration.md` (new env: restricted network name, clone timeout, concurrency).

- [ ] **Step 1 — Security tests.** Path traversal (`WorkspaceDirName` rejects + `resolve_under` blocks); SSRF (a repo URL resolving to 169.254.169.254 / 127.0.0.1 / RFC1918 is blocked by the restricted network — assert the network config, and an integration test that the clone fails closed); tenant boundary (clone never lands outside the creator's workspace root; staging mount excludes siblings); no-secret-in-logs (capture worker logs on a failed auth clone → assert no token/URL-with-creds).
- [ ] **Step 2 — Integration.** `#[sqlx::test]` end-to-end: create-with-repo → outbox → (mock runtime) worker → `ready` + dir present; failure → `failed` + retry; reconciler recovery. One real-docker e2e (gated) cloning a controlled public repo.
- [ ] **Step 3 — Docs.** Beginner-first guide: how to create a project from a git repo, what statuses mean, what to do on failure, the manual escape hatch for oversized repos; glossary terms (`clone attempt`, `workspace dir`, `agentforge-clone`); configuration vars; security note (egress isolation, credential scoping). Follow the Product/Doc standard + the `## Beginner UX / Operator Path` PR gate fields.
- [ ] **Step 4 — Gate.** Full `cd rust && make ci`; FE checks; build `agentforge-clone`; `git diff --check`.
- [ ] **Step 5 — Commit + PR.** `test+docs: project clone security/integration coverage + operator guide`. Open the PR with concrete validation evidence + the Beginner UX section (validate locally via `scripts/check-pr-beginner-ux.mjs`). Use `gh api -X PATCH .../pulls/N -F body=@file` to set the body (the `gh pr edit` path is broken on this repo).
- [ ] **Step 6 — Codex review** of the full diff (adversarial security pass: SSRF, secret lifetime, tenant isolation, reaping, no stuck status), then verify break-glass merges per the repo policy.

---

## Self-review (plan vs spec)

- §2 goals → M2 (create), M3/M4 (clone), M5 (lifecycle), M6 (status/retry), M7 (UI), M8 (security). ✓
- §5 data model → M0. ✓ §6.1/6.2 transactional create+outbox → M2. ✓ §6.3 worker/reconciler → M5. ✓ §6.4 slug/path safety → M1+M2. ✓ §6.5 clone image → M3. ✓ §6.6 staging+rename → M4(runtime)+M5(rename). ✓ §6.7 creds → M3(mount)+M6(host-match). ✓ §6.8 FE → M7. ✓
- §7 state machine → M1 (pure) enforced in M5. ✓ §8 retry/idempotency → M2(key)+M5. ✓ §9 url immutability → M6. ✓ §10 security → M1/M4/M5/M6 built, M8 proven. ✓ §11 limits → M4. ✓ §12 observability → M5. ✓
- Open choices §14 (secrets-broker, restricted-network mechanism, outbox reuse) are resolved at M2/M4 execution time and recorded in the milestone commit messages.
- Migration number is a placeholder (`0NN`) resolved against main at execution — intentional, not a gap.
