# Self-Fix PR Auto-Trigger Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a `self_fix` task reaches `completed`, automatically and durably enqueue a PR-bridge job that drives `SelfFixService::open_pr`, turning the manually-pumped self-fix loop into an event-driven (L3) loop.

**Architecture:** Reuse the existing PG `job_queue` substrate. `complete_task` inserts a `self_fix_pr` job _inside the same transaction_ as `set_result_in_tx`, so the trigger commits atomically with the completion (never lost, never fired for an uncommitted completion). A new `SelfFixPrWorker` (modeled on `ProjectCloneWorker`) dequeues those jobs and calls `open_pr`. No new table and no migration — `job_queue` already exists.

**Tech Stack:** Rust, Axum, SQLx (Postgres), Tokio, `metrics` crate, `agentforge_jobs::queue`.

## Global Constraints

- Backend ownership is Rust; all work is under `rust/`. Active backend only.
- Keep the route → service → domain → repository split. New cross-cutting protocol constants/types go in `rust/crates/core`.
- Tenant-scoped repository methods accept `&TenantScope` and constrain by organization. Background workers build a system scope: `TenantScope::new(OrgId::from(uuid), UserId::from(Uuid::nil()))` (precedent: `rust/crates/api/src/services/project_clone_worker.rs:1372`).
- `clippy::unwrap_used` is denied in handler code. Use typed errors; map to `AppError`.
- No migration in this plan. `job_queue` and its `idx_job_queue_unique_key` partial unique index already exist (migration 068).
- All worktrees share one cargo target (`/data/agentforge/.cargo-shared-target`); never run cargo in two worktrees at once.
- `#[sqlx::test]` local recipe: point `DATABASE_URL` at the role-owned `af_sqlx_bookkeep` DB (see `reference_sqlx_test_local_db`).
- Adding a struct field to a type built with a `Struct { .. }` literal breaks every literal site (entity-literal-fanout trap). This plan adds one field to the **core `Config`** struct — grep and fix all `Config {` literal sites and verify `cargo test -p agentforge-core --lib --no-run`.
- Validation per CLAUDE.md: run the narrow crate test first, then `cd rust && make ci` because this touches API contracts and orchestration.

## File Structure

- `rust/crates/jobs/src/queue.rs` — add `enqueue_in_tx` (transactional sibling of `enqueue`).
- `rust/crates/jobs/src/lib.rs` — re-export `enqueue_in_tx`.
- `rust/crates/core/src/self_fix_protocol.rs` (new) — `SELF_FIX_PR_QUEUE` constant + `SelfFixPrJob` payload type.
- `rust/crates/core/src/lib.rs` — register `self_fix_protocol` module + re-export.
- `rust/crates/core/src/config.rs` — add `self_fix_pr_worker_enabled` flag.
- `rust/crates/api/src/services/orchestration.rs` — enqueue the job in-tx inside `complete_task`.
- `rust/crates/api/src/services/self_fix_pr_worker.rs` (new) — the worker + `register_metrics`.
- `rust/crates/api/src/services/mod.rs` — register the new module; drop `#[allow(dead_code)]` on the now-live self-fix chain.
- `rust/crates/api/src/state_services.rs` — add `pub fn AppState::self_fix_pr_worker(&self)`.
- `rust/bins/server/src/main.rs` — register metrics + spawn the worker behind the flag.

---

### Task 1: `enqueue_in_tx` — transactional enqueue

**Files:**

- Modify: `rust/crates/jobs/src/queue.rs`
- Modify: `rust/crates/jobs/src/lib.rs:85`
- Test: `rust/crates/jobs/src/queue.rs` (tests module)

**Interfaces:**

- Produces: `pub async fn enqueue_in_tx(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, queue: &str, payload: serde_json::Value, priority: i32, run_at: Option<DateTime<Utc>>, unique_key: Option<&str>, max_attempts: i32) -> Result<Option<Uuid>, sqlx::Error>`

- [ ] **Step 1: Write the failing test** (append to the `tests` module in `queue.rs`)

