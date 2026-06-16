# Self-fix loop (Phase 4 + human-gated merge) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A human creates a `self-fix` task on the platform's own board; the existing orchestration spine spawns an agent that fixes this repo in `/workspace`; the server-side PR Bridge rebuilds the change onto a clean clone and opens a draft PR; a human reviews it in-platform; on approval the server-side Merge Executor merges (CI-green-gated). No auto-deploy.

**Architecture:** All privileged Git and GitHub operations run in the **API/server process** (the only process with the `/workspace` bind mount, Docker, config/secrets, and the new GitHub App client). The agent in `/workspace` holds no remote credential and the server **never runs Git against the agent's `.git`** — it rebuilds the change from a server-pinned base via a vetted file-content import. Merge approval is recorded server-side on the `self_fix` task (mirroring the orchestrator's `ReviewState` vocabulary) and gated by a sensitive-path circuit breaker.

**Tech Stack:** Rust (axum, sqlx, `reqwest` 0.12, `jsonwebtoken` 10) in `rust/crates/api` + `rust/crates/db` + `rust/crates/core`; PostgreSQL; the server-side `git` CLI via `tokio::process::Command`; React/Vite FSD frontend in `src/app`.

---

## Spec

This plan implements `docs/superpowers/specs/2026-06-14-self-fix-loop-phase4-design.md` (committed `f32abdf`). Read it first. The spec already incorporates a Codex review + an adversarial security pass; this plan turns its decisions into code against verified codebase facts.

## Design decisions & deviations (read before starting)

These were resolved during planning from verified codebase facts. Each milestone depends on them.