```rust
    #[sqlx::test]
    async fn enqueue_in_tx_commits_and_is_dequeueable(pool: PgPool) {
        let mut tx = pool.begin().await.unwrap();
        let id = enqueue_in_tx(
            &mut tx,
            "self_fix_pr",
            serde_json::json!({"task_id": "t1"}),
            0,
            None,
            Some("uk-1"),
            5,
        )
        .await
        .unwrap();
        assert!(id.is_some());
        tx.commit().await.unwrap();

        let job = dequeue(&pool, "self_fix_pr", "worker-test").await.unwrap();
        assert!(job.is_some(), "committed job must be dequeueable");
        assert_eq!(job.unwrap().queue, "self_fix_pr");
    }

    #[sqlx::test]
    async fn enqueue_in_tx_rolls_back_with_the_outer_tx(pool: PgPool) {
        let mut tx = pool.begin().await.unwrap();
        enqueue_in_tx(&mut tx, "self_fix_pr", serde_json::json!({}), 0, None, Some("uk-2"), 5)
            .await
            .unwrap();
        tx.rollback().await.unwrap();

        let job = dequeue(&pool, "self_fix_pr", "worker-test").await.unwrap();
        assert!(job.is_none(), "rolled-back job must not exist");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd rust && DATABASE_URL=postgres://...af_sqlx_bookkeep cargo test -p agentforge-jobs enqueue_in_tx`
Expected: FAIL — `cannot find function enqueue_in_tx`.

- [ ] **Step 3: Implement `enqueue_in_tx`** (add directly below `enqueue` in `queue.rs`, after line 69)

```rust
/// Transactional sibling of [`enqueue`]. Inserts the job using the caller's
/// transaction so the enqueue commits atomically with the surrounding write
/// (e.g. a task completion). The job row itself is the durable trigger record —
/// no separate outbox/relay is needed for in-process workers.
pub async fn enqueue_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    queue: &str,
    payload: Value,
    priority: i32,
    run_at: Option<DateTime<Utc>>,
    unique_key: Option<&str>,
    max_attempts: i32,
) -> Result<Option<Uuid>, sqlx::Error> {
    let run_at = run_at.unwrap_or_else(Utc::now);
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO job_queue (queue, payload, priority, run_at, unique_key, max_attempts)
           VALUES ($1, $2, $3, $4, $5, $6)
           ON CONFLICT (unique_key) WHERE unique_key IS NOT NULL DO NOTHING
           RETURNING id"#,
    )
    .bind(queue)
    .bind(&payload)
    .bind(priority)
    .bind(run_at)
    .bind(unique_key)
    .bind(max_attempts)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(id)
}
```

- [ ] **Step 4: Re-export it** — change `rust/crates/jobs/src/lib.rs:85`

```rust
pub use queue::{JobEntry, complete, dequeue, enqueue, enqueue_in_tx, fail, release_stale_locks};
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd rust && cargo test -p agentforge-jobs enqueue_in_tx`
Expected: PASS (both tests).

- [ ] **Step 6: Commit**

```bash
git add rust/crates/jobs/src/queue.rs rust/crates/jobs/src/lib.rs
git commit -m "feat(jobs): add enqueue_in_tx for transactional job enqueue"
```

---

### Task 2: Self-fix PR protocol (queue name + payload type)

**Files:**

- Create: `rust/crates/core/src/self_fix_protocol.rs`
- Modify: `rust/crates/core/src/lib.rs`

**Interfaces:**

- Produces: `pub const SELF_FIX_PR_QUEUE: &str = "self_fix_pr";` and `pub struct SelfFixPrJob { pub task_id: Uuid, pub org_id: Uuid }` (Serialize + Deserialize).

- [ ] **Step 1: Write the failing test** (in the new file)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_fix_pr_job_roundtrips() {
        let job = SelfFixPrJob { task_id: Uuid::nil(), org_id: Uuid::nil() };
        let json = serde_json::to_value(&job).unwrap();
        let back: SelfFixPrJob = serde_json::from_value(json).unwrap();
        assert_eq!(back.task_id, job.task_id);
        assert_eq!(back.org_id, job.org_id);
        assert_eq!(SELF_FIX_PR_QUEUE, "self_fix_pr");
    }
}
```

- [ ] **Step 2: Create the module** — `rust/crates/core/src/self_fix_protocol.rs`

```rust
//! Self-fix PR-bridge job protocol shared between the producer (`complete_task`)
//! and the consumer (`SelfFixPrWorker`). The queue name is the `job_queue.queue`
//! discriminator (mirrors `clone_protocol::CLONE_JOB_QUEUE`).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// `job_queue.queue` value the self-fix PR worker dequeues.
pub const SELF_FIX_PR_QUEUE: &str = "self_fix_pr";

/// Payload of a self-fix PR-bridge job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfFixPrJob {
    pub task_id: Uuid,
    pub org_id: Uuid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_fix_pr_job_roundtrips() {
        let job = SelfFixPrJob { task_id: Uuid::nil(), org_id: Uuid::nil() };
        let json = serde_json::to_value(&job).unwrap();
        let back: SelfFixPrJob = serde_json::from_value(json).unwrap();
        assert_eq!(back.task_id, job.task_id);
        assert_eq!(back.org_id, job.org_id);
        assert_eq!(SELF_FIX_PR_QUEUE, "self_fix_pr");
    }
}
```

- [ ] **Step 3: Register + re-export the module** in `rust/crates/core/src/lib.rs` (mirror the existing `clone_protocol` lines)

```rust
pub mod self_fix_protocol;
pub use self_fix_protocol::{SELF_FIX_PR_QUEUE, SelfFixPrJob};
```

- [ ] **Step 4: Run the test**

Run: `cd rust && cargo test -p agentforge-core self_fix_pr_job_roundtrips`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rust/crates/core/src/self_fix_protocol.rs rust/crates/core/src/lib.rs
git commit -m "feat(core): add self-fix PR job protocol (queue + payload)"
```

---

### Task 3: Config flag `self_fix_pr_worker_enabled`

**Files:**

- Modify: `rust/crates/core/src/config.rs` (struct near line 475; `default_true` exists at line 41)

**Interfaces:**

- Produces: `config.self_fix_pr_worker_enabled: bool` (default `true`, env `SELF_FIX_PR_WORKER_ENABLED`).

- [ ] **Step 1: Add the field** to the `Config` struct, next to `project_clone_worker_enabled`

```rust
    /// Enable the self-fix PR-bridge worker. `true` (default) starts the worker
    /// that dequeues `self_fix_pr` jobs and drives `SelfFixService::open_pr`.
    /// Set `false` to keep PR opening manual (e.g. while the GitHub App is
    /// unconfigured). Env: `SELF_FIX_PR_WORKER_ENABLED`.
    #[serde(default = "default_true")]
    pub self_fix_pr_worker_enabled: bool,
```

- [ ] **Step 2: Fix all `Config { .. }` literal sites** (entity-literal-fanout trap)

Run: `cd rust && grep -rn "Config {" crates bins | grep -v "AppConfig\|//"`
For each literal that constructs the core `Config`, add `self_fix_pr_worker_enabled: true,`. Most code uses serde defaults or `..Default::default()`, so expect few or zero.

- [ ] **Step 3: Verify the crate builds (lib + tests)**

Run: `cd rust && cargo test -p agentforge-core --lib --no-run`
Expected: compiles (no missing-field errors).

- [ ] **Step 4: Commit**

```bash
git add rust/crates/core/src/config.rs
git commit -m "feat(config): add self_fix_pr_worker_enabled flag"
```

---

### Task 4: Enqueue the PR job in-tx on self-fix completion

**Files:**

- Modify: `rust/crates/api/src/services/orchestration.rs:585-621` (`complete_task`)
- Test: `rust/crates/api/tests/` (new `#[sqlx::test]`, or extend an existing orchestration test file)

**Interfaces:**

- Consumes: `enqueue_in_tx` (Task 1), `SELF_FIX_PR_QUEUE` + `SelfFixPrJob` (Task 2), `updated.self_fix` from `set_result_in_tx` (returns `OrchestrationTask`, `self_fix: bool` at `entities.rs:476`).

- [ ] **Step 1: Write the failing test**