- **D1 — Server-side `git`, never in `/workspace`.** `agentforge-server` runs Alpine with no `git` (`rust/Dockerfile:50-51`). The Bridge needs `git` only for its **own server-owned clean clone** (clone/worktree/commit/push). We add `git` to the server image (M0). The agent's `/workspace` is read by the server as **plain files** (filesystem I/O), never via a Git command, exactly as the spec's Security §1 requires. *Rejected alternative:* delegating git to a spawned agent container (the scout's suggestion) — it runs git inside the untrusted repo, defeating the trust boundary, and there is no `docker exec` primitive anyway (`platform/src/docker.rs` exposes no exec; tasks reach agents only over NATS).
- **D2 — The whole loop lives in `rust/crates/api` (server process).** That process uniquely has the workspace mount (`compose.yml` bind), `docker: Option<Arc<DockerClient>>` on `AppState`, config/secrets, and (new) the GitHub App client. The orchestrator's `ReviewState` aggregate (`rust/crates/orchestrator/src/review/`) is a *different process* on `:4010` with no workspace/Docker/GitHub access, so we **mirror its state vocabulary** (`in_review` → `approved` / `changes_requested`) on the `self_fix` task API-side instead of cross-process-calling it. The existing orchestration spine (spawn via NATS) is reused unchanged.
- **D3 — `base_commit_sha` is GitHub's `origin/main` SHA, pinned at dispatch.** The server must not trust `/workspace/.git`. At self-fix task dispatch, the orchestration assignment path resolves `origin/main` via the GitHub App client (GitHub is the source of truth) and writes `base_commit_sha`. The Bridge rebuilds the agent's file content onto that base. This makes orphan-branch / unrelated-history attacks moot (the agent's commit graph is never read).
- **D4 — Merge approval is a NEW server-side review surface, not the `waiting_approval` button.** The existing FE "Approve" button keys on `state==='blocked' && blockedReason==='waiting_approval'`, which is a **pre-dispatch** gate (`approval_release_state` → `queued` → re-dispatch). Reusing it would re-run the agent. We add a dedicated review status + approve route that records `approved` and invokes the Merge Executor. The task stays `completed` throughout.
- **D5 — Scope includes the human-gated merge.** Unlike the older roadmap (`docs/plans/self-iteration-roadmap.md`, draft-PR-only), this plan's spec explicitly includes the merge slice. We implement through merge. We do **not** implement auto-deploy, background dispatch, or any auto-merge tier.

## File map (what each new/changed file is responsible for)

Create:
- `rust/crates/db/migrations/068_orchestration_pr_tracking.sql` — additive PR/base-SHA columns.
- `rust/crates/api/src/domain/self_fix.rs` — pure: sensitive-path classifier, review-status vocabulary, response helpers, error helpers.
- `rust/crates/api/src/services/github_app/mod.rs` — GitHub App client (JWT mint, installation token cache, ref resolve, draft PR, check-runs, mark-ready, guarded merge).
- `rust/crates/api/src/services/self_fix/mod.rs` — `SelfFixService` facade.
- `rust/crates/api/src/services/self_fix/bridge.rs` — PR Bridge (stop container, clean clone, import+validate, rebuild, push, draft PR).
- `rust/crates/api/src/services/self_fix/import.rs` — the file-content import validator (symlink/gitlink/escape/oversize/churn).
- `rust/crates/api/src/services/self_fix/merge_executor.rs` — guarded merge.
- `rust/crates/api/src/routes/self_fix.rs` — authed review/approve routes.
- `src/app/features/detail/ReviewSnapshotPanel.tsx` — FE PR-diff + CI-check snapshot + Approve.
- Tests under `rust/crates/api/tests/` and `rust/crates/db` + `src` vitest.

Modify (mechanical, idioms below): `rust/Dockerfile`, `rust/crates/db/migrations/MANIFEST.sha256`, `rust/crates/db/src/pool.rs`, `rust/crates/db/src/entities.rs`, `rust/crates/api/src/repositories/orchestration/mod.rs`, `rust/crates/core/src/config.rs` (+ 5 other struct-literal sites), `rust/bins/server/src/main.rs`, `rust/crates/api/src/state_services.rs`, `rust/crates/api/src/router.rs`, `rust/crates/api/src/services/mod.rs`, `rust/crates/api/src/routes/mod.rs`, `rust/crates/api/src/domain/mod.rs`, `src/app/shared/api/orchestration.ts`, `shared/types/`.

> **Line numbers in this plan are from the planning scout and drift over time. Always re-read the target region before editing; match on surrounding code, not raw line numbers.**

---

## Milestone 0: `git` in the server image

**Files:**
- Modify: `rust/Dockerfile` (runtime stage `apk add` line, ~line 51)

- [ ] **Step 1: Add git to the runtime stage**

In `rust/Dockerfile`, find the runtime stage `apk add` (currently `apk add --no-cache ca-certificates curl` around line 51) and add `git`:

```dockerfile
RUN apk add --no-cache ca-certificates curl git
```

- [ ] **Step 2: Verify the image has git**

Run:
```bash
cd rust && docker build -t agentforge-server:gitcheck -f Dockerfile . \
  && docker run --rm --entrypoint git agentforge-server:gitcheck --version
```
Expected: prints `git version 2.x`. (This is a slow build on this host — background it and wait for the completion notification; do not sleep-poll.)

- [ ] **Step 3: Commit**

```bash
git add rust/Dockerfile
git commit -m "build(server): install git in runtime image for the self-fix PR bridge"
```

---

## Milestone 1: Data model — PR/base-SHA columns on `orchestration_tasks`

**Files:**
- Create: `rust/crates/db/migrations/068_orchestration_pr_tracking.sql`
- Modify: `rust/crates/db/migrations/MANIFEST.sha256`
- Modify: `rust/crates/db/src/pool.rs` (`MIGRATION_SOURCES` array)
- Modify: `rust/crates/db/src/entities.rs` (`OrchestrationTask` struct)
- Modify: `rust/crates/api/src/repositories/orchestration/mod.rs` (`CreateTaskRow` + INSERT)
- Test: `rust/crates/api/tests/self_fix_pr_columns_test.rs` (new)

- [ ] **Step 1: Write the migration**

Create `rust/crates/db/migrations/068_orchestration_pr_tracking.sql`:

```sql
-- 068: self-fix PR tracking columns on orchestration_tasks.
-- Additive + idempotent. base_commit_sha is the origin/main SHA pinned at dispatch
-- (the base the PR Bridge rebuilds onto); pr_* are GitHub opaque values; self_fix marks
-- a code-fix task against this repo; review_status mirrors the orchestrator ReviewState
-- vocabulary but is driven API-side on the task (see plan D2/D4).

ALTER TABLE orchestration_tasks
    ADD COLUMN IF NOT EXISTS self_fix BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS base_commit_sha TEXT,
    ADD COLUMN IF NOT EXISTS pr_number INT,
    ADD COLUMN IF NOT EXISTS pr_url TEXT,
    ADD COLUMN IF NOT EXISTS pr_head_sha TEXT,
    ADD COLUMN IF NOT EXISTS review_status TEXT;
```

- [ ] **Step 2: Regenerate the manifest**

Run:
```bash
cd rust/crates/db/migrations && sha256sum *.sql | sort > MANIFEST.sha256
```
This appends the `068_orchestration_pr_tracking.sql` hash line in sorted order.

- [ ] **Step 3: Add the `include_str!` entry**

In `rust/crates/db/src/pool.rs`, in the `MIGRATION_SOURCES` array (ends ~line 143, before the closing `];`), add in numeric order:

```rust
    ("068_orchestration_pr_tracking.sql", include_str!("../migrations/068_orchestration_pr_tracking.sql")),
```

- [ ] **Step 4: Run the manifest guard tests (expect PASS)**

Run:
```bash
cd rust && cargo test -p agentforge-db --lib migration
```
Expected: `embedded_sources_match_manifest_exactly` and `every_migration_file_on_disk_is_embedded` PASS. If either fails you skipped the manifest regen (Step 2) or the `include_str!` entry (Step 3).

- [ ] **Step 5: Extend the entity struct**

In `rust/crates/db/src/entities.rs`, in the `OrchestrationTask` struct (`#[derive(... FromRow)]`, ends ~line 415), add fields before the closing brace:

```rust
    pub self_fix: bool,
    pub base_commit_sha: Option<String>,
    pub pr_number: Option<i32>,
    pub pr_url: Option<String>,
    pub pr_head_sha: Option<String>,
    pub review_status: Option<String>,
```

`SELECT *` queries auto-hydrate these via `FromRow` (column-name mapping) — no SELECT changes needed.

- [ ] **Step 6: Extend `CreateTaskRow` + INSERT**

In `rust/crates/api/src/repositories/orchestration/mod.rs`:

Re-read the `CreateTaskRow` struct and `create_in_tx` INSERT first (the scout reported ~13 bind params; **confirm the real count before numbering**). Add to `CreateTaskRow`:

```rust
    pub self_fix: bool,
```

(Only `self_fix` is set at create time; `base_commit_sha`/`pr_*`/`review_status` are written later by dispatch/Bridge via dedicated UPDATEs in M5/M6, so they default to NULL/false and need no INSERT binding.)

Add `self_fix` to the INSERT column list and a `$N` placeholder (next number after the current highest), and a `.bind(row.self_fix)` in the matching position. Every existing `CreateTaskRow { .. }` construction site must now set `self_fix` (default `false`) — grep for `CreateTaskRow {` and update each.

- [ ] **Step 7: Add dedicated UPDATE methods (used by M5/M6/M7)**

In the same repository file, add tenant-scoped UPDATEs (follow the existing `patch`/`update_status` idiom in this file):

```rust
/// Pin the base commit SHA the PR Bridge will rebuild onto (M5, dispatch time).
pub async fn set_base_commit_sha(&self, scope: &TenantScope, id: Uuid, sha: &str) -> AppResult<()> {
    sqlx::query("UPDATE orchestration_tasks SET base_commit_sha = $1 WHERE id = $2 AND org_id = $3")
        .bind(sha).bind(id).bind(scope.org_id().as_uuid())
        .execute(&self.pool).await.map_err(internal)?;
    Ok(())
}

/// Record the opened PR + initial review status (M6).
pub async fn set_pr_metadata(
    &self, scope: &TenantScope, id: Uuid,
    pr_number: i32, pr_url: &str, pr_head_sha: &str, review_status: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE orchestration_tasks \
         SET pr_number = $1, pr_url = $2, pr_head_sha = $3, review_status = $4 \
         WHERE id = $5 AND org_id = $6")
        .bind(pr_number).bind(pr_url).bind(pr_head_sha).bind(review_status)
        .bind(id).bind(scope.org_id().as_uuid())
        .execute(&self.pool).await.map_err(internal)?;
    Ok(())
}

/// Transition the review status (M7/M8: approved, changes_requested, merged).
pub async fn set_review_status(&self, scope: &TenantScope, id: Uuid, status: &str) -> AppResult<()> {
    sqlx::query("UPDATE orchestration_tasks SET review_status = $1 WHERE id = $2 AND org_id = $3")
        .bind(status).bind(id).bind(scope.org_id().as_uuid())
        .execute(&self.pool).await.map_err(internal)?;
    Ok(())
}
```

(Use the file's existing error-mapping helper instead of `internal` if it differs; re-read the top of the file. `scope.org_id().as_uuid()` matches the tenant idiom in this repo.)

- [ ] **Step 8: Write the round-trip test**

Create `rust/crates/api/tests/self_fix_pr_columns_test.rs`. Follow the `#[sqlx::test]` idiom from `rust/crates/api/tests/complete_task_tx_test.rs`:

```rust
#[sqlx::test(migrations = "../db/migrations")]
async fn pr_columns_round_trip_and_self_fix_defaults_false(pool: sqlx::PgPool) {
    // seed org/workspace + a task with self_fix=true via the repo's create path
    // (reuse the seeding helpers used by complete_task_tx_test.rs).
    // 1. create a task with self_fix = true
    // 2. set_base_commit_sha, set_pr_metadata, set_review_status
    // 3. SELECT the task; assert all 6 columns round-trip and a default task has self_fix=false.
    // Use a real TenantScope; assert a DIFFERENT org cannot read/update the row (tenant boundary).
}
```

Fill in the body using the seeding helpers in the neighbouring test. Assert: `self_fix` defaults `false` for a normal task; the 6 columns round-trip; a foreign-org `set_review_status` updates 0 rows.

- [ ] **Step 9: Run the test**

Run (see `reference_sqlx_test_local_db` for the local DB setup):
```bash
cd rust && cargo test -p agentforge-api --test self_fix_pr_columns_test
```
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add rust/crates/db/migrations/068_orchestration_pr_tracking.sql \
        rust/crates/db/migrations/MANIFEST.sha256 \
        rust/crates/db/src/pool.rs rust/crates/db/src/entities.rs \
        rust/crates/api/src/repositories/orchestration/mod.rs \
        rust/crates/api/tests/self_fix_pr_columns_test.rs
git commit -m "feat(db): self-fix PR/base-sha/review-status columns on orchestration_tasks"
```

---

## Milestone 2: Sensitive-path policy (pure domain)

This is the spec's non-negotiable circuit breaker (findings #3/#4), with the **corrected repo-root-relative globs + explicit own-code list**.

**Files:**
- Create: `rust/crates/api/src/domain/self_fix.rs`
- Modify: `rust/crates/api/src/domain/mod.rs` (`pub mod self_fix;`)

- [ ] **Step 1: Write the failing test**

Create `rust/crates/api/src/domain/self_fix.rs` with the test first:

```rust
//! Self-fix domain policy: pure, no I/O. Sensitive-path circuit breaker + review vocab.

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked(paths: &[&str]) -> bool {
        SensitivePathPolicy::touches_sensitive(&paths.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn sensitive_paths_trip_the_breaker() {
        // Each of these MUST be classified sensitive (repo-root-relative diff paths).
        assert!(blocked(&["rust/crates/auth/src/jwt.rs"]));
        assert!(blocked(&["rust/crates/db/migrations/099_x.sql"]));
        assert!(blocked(&["rust/crates/api/src/middleware.rs"]));
        assert!(blocked(&["rust/crates/auth/src/middleware.rs"]));
        assert!(blocked(&["rust/crates/api/src/mcp.rs"]));
        assert!(blocked(&["rust/crates/api/src/domain/mcp.rs"]));
        assert!(blocked(&["rust/crates/api/src/repositories/agent/mcp.rs"]));
        assert!(blocked(&["rust/crates/platform/src/security.rs"]));
        assert!(blocked(&[".github/workflows/ci.yml"]));
        assert!(blocked(&[".github/CODEOWNERS"]));
        // The loop's OWN code (no glob prefix distinguishes it — explicit list):
        assert!(blocked(&["rust/crates/api/src/services/self_fix/bridge.rs"]));
        assert!(blocked(&["rust/crates/api/src/services/self_fix/merge_executor.rs"]));
        assert!(blocked(&["rust/crates/api/src/services/github_app/mod.rs"]));
        assert!(blocked(&["rust/crates/api/src/domain/self_fix.rs"]));
    }

    #[test]
    fn benign_paths_do_not_trip() {
        assert!(!blocked(&["src/app/features/board/TaskCard.tsx"]));
        assert!(!blocked(&["rust/crates/api/src/routes/licenses.rs"]));
        assert!(!blocked(&["docs/guides/configuration.md"]));
    }

    #[test]
    fn a_mix_with_one_sensitive_path_trips() {
        assert!(blocked(&["README.md", "rust/crates/auth/src/jwt.rs"]));
    }

    #[test]
    fn bare_prefix_without_rust_is_not_the_matcher_input_form() {
        // Regression: diff paths carry the rust/ prefix. A bare path must NOT match auth.
        assert!(!blocked(&["crates/auth/x.rs"]));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:
```bash
cd rust && cargo test -p agentforge-api --lib domain::self_fix
```
Expected: FAIL (`SensitivePathPolicy` not defined).

- [ ] **Step 3: Implement the policy**

Add above the `#[cfg(test)]` block in the same file:

```rust
use agentforge_core::{AppError, ErrorKind};
use serde::Serialize;
use serde_json::{json, Value};

/// Glob-prefix directories whose ANY descendant is sensitive.
const SENSITIVE_DIR_PREFIXES: &[&str] = &[
    "rust/crates/auth/",
    "rust/crates/db/migrations/",
    ".github/workflows/",
];

/// Exact files (or basename rules) that are sensitive wherever they appear.
const SENSITIVE_BASENAMES: &[&str] = &["middleware.rs", "mcp.rs", "security.rs"];

/// Exact repo-root-relative files that are sensitive (own-code + CODEOWNERS).
const SENSITIVE_EXACT: &[&str] = &[
    ".github/CODEOWNERS",
    "rust/crates/api/src/services/self_fix/mod.rs",
    "rust/crates/api/src/services/self_fix/bridge.rs",
    "rust/crates/api/src/services/self_fix/import.rs",
    "rust/crates/api/src/services/self_fix/merge_executor.rs",
    "rust/crates/api/src/services/github_app/mod.rs",
    "rust/crates/api/src/routes/self_fix.rs",
    "rust/crates/api/src/domain/self_fix.rs",
];

pub(crate) struct SensitivePathPolicy;

impl SensitivePathPolicy {
    /// True if ANY changed path is sensitive. Input paths are repo-root-relative
    /// (the form `git diff --name-only` / diff-tree emits), forward-slashed.
    pub(crate) fn touches_sensitive(changed_paths: &[String]) -> bool {
        changed_paths.iter().any(|p| Self::is_sensitive(p))
    }

    fn is_sensitive(path: &str) -> bool {
        if SENSITIVE_DIR_PREFIXES.iter().any(|d| path.starts_with(d)) {
            return true;
        }
        if SENSITIVE_EXACT.iter().any(|f| path == *f) {
            return true;
        }
        let basename = path.rsplit('/').next().unwrap_or(path);
        SENSITIVE_BASENAMES.iter().any(|b| basename == *b)
    }
}

/// Review status vocabulary (mirrors orchestrator ReviewState; driven API-side — plan D2).
pub(crate) mod review_status {
    pub(crate) const IN_REVIEW: &str = "in_review";
    pub(crate) const APPROVED: &str = "approved";
    pub(crate) const CHANGES_REQUESTED: &str = "changes_requested";
    pub(crate) const MERGED: &str = "merged";
    /// Routed to CODEOWNERS / manual merge; in-platform Approve disabled.
    pub(crate) const SENSITIVE_BLOCKED: &str = "sensitive_blocked";
}

pub(crate) fn self_fix_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

pub(crate) struct SelfFixPolicy;

impl SelfFixPolicy {
    pub(crate) fn sensitive_path_blocked() -> AppError {
        ErrorKind::ForbiddenWithCode {
            code: "errors.self_fix.sensitive_path_blocked",
            message: "This PR touches a security-sensitive path; in-platform merge is disabled. \
                      Route it to a CODEOWNERS review and merge manually."
                .into(),
        }
        .into()
    }

    pub(crate) fn checks_not_green() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.self_fix.checks_not_green",
            message: "Required CI checks are not all green; cannot merge.".into(),
        }
        .into()
    }

    pub(crate) fn head_moved() -> AppError {
        ErrorKind::Conflict("the PR head moved since review; re-review required".into()).into()
    }
}
```

- [ ] **Step 4: Register the module**

In `rust/crates/api/src/domain/mod.rs` add (alphabetical):

```rust
pub(crate) mod self_fix;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run:
```bash
cd rust && cargo test -p agentforge-api --lib domain::self_fix
```
Expected: all 4 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/api/src/domain/self_fix.rs rust/crates/api/src/domain/mod.rs
git commit -m "feat(self-fix): sensitive-path circuit breaker + review vocabulary (pure domain)"
```

---

## Milestone 3: Config — GitHub App fields + secret encryption

**Files:**
- Modify: `rust/crates/core/src/config.rs` (struct + `from_env` validation + test literal)
- Modify (struct-literal fan-out — all required or it won't compile): `rust/crates/api/src/test_support.rs`, `rust/crates/infra/src/redis_client.rs`, `rust/crates/infra/src/nats.rs`, `rust/crates/api/src/routes/cli_auth_proxy.rs`, `rust/crates/api/src/services/cli_auth_proxy/mod.rs`

- [ ] **Step 1: Add the fields to `AppConfig`**

In `rust/crates/core/src/config.rs`, add to the `AppConfig` struct (before the closing brace, ~line 469):

```rust
    #[serde(default)]
    pub github_app_id: Option<String>,
    #[serde(default)]
    pub github_app_installation_id: Option<String>,
    #[serde(default)]
    pub github_app_private_key: Option<SecretString>,
    /// "owner/repo" the self-fix loop targets.
    #[serde(default)]
    pub github_app_repo: Option<String>,
```

- [ ] **Step 2: Add all-or-none validation in `from_env`**

In `AppConfig::from_env()`, after the existing NATS-callout validation block, add:

```rust
    let github_app_fields = [
        cfg.github_app_id.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false),
        cfg.github_app_installation_id.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false),
        cfg.github_app_private_key.as_ref().map(|v| !v.expose_secret().trim().is_empty()).unwrap_or(false),
        cfg.github_app_repo.as_ref().map(|v| !v.trim().is_empty()).unwrap_or(false),
    ];
    let set = github_app_fields.iter().filter(|v| **v).count();
    if set != 0 && set != github_app_fields.len() {
        return Err(config::ConfigError::Message(
            "GITHUB_APP_ID, GITHUB_APP_INSTALLATION_ID, GITHUB_APP_PRIVATE_KEY, and GITHUB_APP_REPO \
             must be configured together (self-fix loop)".to_string(),
        ));
    }
```

(`use secrecy::ExposeSecret` if not already imported in this file.)

- [ ] **Step 3: Add the four fields to every struct-literal site**

Add to each `AppConfig { .. }` literal (before its closing brace), value `None` for all four:

```rust
    github_app_id: None,
    github_app_installation_id: None,
    github_app_private_key: None,
    github_app_repo: None,
```

Sites (re-read each; counts/positions drift):
- `rust/crates/core/src/config.rs` test literal (~702-758)
- `rust/crates/api/src/test_support.rs` (~44-105)
- `rust/crates/infra/src/redis_client.rs` (~82-140)
- `rust/crates/infra/src/nats.rs` (~179-237)
- `rust/crates/api/src/routes/cli_auth_proxy.rs` (~46-104)
- `rust/crates/api/src/services/cli_auth_proxy/mod.rs` (~1104-1160)

- [ ] **Step 4: Write the validation test**

In `rust/crates/core/src/config.rs` `#[cfg(test)]` module add:

```rust
#[test]
fn github_app_fields_must_be_all_or_none() {
    // helper that sets only some of the GITHUB_APP_* env vars and calls from_env,
    // asserting Err; then sets all four and asserts Ok. Follow the existing
    // env-mutation test style in this module (serialize via the existing test guard).
}
```

Implement using the module's existing env-var test harness pattern.

- [ ] **Step 5: Compile + test**

Run:
```bash
cd rust && cargo test -p agentforge-core --lib config
```
Expected: PASS, and the workspace compiles (the fan-out is complete).

- [ ] **Step 6: Wire env into the server binary (no-op if `from_env` already reads them)**

`AppConfig::from_env` reads flat env vars via the `config` crate (`GITHUB_APP_ID` etc., no `__`). No `main.rs` change is needed for reading. Confirm by running the server with the four vars set in a scratch `.env` and checking it boots without the validation error. (Defer real values to deployment.)

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(config): GITHUB_APP_* fields with all-or-none validation"
```

---

## Milestone 4: GitHub App client (`reqwest` + `jsonwebtoken`)

**Files:**
- Create: `rust/crates/api/src/services/github_app/mod.rs`
- Modify: `rust/crates/api/src/services/mod.rs` (`pub mod github_app;`)
- Modify: `rust/crates/api/Cargo.toml` (ensure `jsonwebtoken` + `reqwest` deps; add if missing)
- Test: `rust/crates/api/tests/github_app_client_test.rs` (httpmock-based)

Use `reqwest` (workspace-wide 0.12) + `jsonwebtoken` (workspace `=10`); **do not add octocrab**.

- [ ] **Step 1: Add a mockable client skeleton + the JWT unit test**

Create `rust/crates/api/src/services/github_app/mod.rs`:

```rust
//! Minimal GitHub App REST v3 client for the self-fix loop. No octocrab.
//! Mints an app JWT, exchanges an installation token (cached), and performs the
//! repo operations the PR Bridge / Merge Executor need.

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub(crate) struct GithubAppConfig {
    pub app_id: String,
    pub installation_id: String,
    pub private_key_pem: String, // decrypted at construction time
    pub repo: String,            // "owner/repo"
}

#[derive(serde::Serialize)]
struct AppJwtClaims { iat: u64, exp: u64, iss: String }

/// Build the signed app JWT (RS256). `now_unix` is injected for testability.
pub(crate) fn build_app_jwt(app_id: &str, private_key_pem: &str, now_unix: u64)
    -> Result<String, jsonwebtoken::errors::Error>
{
    let claims = AppJwtClaims {
        iat: now_unix.saturating_sub(60),     // clock-skew backdate
        exp: now_unix + 9 * 60,               // GitHub max 10 min
        iss: app_id.to_string(),
    };
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(private_key_pem.as_bytes())?;
    jsonwebtoken::encode(&jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256), &claims, &key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_RSA_PEM: &str = include_str!("../../../tests/fixtures/test_rsa_private_key.pem");

    #[test]
    fn app_jwt_has_backdated_iat_and_bounded_exp() {
        let now = 1_700_000_000u64;
        let token = build_app_jwt("12345", TEST_RSA_PEM, now).expect("jwt");
        // decode without verification to inspect claims
        let mut v = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        v.insecure_disable_signature_validation();
        v.validate_exp = false;
        let data = jsonwebtoken::decode::<serde_json::Value>(
            &token, &jsonwebtoken::DecodingKey::from_secret(b"x"), &v).expect("decode");
        assert_eq!(data.claims["iss"], "12345");
        assert_eq!(data.claims["iat"].as_u64().unwrap(), now - 60);
        assert!(data.claims["exp"].as_u64().unwrap() <= now + 600);
    }
}
```

Create the fixture key:
```bash
mkdir -p rust/crates/api/tests/fixtures
openssl genrsa -out rust/crates/api/tests/fixtures/test_rsa_private_key.pem 2048
```
(This is a throwaway test key, never a real credential — safe to commit.)

- [ ] **Step 2: Run the JWT test (fails, then passes)**

Run:
```bash
cd rust && cargo test -p agentforge-api --lib services::github_app
```
Expected first run: FAIL if `jsonwebtoken` isn't a dep of `agentforge-api`. Add to `rust/crates/api/Cargo.toml` under `[dependencies]`:
```toml
jsonwebtoken = { workspace = true }
```
(If the workspace doesn't expose it as `workspace = true`, mirror the exact spec from `rust/Cargo.toml` line ~61.) Re-run; expected: PASS.

- [ ] **Step 3: Add the installation-token cache (unit-tested)**

Add to the module:

```rust
#[derive(Clone)]
struct CachedToken { token: String, expires_at: u64 }

pub(crate) struct GithubAppClient {
    http: reqwest::Client,
    cfg: GithubAppConfig,
    cache: std::sync::Arc<tokio::sync::Mutex<Option<CachedToken>>>,
}

impl GithubAppClient {
    pub(crate) fn new(cfg: GithubAppConfig) -> Self {
        Self {
            http: reqwest::Client::builder()
                .user_agent("agentforge-self-fix")
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
            cfg,
            cache: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    fn now_unix() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
    }

    /// Override the GitHub API base (tests point this at httpmock).
    #[cfg(test)]
    fn api_base() -> String { std::env::var("GITHUB_API_BASE").unwrap_or_else(|_| "https://api.github.com".into()) }
    #[cfg(not(test))]
    fn api_base() -> String { "https://api.github.com".into() }
}
```

Add a `cache_is_reused_until_expiry` unit test that seeds the cache with a future-expiry token and asserts `installation_token()` returns it without an HTTP call (use a sentinel base URL that would fail if hit).

- [ ] **Step 4: Implement the REST operations**

Add methods (all return `agentforge_core::AppResult<T>` or a module error mapped to `ErrorKind::Unavailable`/`Internal`):

```rust
#[derive(Deserialize)] struct InstallTokenResp { token: String, expires_at: String }
#[derive(Deserialize)] pub(crate) struct PullRequest { pub number: i32, pub html_url: String, pub head: PrHead, pub draft: bool, pub mergeable_state: Option<String> }
#[derive(Deserialize)] pub(crate) struct PrHead { pub sha: String }

impl GithubAppClient {
    /// Mint/refresh the installation token (cached until ~60s before expiry).
    async fn installation_token(&self) -> agentforge_core::AppResult<String> { /* JWT -> POST /app/installations/{id}/access_tokens; parse expires_at; cache */ }

    /// origin/main SHA (D3 base pin). GET /repos/{repo}/git/ref/heads/main.
    pub(crate) async fn default_branch_sha(&self) -> agentforge_core::AppResult<String> { /* ... */ }

    /// Create a DRAFT PR. POST /repos/{repo}/pulls { title, body, head, base, draft: true }.
    pub(crate) async fn create_draft_pr(&self, head_branch: &str, base: &str, title: &str, body: &str) -> agentforge_core::AppResult<PullRequest> { /* ... */ }

    /// Required-check conclusions for a head SHA. GET /repos/{repo}/commits/{sha}/check-runs.
    pub(crate) async fn all_required_checks_green(&self, head_sha: &str) -> agentforge_core::AppResult<bool> { /* ... */ }

    /// Current PR head SHA. GET /repos/{repo}/pulls/{n}.
    pub(crate) async fn pr_head_sha(&self, pr_number: i32) -> agentforge_core::AppResult<String> { /* ... */ }

    /// Mark a draft PR ready (GraphQL markPullRequestReadyForReview).
    pub(crate) async fn mark_ready_for_review(&self, pr_node_or_number: i32) -> agentforge_core::AppResult<()> { /* ... */ }

    /// Merge with an expected-head guard. PUT /repos/{repo}/pulls/{n}/merge { sha: expected_head, merge_method }.
    /// GitHub returns 409 if the head moved — map to SelfFixPolicy::head_moved().
    pub(crate) async fn merge_with_expected_head(&self, pr_number: i32, expected_head: &str) -> agentforge_core::AppResult<()> { /* ... */ }

    /// Leave an audit comment. POST /repos/{repo}/issues/{n}/comments.
    pub(crate) async fn comment(&self, pr_number: i32, body: &str) -> agentforge_core::AppResult<()> { /* ... */ }
}
```

Use the reqwest idiom from the scout (Bearer token, `Accept: application/vnd.github+json`). Map non-2xx to typed errors; never log the token or PEM.

- [ ] **Step 5: Write the httpmock integration test for the wire shapes**

Create `rust/crates/api/tests/github_app_client_test.rs` using `httpmock` (add as a `[dev-dependencies]` of `agentforge-api` if absent). Cover, with the mock server bound to `GITHUB_API_BASE`:
- `create_draft_pr` POSTs `draft: true` with the `Accept` + `Authorization: Bearer` headers and parses `{number, html_url, head.sha}`.
- `merge_with_expected_head` sends `sha=<expected>` and maps a mocked `409` to `head_moved`.
- `all_required_checks_green` returns `false` when any check conclusion ≠ `success`.

- [ ] **Step 6: Run + commit**

```bash
cd rust && cargo test -p agentforge-api --test github_app_client_test \
  && cargo test -p agentforge-api --lib services::github_app
git add -A
git commit -m "feat(self-fix): GitHub App REST client (JWT mint, token cache, draft PR, guarded merge)"
```

---

## Milestone 5: Pin `base_commit_sha` at dispatch

**Files:**
- Modify: `rust/crates/api/src/services/orchestration.rs` (assignment/dispatch path, ~line 834 publishes `orchestration.assigned`)
- Test: extend `rust/crates/api/tests/` with a service test using a mock GitHub client

- [ ] **Step 1: Write the failing test**

Add a test asserting: when a `self_fix` task is dispatched and a GitHub client is configured, `base_commit_sha` is written from `default_branch_sha()` before assignment is published; when `self_fix=false`, no GitHub call and `base_commit_sha` stays NULL.

- [ ] **Step 2: Inject the GitHub client**

The dispatch path must be able to resolve the base SHA. Add an `Option<GithubAppClient>` (built from config in `AppState`, see M6 Step 2) to the orchestration service or pass it through the dispatch call. For a `self_fix` task only: call `default_branch_sha()` and `OrchestrationTaskRepository::set_base_commit_sha(scope, task_id, &sha)` before the existing publish. If the client is absent, fail the task with a visible error (`SelfFixPolicy`-style) rather than dispatching a loop that can't open a PR.

- [ ] **Step 3: Run + commit**

```bash
cd rust && cargo test -p agentforge-api --lib services::orchestration
git add -A
git commit -m "feat(self-fix): pin base_commit_sha from origin/main at dispatch"
```

---

## Milestone 6: PR Bridge — import + rebuild + push + draft PR

The security-critical core (spec Security §1). All Git runs in the **server-owned clean clone**, never in `/workspace`.

**Files:**
- Create: `rust/crates/api/src/services/self_fix/mod.rs` (facade + `SelfFixService`)
- Create: `rust/crates/api/src/services/self_fix/import.rs` (validator)
- Create: `rust/crates/api/src/services/self_fix/bridge.rs` (orchestrates the rebuild)
- Modify: `rust/crates/api/src/services/mod.rs` (`pub mod self_fix;`)
- Modify: `rust/crates/api/src/state_services.rs` + `rust/crates/api/src/health.rs` (build the GitHub client + `SelfFixService` on `AppState`)
- Test: `rust/crates/api/src/services/self_fix/import.rs` `#[cfg(test)]` unit tests; `rust/crates/api/tests/self_fix_bridge_rebuild_test.rs` (gated integration)

- [ ] **Step 1: Write the import-validator failing tests**

Create `rust/crates/api/src/services/self_fix/import.rs`:

```rust
//! Vetted file-content import from the untrusted /workspace into a server-owned worktree.
//! No git is ever run against /workspace; this is a filesystem walk with hard rejections.

use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub(crate) enum ImportReject {
    Symlink(String),
    Gitlink(String),
    EscapesRoot(String),
    OutsideAllowlist(String),
    OversizeFile(String),
    ChurnCapExceeded { changed: usize, cap: usize },
    DeletionCapExceeded { deleted: usize, cap: usize },
}

pub(crate) struct ImportLimits { pub max_file_bytes: u64, pub max_changed_files: usize, pub max_deletions: usize }
impl Default for ImportLimits {
    fn default() -> Self { Self { max_file_bytes: 2 * 1024 * 1024, max_changed_files: 400, max_deletions: 200 } }
}

/// Decide whether a single relative path may be imported. Pure; `is_symlink`/`mode`
/// are passed in so tests don't touch the filesystem.
pub(crate) fn classify_entry(rel_path: &str, is_symlink: bool, is_gitlink: bool, size: u64, limits: &ImportLimits)
    -> Result<(), ImportReject>
{
    if is_symlink { return Err(ImportReject::Symlink(rel_path.into())); }
    if is_gitlink { return Err(ImportReject::Gitlink(rel_path.into())); }
    if path_escapes(rel_path) { return Err(ImportReject::EscapesRoot(rel_path.into())); }
    if rel_path.starts_with(".git/") || rel_path == ".git" { return Err(ImportReject::OutsideAllowlist(rel_path.into())); }
    if size > limits.max_file_bytes { return Err(ImportReject::OversizeFile(rel_path.into())); }
    Ok(())
}

/// Reject `..`, absolute, or empty components after normalization.
fn path_escapes(rel: &str) -> bool {
    if rel.starts_with('/') { return true; }
    Path::new(rel).components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::RootDir))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lim() -> ImportLimits { ImportLimits::default() }

    #[test] fn rejects_symlink() { assert_eq!(classify_entry("a.rs", true, false, 10, &lim()), Err(ImportReject::Symlink("a.rs".into()))); }
    #[test] fn rejects_gitlink() { assert_eq!(classify_entry("sub", false, true, 10, &lim()), Err(ImportReject::Gitlink("sub".into()))); }
    #[test] fn rejects_parent_escape() { assert!(matches!(classify_entry("../etc/passwd", false, false, 1, &lim()), Err(ImportReject::EscapesRoot(_)))); }
    #[test] fn rejects_absolute() { assert!(matches!(classify_entry("/etc/passwd", false, false, 1, &lim()), Err(ImportReject::EscapesRoot(_)))); }
    #[test] fn rejects_dotgit() { assert!(matches!(classify_entry(".git/config", false, false, 1, &lim()), Err(ImportReject::OutsideAllowlist(_)))); }
    #[test] fn rejects_oversize() { assert!(matches!(classify_entry("big.bin", false, false, 9_000_000, &lim()), Err(ImportReject::OversizeFile(_)))); }
    #[test] fn accepts_regular_file() { assert_eq!(classify_entry("rust/crates/api/src/x.rs", false, false, 100, &lim()), Ok(())); }
}
```

- [ ] **Step 2: Run the validator tests (fail → pass)**

Run:
```bash
cd rust && cargo test -p agentforge-api --lib services::self_fix::import
```
Expected: PASS once the module compiles (register `pub(crate) mod import;` in `self_fix/mod.rs`, Step 4).

- [ ] **Step 3: Write the bridge orchestration (and the gated rebuild test)**

Create `rust/crates/api/src/services/self_fix/bridge.rs`. The `run(...)` flow (each step a private fn for unit-testing where pure):

1. **Stop the container** for the task's agent (TOCTOU freeze) via the existing container-control service (`AgentContainerControlService::stop`-equivalent — re-read its real method name) before reading `/workspace`.
2. **Resolve paths**: workspace project dir via `agent_workspace::workspace_projects_root()` + project name; reject if it escapes (reuse `safe_join_under`).
3. **Prepare the clean clone** in a server-owned scratch dir (e.g. `${SELF_FIX_WORK_DIR:-/tmp/agentforge-selffix}/<task-id>`): `git clone --depth 50 https://x-access-token:<token>@github.com/<repo>.git <dir>` (token from the GitHub client), then `git -C <dir> worktree add --detach <wt> <base_commit_sha>`. The token is only ever in this clone's process env — never in `/workspace`.
4. **Import**: walk the project dir (excluding `.git/`), for each entry call `classify_entry`; on any `Err`, abort the task with a visible reason (increment `self_fix_import_rejected_total{reason}`). Mirror the worktree to match the agent's content (copy regular files; capture deletions) honouring the **base tree's** `.gitignore`. Enforce churn/deletion caps across the whole set.
5. **Commit** in the worktree: `git -C <wt> -c user.name=... -c user.email=... checkout -b agent/<task-id>` then `add -A` + `commit -m`. Run git with `-c core.hooksPath=/dev/null` and `GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null` (defence in depth; the clone is server-owned anyway).
6. **Object-level re-check**: `git -C <wt> diff-tree -r --raw <base_commit_sha> HEAD`; reject if any dst mode is `120000` (symlink) or `160000` (gitlink), or any path is sensitive-per-M2 is recorded (not rejected — sensitivity gates *merge*, not PR creation) — but symlink/gitlink here is a hard abort.
7. **Push**: `git -C <wt> push origin agent/<task-id>` (idempotent: if the ref exists with the same tree, reuse).
8. **Open draft PR** via `GithubAppClient::create_draft_pr(head="agent/<task-id>", base="main", title, body)` where `body` includes a generated `## Beginner UX / Operator Path` section (see `feedback_pr_body_edit_and_ux_gate`).
9. **Persist**: `set_pr_metadata(scope, task_id, pr.number, &pr.html_url, &pr.head.sha, review_status::IN_REVIEW)`. If the change set touches a sensitive path (M2), set `review_status::SENSITIVE_BLOCKED` instead.
10. On any step error: stop the task with a visible error; never leave a half-written PR (idempotency keyed by `agent/<task-id>` + open-or-find).

All `git` invocations use `tokio::process::Command` with `.kill_on_drop(true)` + `tokio::time::timeout` (scout idiom).

Create `rust/crates/api/tests/self_fix_bridge_rebuild_test.rs`, **gated** like the Redis/NATS tests (skip with `eprintln!` unless `SELF_FIX_IT=1` and `git` is available). It should, against a local temp git repo (no GitHub): build a clean clone from a `file://` origin, import a small change from a fake `/workspace` tree containing a symlink + a normal file, and assert the symlink is rejected and a clean rebuild of the normal file produces a one-file diff with no symlink/gitlink in `diff-tree --raw`.

- [ ] **Step 4: Register modules + build `SelfFixService` on state**

`rust/crates/api/src/services/self_fix/mod.rs`:
```rust
pub(crate) mod bridge;
pub(crate) mod import;
pub(crate) mod merge_executor; // added in M7

pub(crate) struct SelfFixService { /* repo, github client, container control, workspace, limits */ }
impl SelfFixService { /* from_state-style constructor; pub(crate) async fn open_pr(...), pub(crate) async fn approve_and_merge(...) */ }
```
Add `pub mod self_fix;` and `pub mod github_app;` to `services/mod.rs`. On `AppState` (build in `state_services.rs`): construct an `Option<GithubAppClient>` by decrypting `github_app_private_key` with `agentforge_core::crypto::decode_key_hex(LLM_ENCRYPTION_KEY)` + `decrypt_base64` (scout `cfg` card), and a `self_fix_service()` factory following the `agent_container_control_service()` pattern.

- [ ] **Step 5: Run + commit**

```bash
cd rust && cargo test -p agentforge-api --lib services::self_fix \
  && SELF_FIX_IT=1 cargo test -p agentforge-api --test self_fix_bridge_rebuild_test
git add -A
git commit -m "feat(self-fix): PR Bridge — vetted import, server-side rebuild, draft PR"
```

---

## Milestone 7: Merge Executor (guarded)

**Files:**
- Create: `rust/crates/api/src/services/self_fix/merge_executor.rs`
- Test: `merge_executor.rs` `#[cfg(test)]` + a service test with a mock GitHub client

- [ ] **Step 1: Write the failing guard tests**

In `merge_executor.rs`, test the pure precondition gate first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // MergeGate::evaluate(sensitive, checks_green, head_unchanged) -> Result<(), AppError>
    #[test] fn refuses_sensitive_path() { assert!(MergeGate::evaluate(true, true, true).is_err()); }
    #[test] fn refuses_red_ci() { assert!(MergeGate::evaluate(false, false, true).is_err()); }
    #[test] fn refuses_moved_head() { assert!(MergeGate::evaluate(false, true, false).is_err()); }
    #[test] fn allows_all_green_clear_unchanged() { assert!(MergeGate::evaluate(false, true, true).is_ok()); }
}
```

- [ ] **Step 2: Implement the gate + executor**

```rust
pub(crate) struct MergeGate;
impl MergeGate {
    pub(crate) fn evaluate(sensitive: bool, checks_green: bool, head_unchanged: bool) -> agentforge_core::AppResult<()> {
        use crate::domain::self_fix::SelfFixPolicy;
        if sensitive { return Err(SelfFixPolicy::sensitive_path_blocked()); }
        if !checks_green { return Err(SelfFixPolicy::checks_not_green()); }
        if !head_unchanged { return Err(SelfFixPolicy::head_moved()); }
        Ok(())
    }
}
```

`MergeExecutor::run(scope, task)` (the atomic guarded tail, spec Security §5 + Merge race):
1. Load the task; recompute the change-set sensitivity (M2) — **hard refuse** server-side regardless of any GitHub state.
2. `all_required_checks_green(pr_head_sha)`; `head_unchanged = github.pr_head_sha(n) == task.pr_head_sha`.
3. `MergeGate::evaluate(...)?`.
4. `github.mark_ready_for_review(n)`.
5. **Re-read** `head = github.pr_head_sha(n)` and re-check green (a `ready_for_review` automation could have pushed).
6. `github.merge_with_expected_head(n, head)` — GitHub rejects (409 → `head_moved`) if it moved; that is the atomic guard.
7. `github.comment(n, audit_body)` (approver, task id, timestamp, head sha); `set_review_status(scope, task, review_status::MERGED)`.
Idempotent: if the PR is already merged, treat as success.

- [ ] **Step 3: Run + commit**

```bash
cd rust && cargo test -p agentforge-api --lib services::self_fix::merge_executor
git add -A
git commit -m "feat(self-fix): guarded Merge Executor (sensitive hard-refuse, expected-head merge)"
```

---

## Milestone 8: Review/approve routes (authed, tenant-scoped)

**Files:**
- Create: `rust/crates/api/src/routes/self_fix.rs`
- Modify: `rust/crates/api/src/routes/mod.rs` (`pub mod self_fix;`)
- Modify: `rust/crates/api/src/router.rs` (`.merge(routes::self_fix::self_fix_routes())`)
- Modify: `rust/crates/api/src/state_services.rs` (`self_fix_service()` factory — done in M6)
- Test: `rust/crates/api/tests/self_fix_routes_test.rs`

Follow the service+route pattern card verbatim (AuthUser extractor, `&auth.scope`, `{ok:true,...}` via a domain response helper, `route → service → domain → repository`).

- [ ] **Step 1: Implement the routes**

```rust
//! GET  /self-fix/tasks/{id}/review  -> PR snapshot (diff url, head sha, check status, review_status, sensitive flag)
//! POST /self-fix/tasks/{id}/approve -> record approved + invoke MergeExecutor (server-side gate)

async fn get_review(State(state): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>)
    -> AppResult<Json<Value>>
{
    let snapshot = state.self_fix_service().review_snapshot(&auth.scope, id).await?;
    Ok(Json(self_fix_data_response(snapshot)))
}

async fn approve(State(state): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>)
    -> AppResult<Json<Value>>
{
    // service: refuse if review_status == SENSITIVE_BLOCKED; else set approved -> MergeExecutor::run
    let result = state.self_fix_service().approve_and_merge(&auth.scope, id, &auth.claims.sub).await?;
    Ok(Json(self_fix_data_response(result)))
}

pub fn self_fix_routes() -> Router<AppState> {
    Router::new()
        .route("/self-fix/tasks/{id}/review", get(get_review))
        .route("/self-fix/tasks/{id}/approve", post(approve))
}
```

`review_snapshot` reads the task PR columns (M1) + live check status via the GitHub client; it sets the `sensitive` flag from M2 so the FE can disable Approve. `approve_and_merge` enforces the sensitive hard-refuse before calling `MergeExecutor`.

- [ ] **Step 2: Register the routes + module**

Add `pub mod self_fix;` to `routes/mod.rs` and `.merge(routes::self_fix::self_fix_routes())` inside the `api_v1` router in `router.rs` (behind the standard auth path — the `AuthUser` extractor enforces it).

- [ ] **Step 3: Write tenant + behavior tests**

`rust/crates/api/tests/self_fix_routes_test.rs`: assert (a) a foreign-org caller gets no access to another org's task review; (b) `approve` on a `SENSITIVE_BLOCKED` task returns the forbidden code and does **not** call merge; (c) `approve` on a clean, green task transitions `review_status` to `merged` (mock GitHub client). Also confirm the route DDD-boundary guard test still passes (services use domain error helpers, not raw `ErrorKind`).

- [ ] **Step 4: Run + commit**

```bash
cd rust && cargo test -p agentforge-api --test self_fix_routes_test \
  && cargo test -p agentforge-api --lib route_ddd_boundary
git add -A
git commit -m "feat(self-fix): authed review snapshot + approve→merge routes"
```

---

## Milestone 9: Frontend review surface (FSD)

**Files:**
- Create: `src/app/features/detail/ReviewSnapshotPanel.tsx`
- Modify: `src/app/features/detail/TaskDetailPanel.tsx` (render the panel for `self_fix` tasks)
- Modify: `src/app/shared/api/orchestration.ts` (`TaskSummary` PR fields + `getSelfFixReview` + `approveSelfFix`)
- Modify: `shared/types/` (keep Rust serializers ↔ TS types in sync)
- Test: `tests/` vitest for the panel

- [ ] **Step 1: Extend the API client + types**

In `src/app/shared/api/orchestration.ts`, extend `TaskSummary` with optional `selfFix?: boolean; prNumber?: number; prUrl?: string; prHeadSha?: string; reviewStatus?: 'in_review'|'approved'|'changes_requested'|'merged'|'sensitive_blocked'` and add:

```ts
async getSelfFixReview(taskId: string): Promise<{ ok: boolean; data: SelfFixReview }> {
  return http.get(`/self-fix/tasks/${taskId}/review`)
},
async approveSelfFix(taskId: string): Promise<{ ok: boolean; data: { reviewStatus: string } }> {
  return http.post(`/self-fix/tasks/${taskId}/approve`, {})
},
```

Mirror `SelfFixReview` (diffUrl, headSha, checksGreen, sensitive, reviewStatus) in `shared/types/` and the Rust `review_snapshot` serializer field names exactly.

- [ ] **Step 2: Build the panel**

`src/app/features/detail/ReviewSnapshotPanel.tsx`: shows the PR link, a one-shot CI-check status (with a manual Refresh), and an **Approve** button **disabled** when `!checksGreen || sensitive`. On click → `approveSelfFix(task.id)` then `upsertTask`. Gate visibility on `task.selfFix === true`. Reuse the `handleRecovery`-style optimistic update but add an error banner on failure (the existing pattern swallows errors — do not repeat that; surface the server error code).

- [ ] **Step 3: Wire into the detail panel**

In `TaskDetailPanel.tsx`, when `task.selfFix`, render `<ReviewSnapshotPanel task={task} />` as a tab/section. Do **not** reuse the `waiting_approval` recovery button for this (plan D4).

- [ ] **Step 4: Test + checks**

Add a vitest rendering test (Approve disabled when `sensitive` / red CI; enabled + calls API when green+clear). Run:
```bash
npm run fsd:check && npm run lint && npm run typecheck && npm run test:unit -- ReviewSnapshotPanel
```
Expected: PASS, no FSD boundary violation.

- [ ] **Step 5: Commit**

```bash
git add src/app shared/types tests
git commit -m "feat(self-fix): in-platform PR review snapshot + approve surface"
```

---

## Milestone 10: End-to-end wiring, docs, validation

**Files:**
- Modify: WS broadcast for review-state changes (`rust/crates/api/src/.../websocket` + `src/app/hooks/useWsDispatch.ts`)
- Create/modify: docs — `docs/guides/self-fix-loop.md`, `docs/guides/configuration.md` (`GITHUB_APP_*`, `SELF_FIX_WORK_DIR`), `docs/architecture/glossary.md`, `docs/security/` note
- Modify: `docker/compose.yml` / `docker/.env.example` (`GITHUB_APP_*`, `SELF_FIX_WORK_DIR`)

- [ ] **Step 1: Broadcast review-state changes**

When `set_review_status`/`set_pr_metadata` change a task, broadcast over the existing `orchestration:task_update` WS path so the board reflects PR + review state (`useWsDispatch.ts:22-32` already calls `upsertTask`). Add the PR fields to the task projection the WS sends.

- [ ] **Step 2: Docs (operator-first, per CLAUDE.md product standard)**

`docs/guides/self-fix-loop.md`: prerequisites (a GitHub App with `contents:write` + `pull_requests:write`, the four `GITHUB_APP_*` vars, `LLM_ENCRYPTION_KEY`), the shortest happy path (create a `self_fix` task → review the draft PR → Approve → merged), what success looks like, and a status/troubleshooting table. Add `GITHUB_APP_*` + `SELF_FIX_WORK_DIR` to `docs/guides/configuration.md`; add "self-fix loop", "PR Bridge", "Merge Executor", "sensitive-path circuit breaker" to the glossary; add a security note that sensitive paths are server-side hard-refused and never auto-merged.

- [ ] **Step 3: Beginner-UX PR body**

Ensure the Bridge-generated PR body includes the exact `## Beginner UX / Operator Path` H2 with plain `- Field:` bullets (6 named fields, values ≥12 chars) per `feedback_pr_body_edit_and_ux_gate`, so agent-authored PRs pass the same UX gate as human PRs.

- [ ] **Step 4: Full validation**

Run (blast radius touches shared crates, API contracts, DB, auth-adjacent):
```bash
cd rust && make ci
cd .. && npm run fsd:check && npm run lint && npm run format:check && npm run typecheck && npm run test
```
Confirm: migration-manifest CI guard, `route_ddd_boundary_test`, every new repo/route method has a tenant-boundary test, and no `clippy::unwrap_used` in handlers. If a new lazy FE route was added, apply the `waitFor({ state: 'visible' })` + `click({ timeout: 30000 })` Playwright guidance from CLAUDE.md.

- [ ] **Step 5: Commit + open the feature PR**

```bash
git add -A
git commit -m "feat(self-fix): e2e wiring, realtime broadcast, operator docs"
```
Open the PR from `docs/self-fix-loop-phase4-spec` (or a dedicated feature branch) with concrete validation evidence and the Beginner-UX body section.

---

## Self-review (planning)

- **Spec coverage:** Bridge/import/rebuild (Security §1) → M6; base-SHA pin (§1, D3) → M5; sensitive-path breaker with real globs + own-code (§6, findings #3/#4) → M2; server-side hard refusal (§6, #4) → M7/M8; symlink/gitlink/mode + churn caps (§1, #5) → M6 import + object re-check; container-stop TOCTOU (#6) → M6 Step 3.1; clean-clone hygiene (#7) → M6 (ephemeral per-task dir); draft→ready→guarded merge (#8) → M7; ReviewState vocabulary / no invented task statuses (#9, D2/D4) → M1 `review_status` + M8; GitHub App not PAT (§3) → M3/M4; data model (§Data model) → M1; observability counters (§Observability) → referenced in M6/M7 (`self_fix_import_rejected_total{reason}` etc. — add the counters as you write each stage); testing (§Testing) → per-milestone TDD + M10. **Gap intentionally deferred:** auto-deploy, background dispatch, auto-merge (spec "Explicitly deferred").
- **Open questions from the spec** resolved here: CI-check freshness → snapshot + manual Refresh (M8/M9); `self-fix` marker → a `self_fix BOOLEAN` flag on the task (M1), not a task group.
- **Known unknowns to confirm at execution time (do not trust planning line numbers):** the exact `CreateTaskRow` bind count (M1 Step 6); the real container-stop method name on the control service (M6 Step 3.1); whether `jsonwebtoken` is exposed as `workspace = true` (M4 Step 2); the exact WS task-projection type (M10 Step 1).
- **Type consistency:** `review_status` string vocabulary (`in_review`/`approved`/`changes_requested`/`merged`/`sensitive_blocked`) is defined once in `domain::self_fix::review_status` (M2) and reused by M6/M7/M8 and mirrored in the FE union (M9). `SensitivePathPolicy::touches_sensitive(&[String])` signature is fixed in M2 and consumed unchanged in M6/M7/M8.
```