```rust
    #[sqlx::test]
    async fn completing_self_fix_task_enqueues_one_pr_job(pool: PgPool) {
        let (svc, scope, task_id) = seed_self_fix_task(&pool, true).await;
        svc.complete_task(&scope, task_id, serde_json::json!({"ok": true})).await.unwrap();

        let job = agentforge_jobs::queue::dequeue(&pool, agentforge_core::SELF_FIX_PR_QUEUE, "t").await.unwrap();
        let job = job.expect("a self_fix_pr job must be enqueued");
        let payload: agentforge_core::SelfFixPrJob = serde_json::from_value(job.payload).unwrap();
        assert_eq!(payload.task_id, task_id);
    }

    #[sqlx::test]
    async fn completing_non_self_fix_task_enqueues_nothing(pool: PgPool) {
        let (svc, scope, task_id) = seed_self_fix_task(&pool, false).await;
        svc.complete_task(&scope, task_id, serde_json::json!({"ok": true})).await.unwrap();

        let job = agentforge_jobs::queue::dequeue(&pool, agentforge_core::SELF_FIX_PR_QUEUE, "t").await.unwrap();
        assert!(job.is_none(), "non-self_fix completion must not enqueue a PR job");
    }
```

> `seed_self_fix_task(&pool, self_fix)` inserts an organization, an agent, and an `orchestration_tasks` row in `working` status with `self_fix = $self_fix` assigned to that agent, and returns the built `OrchestrationService`, a `TenantScope` for the org, and the task id. Mirror the seeding already used by the nearest existing `complete_task` / orchestration `#[sqlx::test]`; reuse that helper if one exists.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd rust && cargo test -p agentforge-api completing_self_fix_task_enqueues_one_pr_job`
Expected: FAIL — no job enqueued (dequeue returns `None`).

- [ ] **Step 3: Add the in-tx enqueue** in `complete_task`, immediately after the `unblock_children_of_in_tx` call and **before** `tx.commit()` (orchestration.rs ~620)

```rust
        let unblocked_children =
            OrchestrationTaskRepository::unblock_children_of_in_tx(&mut tx, scope, task_id).await?;

        // Event-driven self-fix trigger: when this task is a self-fix task,
        // enqueue the PR-bridge job inside the SAME transaction as the result
        // write. The job commits atomically with the completion, so a crash
        // never loses the trigger and it never fires for an uncommitted
        // completion. `unique_key = task_id` makes re-completion idempotent
        // (ON CONFLICT DO NOTHING).
        if updated.self_fix {
            let payload = serde_json::to_value(agentforge_core::SelfFixPrJob {
                task_id,
                org_id: scope.org_id().as_uuid(),
            })
            .map_err(|err| agentforge_core::AppError::from(anyhow::Error::from(err)))?;
            agentforge_jobs::queue::enqueue_in_tx(
                &mut tx,
                agentforge_core::SELF_FIX_PR_QUEUE,
                payload,
                0,
                None,
                Some(&task_id.to_string()),
                5,
            )
            .await
            .map_err(|err| agentforge_core::AppError::from(anyhow::Error::from(err)))?;
        }

        tx.commit().await.map_err(|err| OrchestrationTransactionPolicy::commit_failed("complete_task", err))?;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd rust && cargo test -p agentforge-api completing_self_fix_task_enqueues`
Expected: PASS (both).

- [ ] **Step 5: Commit**

```bash
git add rust/crates/api/src/services/orchestration.rs rust/crates/api/tests/
git commit -m "feat(orchestration): enqueue self-fix PR job in-tx on completion"
```

---

### Task 5: `SelfFixPrWorker` — dequeue and drive `open_pr`

**Files:**

- Create: `rust/crates/api/src/services/self_fix_pr_worker.rs`
- Modify: `rust/crates/api/src/services/mod.rs` (add `pub mod self_fix_pr_worker;`)
- Modify: `rust/crates/api/src/services/self_fix/mod.rs` — remove `#[allow(dead_code)]` on `SelfFixService` (line 52), `new` (line 63), and `open_pr` (line 84), now that they are live.
- Test: `rust/crates/api/tests/` (new `#[sqlx::test]`)

**Interfaces:**

- Consumes: `agentforge_core::{SELF_FIX_PR_QUEUE, SelfFixPrJob, TenantScope, OrgId, UserId}`, `agentforge_jobs::queue::{dequeue, complete, fail}`, `SelfFixService::open_pr`.
- Produces: `pub struct SelfFixPrWorker { pool: PgPool, service: Arc<SelfFixService>, worker_id: String }`, `pub fn new(pool, service) -> Self`, `pub async fn run(self, shutdown: watch::Receiver<bool>)`, `pub async fn dequeue_and_process(&self) -> AppResult<bool>`, `pub fn register_metrics()`.

- [ ] **Step 1: Write the failing test**

```rust
    #[sqlx::test]
    async fn worker_fails_job_when_github_unconfigured(pool: PgPool) {
        // Seed a completed self_fix task + its enqueued PR job.
        let (scope, task_id) = seed_completed_self_fix_with_job(&pool).await;
        // Build a worker whose SelfFixService has github = None (open_pr fails visibly).
        let worker = build_self_fix_pr_worker_for_test(&pool);

        let processed = worker.dequeue_and_process().await.unwrap();
        assert!(processed, "a queued job should be processed");

        // github=None => open_pr errors => queue::fail bumps attempts; the job is
        // not silently completed/deleted.
        let row: (String, i32) =
            sqlx::query_as("SELECT status, attempts FROM job_queue WHERE queue = $1 AND unique_key = $2")
                .bind(agentforge_core::SELF_FIX_PR_QUEUE)
                .bind(task_id.to_string())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(row.1 >= 1, "attempt count must increment on failure");
        let _ = scope;
    }
```

> `build_self_fix_pr_worker_for_test(&pool)` constructs `SelfFixService::new(...)` with `github = None`, a temp `workspace_root`, and `ImportLimits::default()`, wraps it in `Arc`, and returns `SelfFixPrWorker::new(pool.clone(), service)`. `seed_completed_self_fix_with_job` inserts the org + a `completed` `self_fix` task and an enqueued `self_fix_pr` job (reuse Task 4's seeding helper + `enqueue`).

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd rust && cargo test -p agentforge-api worker_fails_job_when_github_unconfigured`
Expected: FAIL — `self_fix_pr_worker` module / types not found.

- [ ] **Step 3: Implement the worker** — `rust/crates/api/src/services/self_fix_pr_worker.rs`

```rust
//! Self-fix PR-bridge worker. Dequeues `self_fix_pr` jobs (enqueued in-tx by
//! `complete_task`) and drives `SelfFixService::open_pr`. Mirrors the
//! dequeue/poll/shutdown shape of `ProjectCloneWorker`.

use std::sync::Arc;

use agentforge_core::{AppResult, OrgId, SELF_FIX_PR_QUEUE, SelfFixPrJob, TenantScope, UserId};
use sqlx::PgPool;
use tokio::sync::watch;
use uuid::Uuid;

use crate::services::self_fix::SelfFixService;

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

pub struct SelfFixPrWorker {
    pool: PgPool,
    service: Arc<SelfFixService>,
    worker_id: String,
}

impl SelfFixPrWorker {
    pub fn new(pool: PgPool, service: Arc<SelfFixService>) -> Self {
        Self { pool, service, worker_id: format!("self-fix-pr-{}", Uuid::now_v7()) }
    }

    /// Dequeue loop until shutdown. pg_notify is wake-only; poll on the interval.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        tracing::info!(worker_id = %self.worker_id, "self_fix_pr worker starting");
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!(worker_id = %self.worker_id, "self_fix_pr worker shutting down");
                        return;
                    }
                }
                result = self.dequeue_and_process() => match result {
                    Ok(true) => {}
                    Ok(false) => tokio::time::sleep(POLL_INTERVAL).await,
                    Err(err) => {
                        tracing::warn!(error = %err, "self_fix_pr worker tick failed");
                        tokio::time::sleep(POLL_INTERVAL).await;
                    }
                },
            }
        }
    }

    /// Process at most one job. Returns `Ok(true)` if a job was claimed.
    pub async fn dequeue_and_process(&self) -> AppResult<bool> {
        let job = agentforge_jobs::queue::dequeue(&self.pool, SELF_FIX_PR_QUEUE, &self.worker_id)
            .await
            .map_err(|e| agentforge_core::AppError::from(anyhow::Error::from(e)))?;
        let Some(job) = job else {
            return Ok(false);
        };

        let payload: SelfFixPrJob = match serde_json::from_value(job.payload.clone()) {
            Ok(p) => p,
            Err(err) => {
                tracing::error!(job_id = %job.id, error = %err, "self_fix_pr payload undecodable; dropping");
                agentforge_jobs::queue::complete(&self.pool, job.id)
                    .await
                    .map_err(|e| agentforge_core::AppError::from(anyhow::Error::from(e)))?;
                return Ok(true);
            }
        };

        // Background workers act on behalf of the job's org; the user axis is
        // unused by `open_pr` (org-scoped), so use a nil placeholder
        // (precedent: project_clone_worker.rs:1372).
        let scope = TenantScope::new(OrgId::from(payload.org_id), UserId::from(Uuid::nil()));

        match self.service.open_pr(&scope, payload.task_id).await {
            Ok(outcome) => {
                metrics::counter!("agentforge_self_fix_pr_total", "outcome" => "opened").increment(1);
                tracing::info!(task_id = %payload.task_id, pr = outcome.pr_number, "self-fix PR opened");
                agentforge_jobs::queue::complete(&self.pool, job.id)
                    .await
                    .map_err(|e| agentforge_core::AppError::from(anyhow::Error::from(e)))?;
            }
            Err(err) => {
                metrics::counter!("agentforge_self_fix_pr_total", "outcome" => "failed").increment(1);
                tracing::warn!(task_id = %payload.task_id, error = %err, "self-fix PR open failed");
                agentforge_jobs::queue::fail(&self.pool, job.id, &err.to_string())
                    .await
                    .map_err(|e| agentforge_core::AppError::from(anyhow::Error::from(e)))?;
            }
        }
        Ok(true)
    }
}

/// Describe metric series so they are present from the first scrape.
pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_self_fix_pr_total",
        "Self-fix PR-bridge outcomes, labeled opened|failed"
    );
    metrics::counter!("agentforge_self_fix_pr_total", "outcome" => "opened").increment(0);
    metrics::counter!("agentforge_self_fix_pr_total", "outcome" => "failed").increment(0);
}
```

- [ ] **Step 4: Register the module + drop dead-code allows**

In `rust/crates/api/src/services/mod.rs` add `pub mod self_fix_pr_worker;`. In `rust/crates/api/src/services/self_fix/mod.rs` delete the three `#[allow(dead_code)]` attributes on `SelfFixService` (line 52), `new` (line 63), and `open_pr` (line 84). Leave the allows on `approve_and_merge`/`review_snapshot`/`pr_body`/`merge_audit_body` (still unused until later milestones).

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd rust && cargo test -p agentforge-api worker_fails_job_when_github_unconfigured`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add rust/crates/api/src/services/self_fix_pr_worker.rs rust/crates/api/src/services/mod.rs rust/crates/api/src/services/self_fix/mod.rs rust/crates/api/tests/
git commit -m "feat(self-fix): add SelfFixPrWorker that drives open_pr from the queue"
```

---

### Task 6: `AppState::self_fix_pr_worker()` builder

**Files:**

- Modify: `rust/crates/api/src/state_services.rs` (near `self_fix_service` at line ~347)

**Interfaces:**

- Consumes: existing `self_fix_service()` (pub(crate)).
- Produces: `pub fn AppState::self_fix_pr_worker(&self) -> crate::services::self_fix_pr_worker::SelfFixPrWorker` (callable from the `rust/bins/server` binary).

- [ ] **Step 1: Add the builder** (a `pub` method so `main.rs` in the separate binary crate can call it)

```rust
    /// Build the self-fix PR worker, reusing the same dependency wiring as
    /// `self_fix_service`. `pub` so the server binary can spawn it.
    pub fn self_fix_pr_worker(&self) -> crate::services::self_fix_pr_worker::SelfFixPrWorker {
        crate::services::self_fix_pr_worker::SelfFixPrWorker::new(
            self.pool.clone(),
            std::sync::Arc::new(self.self_fix_service()),
        )
    }
```

- [ ] **Step 2: Verify the crate builds**

Run: `cd rust && cargo build -p agentforge-api`
Expected: compiles. (If `self_fix_service` still carries `#[allow(dead_code)]`, leave it — it is now reachable through this `pub` method, so also remove that allow and the one on `github_app_client` if clippy flags them as now-used.)

- [ ] **Step 3: Commit**

```bash
git add rust/crates/api/src/state_services.rs
git commit -m "feat(api): add AppState::self_fix_pr_worker builder"
```

---

### Task 7: Spawn the worker + register metrics in the server binary

**Files:**

- Modify: `rust/bins/server/src/main.rs:128-132` (metrics) and after the `project_clone_worker` spawn block (~578); the worker handle joins the existing shutdown set.

**Interfaces:**

- Consumes: `config.self_fix_pr_worker_enabled` (Task 3), `state.self_fix_pr_worker()` (Task 6), `self_fix_pr_worker::register_metrics` (Task 5), the existing `shutdown_rx`.

- [ ] **Step 1: Register metrics** — add to the `register_metrics` block at `main.rs:132`

```rust
    agentforge_api::services::project_clone_worker::register_metrics();
    agentforge_api::services::self_fix_pr_worker::register_metrics();
```

- [ ] **Step 2: Spawn the worker** — add immediately after the `project_clone_worker_handle` block (`main.rs` ~578), where `state` and `shutdown_rx` are in scope

```rust
    // Self-fix PR worker: dequeues `self_fix_pr` jobs enqueued in-tx by
    // `complete_task` and drives the PR bridge. Gated by config; the bridge
    // itself no-ops with a visible error when the GitHub App is unconfigured.
    let self_fix_pr_worker_handle = if config.self_fix_pr_worker_enabled {
        let worker = state.self_fix_pr_worker();
        let worker_shutdown = shutdown_rx.clone();
        Some(tokio::spawn(async move { worker.run(worker_shutdown).await }))
    } else {
        tracing::info!("self_fix_pr worker disabled (flag off)");
        None
    };
```

- [ ] **Step 3: Join on shutdown** — wherever `project_clone_worker_handle` is awaited/aborted during graceful shutdown, do the same for `self_fix_pr_worker_handle` (mirror the existing `if let Some(handle) = ... { handle.abort()/await }` pattern).

- [ ] **Step 4: Build the binary**

Run: `cd rust && cargo build -p agentforge-server`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add rust/bins/server/src/main.rs
git commit -m "feat(server): spawn self-fix PR worker + register its metrics"
```

---

### Task 8: Full validation + docs

- [ ] **Step 1: Run the full Rust CI gate**

Run: `cd rust && make ci`
Expected: PASS (clippy, fmt, full `cargo test --workspace`). This change touches API contracts and orchestration, so the full gate is required per CLAUDE.md.

- [ ] **Step 2: Document the flag** — add `SELF_FIX_PR_WORKER_ENABLED` to `docs/guides/configuration.md` (next to `PROJECT_CLONE_WORKER_ENABLED`) and note the new event-driven trigger in the self-fix section of `docs/plans/self-iteration-roadmap.md`.

- [ ] **Step 3: Commit**

```bash
git add docs/guides/configuration.md docs/plans/self-iteration-roadmap.md
git commit -m "docs: document self_fix_pr_worker_enabled + event-driven trigger"
```

---

## Self-Review

1. **Spec coverage:** Proposal 1.1 asks for (a) in-tx enqueue on completion — Task 4; (b) a worker modeled on project_clone — Task 5; (c) gated spawn — Tasks 6-7. The proposal's "use `queue::enqueue`" was corrected: `enqueue` is pool-only, so Task 1 adds `enqueue_in_tx` for the true transactional guarantee. Direct in-tx `job_queue` insert is used instead of the clone-style outbox+relay (simpler, no relay infra; the job row is itself durable).
2. **Placeholder scan:** The two test helpers (`seed_self_fix_task`, `build_self_fix_pr_worker_for_test`) are described with exact behavior and reuse instructions rather than inlined, because their bodies depend on the nearest existing orchestration `#[sqlx::test]` seeding helper — confirm that helper before writing them.
3. **Type consistency:** `SelfFixPrJob { task_id, org_id }`, `SELF_FIX_PR_QUEUE = "self_fix_pr"`, `enqueue_in_tx(tx, queue, payload, priority, run_at, unique_key, max_attempts)`, and `SelfFixPrWorker::new(pool, Arc<SelfFixService>)` are used identically across producer (Task 4) and consumer (Task 5).
