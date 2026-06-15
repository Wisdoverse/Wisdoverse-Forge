//! Project clone-attempt repository (M5) — the worker + reconciler's data layer.
//!
//! Owns every read/write of `project_clone_attempts` (the source of truth) plus
//! the denormalized `projects.clone_status` summary. The M5 worker drives the
//! attempt state machine through these methods; the reconciler uses the
//! lease/queue-recovery scans.
//!
//! Tenant scope: a clone attempt is identified by its `(project_id, attempt)`
//! and always carries its own `organization_id`/`workspace_id` snapshot (written
//! at create time in M2). The worker re-reads the authoritative row by
//! `(project_id, attempt)` rather than trusting the job payload, and every
//! mutation is constrained by `id`/`project_id` so it can only ever touch the
//! attempt it loaded — there is no cross-org write surface here.

use agentforge_core::{AppResult, ProjectId};
use agentforge_db::entities::ProjectCloneAttempt;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::project_clone::{CloneAttemptStatus, CloneErrorClass, CloneStatus};

/// The successful-finalize payload written onto a `ready` attempt + its project.
#[derive(Debug, Clone)]
pub struct CloneSuccess {
    pub resolved_branch: Option<String>,
    pub head_sha: String,
    pub bytes_cloned: i64,
    pub duration_ms: i64,
}

/// The failure-finalize payload written onto a `failed` attempt + its project.
/// `error_message` is ALREADY redacted by the worker (the persistence boundary
/// only ever receives a scrubbed string — see the M5 worker).
#[derive(Debug, Clone)]
pub struct CloneFailure {
    pub error_class: String,
    /// The REDACTED error message. The repository never sees the raw stderr.
    pub error_message: String,
    pub duration_ms: Option<i64>,
}

/// A reconciler candidate: an attempt that may need recovery (stuck `cloning`
/// past its lease, or `queued` with no live job). Carries just the identity +
/// retry context the reconciler needs.
#[derive(Debug, Clone)]
pub struct ReconcileCandidate {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub attempt: i32,
    pub repository_url: String,
    pub provider: Option<String>,
    pub container_id: Option<String>,
    /// Set once the cloned tree was renamed live under the projects root, before
    /// the DB finalize-to-`ready` committed. A non-NULL value means the recovery
    /// owner must FORCE `ready` (the on-disk publish is irreversible) instead of
    /// failing/re-cloning into an already-correct target.
    pub materialized_at: Option<DateTime<Utc>>,
}

/// Outcome of the locked publish step ([`ProjectCloneRepository::publish_ready_locked`]).
///
/// The project row is locked `FOR UPDATE` for the whole rename so a concurrent
/// soft-delete (M6 delete path) cannot slip between the live-check and the rename
/// and strand an orphan directory in the projects root.
#[derive(Debug)]
pub enum PublishOutcome {
    /// The project was live, the dir name still matched, the rename succeeded,
    /// `materialized_at` was stamped, and the attempt + project were finalized
    /// `ready` — all in one transaction. Carries whether the `status='cloning'`
    /// finalize predicate matched (`false` ⇒ the row was no longer `cloning`, e.g.
    /// a reconciler already failed it; the rename is the source of truth and the
    /// caller forces `ready`).
    Published { finalized: bool },
    /// The project was soft-deleted OR its `workspace_dir_name` no longer matches
    /// the attempt's target. NO rename happened; the attempt must be `cancelled`.
    ProjectGone,
    /// The rename itself failed (cross-device, missing source, race). NO partial
    /// target is left (rename is all-or-nothing). The caller fails the attempt.
    RenameFailed(std::io::Error),
}

/// Database access layer for project clone attempts.
#[derive(Clone)]
pub struct ProjectCloneRepository {
    pool: PgPool,
}

impl ProjectCloneRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Load the authoritative attempt row by `(project_id, attempt)`. The worker
    /// re-reads this rather than trusting the job payload snapshot. `None` when
    /// the attempt no longer exists (e.g. the project was hard-deleted).
    pub async fn find_attempt(&self, project_id: Uuid, attempt: i32) -> AppResult<Option<ProjectCloneAttempt>> {
        let row = sqlx::query_as::<_, ProjectCloneAttempt>(
            r#"SELECT * FROM project_clone_attempts WHERE project_id = $1 AND attempt = $2"#,
        )
        .bind(project_id)
        .bind(attempt)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Atomically claim a `queued` attempt for this worker: transition it to
    /// `cloning`, stamp `worker_id`/`job_id`/`started_at`/`lease_expires_at`, and
    /// mirror `projects.clone_status='cloning'`. Returns the updated row, or
    /// `None` when the row was NOT in `queued` (already claimed by a racing
    /// worker, or already terminal) — the durable, exactly-once-per-attempt guard.
    ///
    /// The `status='queued'` predicate in the UPDATE is the claim: only one worker
    /// can flip a given attempt out of `queued`, so a re-relayed/duplicate job for
    /// the same attempt finds the row already `cloning`/terminal and no-ops.
    pub async fn claim_for_cloning(
        &self,
        project_id: Uuid,
        attempt: i32,
        worker_id: &str,
        job_id: Option<Uuid>,
        lease_expires_at: DateTime<Utc>,
    ) -> AppResult<Option<ProjectCloneAttempt>> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as::<_, ProjectCloneAttempt>(
            r#"UPDATE project_clone_attempts
                  SET status = $3,
                      worker_id = $4,
                      job_id = $5,
                      lease_expires_at = $6,
                      started_at = COALESCE(started_at, now()),
                      updated_at = now()
                WHERE project_id = $1 AND attempt = $2 AND status = $7
                RETURNING *"#,
        )
        .bind(project_id)
        .bind(attempt)
        .bind(CloneAttemptStatus::Cloning.as_str())
        .bind(worker_id)
        .bind(job_id)
        .bind(lease_expires_at)
        .bind(CloneAttemptStatus::Queued.as_str())
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(ref attempt_row) = row {
            Self::set_project_clone_status_tx(&mut tx, attempt_row.project_id.as_uuid(), CloneStatus::Cloning).await?;
        }
        tx.commit().await?;
        Ok(row)
    }

    /// Record the chosen credential id on the attempt (never the secret). Called
    /// right before launching the container so the attempt faithfully records
    /// which `git_credentials` row was used. A no-op when `credential_id` is None.
    pub async fn set_credential_id(&self, attempt_id: Uuid, credential_id: Option<Uuid>) -> AppResult<()> {
        let Some(credential_id) = credential_id else {
            return Ok(());
        };
        sqlx::query(r#"UPDATE project_clone_attempts SET credential_id = $2, updated_at = now() WHERE id = $1"#)
            .bind(attempt_id)
            .bind(credential_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Record the deterministic clone container name on the attempt
    /// (`agentforge-clone-<attempt_id>`) right before the wait, so a crashed-worker
    /// recovery + a human operator both have the exact container to inspect/reap
    /// without re-deriving it. Best-effort observability (no transition).
    pub async fn set_container_id(&self, attempt_id: Uuid, container_id: &str) -> AppResult<()> {
        sqlx::query(r#"UPDATE project_clone_attempts SET container_id = $2, updated_at = now() WHERE id = $1"#)
            .bind(attempt_id)
            .bind(container_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Publish a finished clone under the project lock, closing the delete-race.
    ///
    /// In ONE transaction: lock the project row `FOR UPDATE`; if it is soft-deleted
    /// OR its `workspace_dir_name` no longer matches `expected_dir_name`, return
    /// [`PublishOutcome::ProjectGone`] WITHOUT renaming (so no orphan dir lands in
    /// the projects root for a dead/renamed project). Otherwise invoke `rename`
    /// (the atomic same-fs `staging/repo` → target rename) while the lock is held;
    /// on success stamp `materialized_at` + finalize the attempt + project `ready`
    /// and commit; on rename error roll back and return [`PublishOutcome::RenameFailed`].
    ///
    /// The lock is held across the single local rename syscall so a concurrent M6
    /// soft-delete (which must take the same `FOR UPDATE` lock and cancel in-flight
    /// attempts) serializes against us: either it wins and we observe `ProjectGone`,
    /// or we win and it sees a `ready` clone it can clean up on delete.
    ///
    /// `rename` is a synchronous `FnOnce` returning `io::Result` so the filesystem
    /// policy stays in the caller (the worker) and the repo owns only the SQL +
    /// transaction boundary. It is called at most once.
    ///
    /// RESIDUAL (be honest): a filesystem rename cannot be made atomic with a
    /// Postgres commit. If the process crashes AFTER `rename()` succeeds but BEFORE
    /// the tx commits, the clone is live on disk yet `materialized_at` stays NULL
    /// and the attempt stays `cloning`. That residual is closed OUTSIDE this method
    /// by the reconciler's expired-lease recovery, which checks the filesystem
    /// (`recover_if_target_published`): a recovered `cloning` attempt whose target
    /// dir already exists is ADOPTED (force-ready) rather than failed/retried into
    /// the overwrite guard. So the publish is recoverable across BOTH a
    /// committed-marker crash (materialized_at set ⇒ the materialized-unfinalized
    /// scan heals it) and an uncommitted-marker crash (dir on disk ⇒ the
    /// filesystem recovery heals it).
    pub async fn publish_ready_locked(
        &self,
        attempt_id: Uuid,
        project_id: Uuid,
        expected_dir_name: &str,
        success: &CloneSuccess,
        rename: impl FnOnce() -> std::io::Result<()>,
    ) -> AppResult<PublishOutcome> {
        let mut tx = self.pool.begin().await?;

        // Lock the project row + read its current live dir name atomically. A
        // soft-deleted project returns no row (the `deleted_at IS NULL` filter).
        let live_dir: Option<String> = sqlx::query_scalar(
            r#"SELECT workspace_dir_name FROM projects
                WHERE id = $1 AND deleted_at IS NULL
                FOR UPDATE"#,
        )
        .bind(project_id)
        .fetch_optional(&mut *tx)
        .await?;

        // Gone (soft-deleted) or its directory name changed under us ⇒ do NOT
        // publish; the caller cancels the attempt + removes staging.
        if live_dir.as_deref() != Some(expected_dir_name) {
            tx.rollback().await?;
            return Ok(PublishOutcome::ProjectGone);
        }

        // Perform the atomic rename WHILE holding the project lock. On failure roll
        // back (nothing was stamped) and surface the io error.
        if let Err(err) = rename() {
            tx.rollback().await?;
            return Ok(PublishOutcome::RenameFailed(err));
        }

        // The clone is now live on disk: stamp materialized_at + finalize ready in
        // the SAME tx. `materialized_at` is set so a crash before commit is still
        // recoverable to `ready` (the rename is irreversible).
        let finalized = self.finalize_ready_tx(&mut tx, attempt_id, success, false).await?;
        // Ensure materialized_at is set even if the finalize predicate did not
        // match (e.g. a reconciler already moved the row off `cloning`): the rename
        // happened, so the disk is the source of truth.
        if !finalized {
            sqlx::query(
                r#"UPDATE project_clone_attempts
                      SET materialized_at = COALESCE(materialized_at, now()), updated_at = now()
                    WHERE id = $1"#,
            )
            .bind(attempt_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(PublishOutcome::Published { finalized })
    }

    /// Transition an attempt to `cancelled` (the delete-race outcome): the project
    /// was soft-deleted/renamed mid-flight, so the clone must not publish. Mirrors
    /// the project summary to `none` (a cancelled attempt's project shows no active
    /// clone) ONLY when the project still exists; a hard-deleted project simply has
    /// no row to mirror. Accepts the transition from any non-terminal status.
    pub async fn cancel_attempt(&self, attempt_id: Uuid, reason: &str) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as::<_, ProjectCloneAttempt>(
            r#"UPDATE project_clone_attempts
                  SET status = $2,
                      error_class = $3,
                      error_message = $4,
                      lease_expires_at = NULL,
                      finished_at = now(),
                      updated_at = now()
                WHERE id = $1 AND status IN ($5, $6)
                RETURNING *"#,
        )
        .bind(attempt_id)
        .bind(CloneAttemptStatus::Cancelled.as_str())
        .bind(CloneErrorClass::Internal.as_str())
        .bind(reason)
        .bind(CloneAttemptStatus::Cloning.as_str())
        .bind(CloneAttemptStatus::Queued.as_str())
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(ref attempt_row) = row {
            // A cancelled latest attempt collapses the project summary to `none`.
            Self::set_project_clone_status_tx(&mut tx, attempt_row.project_id.as_uuid(), CloneStatus::None).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Finalize an attempt as `ready`: persist branch/head_sha/bytes/duration and
    /// mirror `projects.clone_status='ready'`. The `status='cloning'` predicate
    /// keeps the transition legal (only an in-flight clone may become ready).
    ///
    /// Returns whether the `status='cloning'` predicate matched a row. A `false`
    /// AFTER the publish rename has already materialized the clone on disk is NOT
    /// a benign no-op: the data is correct + irreversible, so the caller must FORCE
    /// `ready` via [`force_ready`](Self::force_ready) (the rename is the source of
    /// truth) rather than leave the attempt desynced from the live directory.
    pub async fn finalize_ready(&self, attempt_id: Uuid, success: &CloneSuccess) -> AppResult<bool> {
        let mut tx = self.pool.begin().await?;
        let matched = self.finalize_ready_tx(&mut tx, attempt_id, success, false).await?;
        tx.commit().await?;
        Ok(matched)
    }

    /// FORCE an attempt to `ready` regardless of its current status, used ONLY for
    /// recovery once the clone is irreversibly materialized on disk
    /// (`materialized_at IS NOT NULL`). The `materialized_at IS NOT NULL` predicate
    /// is the safety gate: a row that was never published can never be forced, so
    /// this can only ever promote a clone whose bytes are already live under the
    /// projects root. Idempotent: a row already `ready` re-stamps the same values.
    pub async fn force_ready(&self, attempt_id: Uuid, success: &CloneSuccess) -> AppResult<bool> {
        let mut tx = self.pool.begin().await?;
        let matched = self.finalize_ready_tx(&mut tx, attempt_id, success, true).await?;
        tx.commit().await?;
        Ok(matched)
    }

    /// ADOPT an on-disk-published clone whose `materialized_at` was never committed
    /// (the crash-after-rename-before-commit window, #1): the CALLER has proven the
    /// clone is live on disk (it checked the filesystem), so this stamps
    /// `materialized_at` AND finalizes `ready` in one UPDATE — without the
    /// `materialized_at IS NOT NULL` gate that [`force_ready`] requires (which would
    /// reject this case precisely because the marker was lost). Gated on the attempt
    /// NOT already being terminal-non-ready (`cancelled`/`failed` stays as-is unless
    /// it is the in-flight `cloning`/`queued` the recovery owns), so it can never
    /// resurrect a deliberately-cancelled attempt. Idempotent on a `ready` row.
    pub async fn adopt_published_ready(&self, attempt_id: Uuid, success: &CloneSuccess) -> AppResult<bool> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as::<_, ProjectCloneAttempt>(
            r#"UPDATE project_clone_attempts
                  SET status = $2,
                      resolved_branch = $3,
                      head_sha = $4,
                      bytes_cloned = $5,
                      duration_ms = $6,
                      lease_expires_at = NULL,
                      materialized_at = COALESCE(materialized_at, now()),
                      finished_at = now(),
                      updated_at = now()
                WHERE id = $1 AND status IN ($7, $8, $9)
                RETURNING *"#,
        )
        .bind(attempt_id)
        .bind(CloneAttemptStatus::Ready.as_str())
        .bind(success.resolved_branch.as_deref())
        .bind(&success.head_sha)
        .bind(success.bytes_cloned)
        .bind(success.duration_ms)
        .bind(CloneAttemptStatus::Cloning.as_str())
        .bind(CloneAttemptStatus::Queued.as_str())
        .bind(CloneAttemptStatus::Ready.as_str())
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(ref attempt_row) = row {
            Self::set_project_clone_status_tx(&mut tx, attempt_row.project_id.as_uuid(), CloneStatus::Ready).await?;
        }
        tx.commit().await?;
        Ok(row.is_some())
    }

    /// Shared `ready` finalize body. When `force` is false the transition is gated
    /// on `status='cloning'`; when true it is gated on `materialized_at IS NOT
    /// NULL` (the on-disk publish proof) so a recovery can promote a materialized
    /// clone from any non-ready status.
    async fn finalize_ready_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        attempt_id: Uuid,
        success: &CloneSuccess,
        force: bool,
    ) -> AppResult<bool> {
        let predicate = if force {
            // Recovery: only ever promote a clone whose bytes are already live on
            // disk. `materialized_at IS NOT NULL` is the irreversibility proof.
            "id = $1 AND materialized_at IS NOT NULL"
        } else {
            "id = $1 AND status = $7"
        };
        let sql = format!(
            r#"UPDATE project_clone_attempts
                  SET status = $2,
                      resolved_branch = $3,
                      head_sha = $4,
                      bytes_cloned = $5,
                      duration_ms = $6,
                      lease_expires_at = NULL,
                      materialized_at = COALESCE(materialized_at, now()),
                      finished_at = now(),
                      updated_at = now()
                WHERE {predicate}
                RETURNING *"#
        );
        let row = sqlx::query_as::<_, ProjectCloneAttempt>(&sql)
            .bind(attempt_id)
            .bind(CloneAttemptStatus::Ready.as_str())
            .bind(success.resolved_branch.as_deref())
            .bind(&success.head_sha)
            .bind(success.bytes_cloned)
            .bind(success.duration_ms)
            .bind(CloneAttemptStatus::Cloning.as_str())
            .fetch_optional(&mut **tx)
            .await?;

        if let Some(ref attempt_row) = row {
            Self::set_project_clone_status_tx(tx, attempt_row.project_id.as_uuid(), CloneStatus::Ready).await?;
        }
        Ok(row.is_some())
    }

    /// Finalize an attempt as `failed`: persist the (already-redacted) error +
    /// class + duration and mirror `projects.clone_status='failed'`. Accepts the
    /// transition from EITHER `cloning` (the normal failure path) OR `queued`
    /// (a reconciler giving up on a lost-enqueue attempt) so the reconciler can
    /// terminate a stuck `queued` row without a phantom `cloning` hop.
    pub async fn finalize_failed(&self, attempt_id: Uuid, failure: &CloneFailure) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as::<_, ProjectCloneAttempt>(
            r#"UPDATE project_clone_attempts
                  SET status = $2,
                      error_class = $3,
                      error_message = $4,
                      duration_ms = COALESCE($5, duration_ms),
                      lease_expires_at = NULL,
                      finished_at = now(),
                      updated_at = now()
                WHERE id = $1 AND status IN ($6, $7)
                RETURNING *"#,
        )
        .bind(attempt_id)
        .bind(CloneAttemptStatus::Failed.as_str())
        .bind(&failure.error_class)
        .bind(&failure.error_message)
        .bind(failure.duration_ms)
        .bind(CloneAttemptStatus::Cloning.as_str())
        .bind(CloneAttemptStatus::Queued.as_str())
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(ref attempt_row) = row {
            Self::set_project_clone_status_tx(&mut tx, attempt_row.project_id.as_uuid(), CloneStatus::Failed).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Schedule a bounded retry: in ONE transaction insert a NEW attempt row
    /// (`attempt+1`, `queued`, same URL/provider/tenant snapshot) AND a
    /// `project_clone` transactional-outbox row so the existing outbox publisher
    /// re-enqueues it. Mirrors `projects.clone_status='queued'` so the UI shows
    /// the project is retrying.
    ///
    /// `run_after` is the computed backoff deadline (from the attempt number): it
    /// is carried in the outbox payload and the relay maps it onto the relayed
    /// `job_queue.run_at`, so the retry is genuinely DELAYED rather than re-running
    /// instantly (no fast-fail retry storm). `None` ⇒ enqueue immediately.
    ///
    /// The outbox INSERT carries the SAME `WHERE NOT EXISTS (… published_at IS NULL
    /// AND payload->>'attempt' = …)` dedup guard as [`reenqueue_outbox`], so a
    /// retry racing a reconciler re-enqueue for the same attempt cannot pile up two
    /// unpublished outbox rows.
    ///
    /// Returns the new attempt number. Idempotent against the
    /// `uq_project_clone_attempt(project_id, attempt)` index: a duplicate retry
    /// insert (same project_id+next_attempt) is a no-op and returns `None`.
    pub async fn schedule_retry(
        &self,
        organization_id: Uuid,
        workspace_id: Uuid,
        project_id: Uuid,
        next_attempt: i32,
        repository_url: &str,
        provider: Option<&str>,
        run_after: Option<DateTime<Utc>>,
    ) -> AppResult<Option<i32>> {
        let mut tx = self.pool.begin().await?;

        // Insert the next attempt row. ON CONFLICT on the (project_id, attempt)
        // unique index makes a duplicate retry a no-op (returns no row).
        let inserted: Option<(i32,)> = sqlx::query_as(
            r#"INSERT INTO project_clone_attempts
                   (organization_id, workspace_id, project_id, attempt, repository_url, provider, status)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               ON CONFLICT (project_id, attempt) DO NOTHING
               RETURNING attempt"#,
        )
        .bind(organization_id)
        .bind(workspace_id)
        .bind(project_id)
        .bind(next_attempt)
        .bind(repository_url)
        .bind(provider)
        .bind(CloneAttemptStatus::Queued.as_str())
        .fetch_optional(&mut *tx)
        .await?;

        let Some((attempt_num,)) = inserted else {
            // A concurrent retry already created this attempt — leave it to that
            // path's outbox row; do not write a duplicate outbox row.
            tx.commit().await?;
            return Ok(None);
        };

        // Transactional-outbox row so the publisher relays the retry into the
        // job_queue (same contract as the M2 create path). The backoff rides in the
        // payload's `run_after`; the relay honors it as `job_queue.run_at`.
        let payload = crate::domain::project_clone::CloneOutboxPayload { project_id, attempt: attempt_num, run_after };
        let payload_json =
            serde_json::to_value(&payload).map_err(|e| agentforge_core::AppError::from(anyhow::Error::from(e)))?;
        // SAME dedup guard as reenqueue_outbox: at most one unpublished outbox row
        // per (project_id, attempt), so a retry racing a reconciler re-enqueue does
        // not double-publish.
        sqlx::query(
            r#"INSERT INTO orchestration_outbox
                   (id, organization_id, aggregate_type, aggregate_id, event_type, payload)
               SELECT gen_random_uuid(), $1, $2, $3, $4, $5
               WHERE NOT EXISTS (
                   SELECT 1 FROM orchestration_outbox o
                    WHERE o.aggregate_type = $2
                      AND o.aggregate_id = $3
                      AND o.published_at IS NULL
                      AND o.payload->>'attempt' = $6
               )"#,
        )
        .bind(organization_id)
        .bind(crate::domain::project_clone::CLONE_OUTBOX_AGGREGATE_TYPE)
        .bind(project_id)
        .bind(crate::domain::project_clone::CLONE_OUTBOX_EVENT_TYPE)
        .bind(payload_json)
        .bind(attempt_num.to_string())
        .execute(&mut *tx)
        .await?;

        // Mirror the project summary back to `queued` so the UI shows a retry.
        Self::set_project_clone_status_tx(&mut tx, project_id, CloneStatus::Queued).await?;
        tx.commit().await?;
        Ok(Some(attempt_num))
    }

    /// Extend the lease on a `cloning` attempt this worker still owns (the lease
    /// HEARTBEAT). Bounded by `status='cloning' AND worker_id=$worker`, so it only
    /// ever extends a lease we hold — never one the reconciler already claimed for
    /// recovery (which rewrites `worker_id`). Returns whether a row was extended;
    /// `false` means we lost ownership (reconciler reaped us) and the heartbeat
    /// task should stop. Keeps a healthy long clone from being recovered out from
    /// under a live worker.
    pub async fn extend_lease(
        &self,
        attempt_id: Uuid,
        worker_id: &str,
        lease_expires_at: DateTime<Utc>,
    ) -> AppResult<bool> {
        let result = sqlx::query(
            r#"UPDATE project_clone_attempts
                  SET lease_expires_at = $3, updated_at = now()
                WHERE id = $1 AND worker_id = $2 AND status = $4"#,
        )
        .bind(attempt_id)
        .bind(worker_id)
        .bind(lease_expires_at)
        .bind(CloneAttemptStatus::Cloning.as_str())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Reconciler scan (a): ATOMICALLY claim `cloning` attempts whose lease has
    /// expired (the worker likely crashed) for recovery by THIS reconciler.
    ///
    /// The claim is the `UPDATE … WHERE status='cloning' AND lease_expires_at <
    /// now() RETURNING *` itself: it rewrites `worker_id` to the recoverer and
    /// pushes `lease_expires_at` out by `recovery_grace`, so a SECOND concurrent
    /// reconciler's identical predicate no longer matches these rows (their lease
    /// is now in the future) — exactly one recoverer owns each row. A slow-but-LIVE
    /// worker whose lease lapsed is protected by its [`extend_lease`] heartbeat,
    /// which keeps `lease_expires_at` ahead of `now()` so the row is never claimed
    /// here; if it nonetheless loses the race, its own heartbeat/finalize now finds
    /// `worker_id` rewritten and stops. Only the returned rows are recovered.
    pub async fn claim_expired_cloning_for_recovery(
        &self,
        recoverer_id: &str,
        now: DateTime<Utc>,
        recovery_grace: DateTime<Utc>,
        limit: i64,
    ) -> AppResult<Vec<ReconcileCandidate>> {
        let rows = sqlx::query_as::<_, ProjectCloneAttempt>(
            r#"UPDATE project_clone_attempts
                  SET worker_id = $1,
                      lease_expires_at = $4,
                      updated_at = now()
                WHERE id IN (
                    SELECT id FROM project_clone_attempts
                     WHERE status = $2
                       AND lease_expires_at IS NOT NULL
                       AND lease_expires_at < $3
                     ORDER BY lease_expires_at ASC
                     FOR UPDATE SKIP LOCKED
                     LIMIT $5
                )
                RETURNING *"#,
        )
        .bind(recoverer_id)
        .bind(CloneAttemptStatus::Cloning.as_str())
        .bind(now)
        .bind(recovery_grace)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(reconcile_candidate).collect())
    }

    /// Reconciler scan (c): attempts whose clone is irreversibly materialized on
    /// disk (`materialized_at IS NOT NULL`) but never reached `ready` — the
    /// rename-then-DB-finalize split-brain (#1). The recoverer FORCES `ready`
    /// (the on-disk clone is the source of truth) rather than re-cloning into a
    /// target the overwrite guard would refuse forever. Bounded by `limit`.
    pub async fn find_materialized_unfinalized(&self, limit: i64) -> AppResult<Vec<ReconcileCandidate>> {
        let rows = sqlx::query_as::<_, ProjectCloneAttempt>(
            r#"SELECT * FROM project_clone_attempts
                WHERE materialized_at IS NOT NULL
                  AND status <> $1
                ORDER BY materialized_at ASC
                LIMIT $2"#,
        )
        .bind(CloneAttemptStatus::Ready.as_str())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(reconcile_candidate).collect())
    }

    /// Load the success-finalize payload (branch/sha/bytes) the worker persisted on
    /// an attempt — used by the reconciler's force-ready recovery, which has no
    /// in-memory `CloneSuccess` because the worker that ran the clone is gone.
    /// Returns the stored values (defaulting missing optionals) for re-finalize.
    pub async fn load_clone_success(&self, attempt_id: Uuid) -> AppResult<Option<CloneSuccess>> {
        let row: Option<StoredCloneSuccessRow> = sqlx::query_as(
            r#"SELECT resolved_branch, head_sha, bytes_cloned, duration_ms
                 FROM project_clone_attempts WHERE id = $1"#,
        )
        .bind(attempt_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(resolved_branch, head_sha, bytes_cloned, duration_ms)| CloneSuccess {
            resolved_branch,
            head_sha: head_sha.unwrap_or_default(),
            bytes_cloned: bytes_cloned.unwrap_or_default(),
            duration_ms: duration_ms.unwrap_or_default(),
        }))
    }

    /// Reconciler scan (b): attempts in `queued` whose enqueue appears lost — no
    /// live `job_queue` row carries their `project_clone:<project_id>:<attempt>`
    /// unique key AND no unpublished outbox row is pending for them. These are
    /// the "committed attempt but the outbox/job vanished" rows the reconciler
    /// re-enqueues. Only rows older than `older_than` are considered, so a row
    /// that was JUST created (whose outbox publisher simply has not run yet) is
    /// not prematurely double-enqueued.
    pub async fn find_orphaned_queued(
        &self,
        older_than: DateTime<Utc>,
        limit: i64,
    ) -> AppResult<Vec<ReconcileCandidate>> {
        let rows = sqlx::query_as::<_, ProjectCloneAttempt>(
            r#"SELECT a.* FROM project_clone_attempts a
                WHERE a.status = $1
                  AND a.created_at < $2
                  AND NOT EXISTS (
                      SELECT 1 FROM job_queue j
                       WHERE j.unique_key = 'project_clone:' || a.project_id::text || ':' || a.attempt::text
                  )
                  AND NOT EXISTS (
                      SELECT 1 FROM orchestration_outbox o
                       WHERE o.aggregate_type = $3
                         AND o.aggregate_id = a.project_id
                         AND o.published_at IS NULL
                         AND o.payload->>'attempt' = a.attempt::text
                  )
                ORDER BY a.created_at ASC
                LIMIT $4"#,
        )
        .bind(CloneAttemptStatus::Queued.as_str())
        .bind(older_than)
        .bind(crate::domain::project_clone::CLONE_OUTBOX_AGGREGATE_TYPE)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(reconcile_candidate).collect())
    }

    /// Re-enqueue a lost `queued` attempt by writing a fresh transactional-outbox
    /// row for it (the publisher relays it into `job_queue`). Idempotent: writes
    /// at most one unpublished outbox row per `(project_id, attempt)` so a second
    /// reconciler pass does not pile up duplicates.
    pub async fn reenqueue_outbox(&self, organization_id: Uuid, project_id: Uuid, attempt: i32) -> AppResult<bool> {
        // A lost queued attempt re-enqueues for immediate relay (no backoff): the
        // enqueue was lost, not failed, so there is nothing to back off from.
        let payload = crate::domain::project_clone::CloneOutboxPayload::now(project_id, attempt);
        let payload_json =
            serde_json::to_value(&payload).map_err(|e| agentforge_core::AppError::from(anyhow::Error::from(e)))?;
        // Guard against a duplicate unpublished row for the same attempt.
        let result = sqlx::query(
            r#"INSERT INTO orchestration_outbox
                   (id, organization_id, aggregate_type, aggregate_id, event_type, payload)
               SELECT gen_random_uuid(), $1, $2, $3, $4, $5
               WHERE NOT EXISTS (
                   SELECT 1 FROM orchestration_outbox o
                    WHERE o.aggregate_type = $2
                      AND o.aggregate_id = $3
                      AND o.published_at IS NULL
                      AND o.payload->>'attempt' = $6
               )"#,
        )
        .bind(organization_id)
        .bind(crate::domain::project_clone::CLONE_OUTBOX_AGGREGATE_TYPE)
        .bind(project_id)
        .bind(crate::domain::project_clone::CLONE_OUTBOX_EVENT_TYPE)
        .bind(payload_json)
        .bind(attempt.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Count attempts currently in `cloning`/`queued` for the in-flight gauge.
    pub async fn count_in_flight(&self) -> AppResult<(i64, i64)> {
        let row: (i64, i64) = sqlx::query_as(
            r#"SELECT
                   count(*) FILTER (WHERE status = $1) AS queued,
                   count(*) FILTER (WHERE status = $2) AS cloning
               FROM project_clone_attempts"#,
        )
        .bind(CloneAttemptStatus::Queued.as_str())
        .bind(CloneAttemptStatus::Cloning.as_str())
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Whether the project still exists and is not soft-deleted. The worker skips
    /// a clone whose project was deleted mid-flight rather than recreating a
    /// directory for a dead project.
    pub async fn project_is_live(&self, project_id: ProjectId) -> AppResult<bool> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (SELECT 1 FROM projects WHERE id = $1 AND deleted_at IS NULL)"#,
        )
        .bind(project_id.as_uuid())
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    /// Set the denormalized `projects.clone_status` within a caller transaction.
    async fn set_project_clone_status_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        project_id: Uuid,
        status: CloneStatus,
    ) -> AppResult<()> {
        sqlx::query(
            r#"UPDATE projects SET clone_status = $2, updated_at = now()
                WHERE id = $1 AND deleted_at IS NULL"#,
        )
        .bind(project_id)
        .bind(status.as_str())
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

/// The four nullable columns of a persisted clone-success payload, named to keep
/// the `load_clone_success` query off the `type_complexity` lint.
type StoredCloneSuccessRow = (Option<String>, Option<String>, Option<i64>, Option<i64>);

fn reconcile_candidate(row: ProjectCloneAttempt) -> ReconcileCandidate {
    ReconcileCandidate {
        id: row.id,
        organization_id: row.organization_id.as_uuid(),
        workspace_id: row.workspace_id.as_uuid(),
        project_id: row.project_id.as_uuid(),
        attempt: row.attempt,
        repository_url: row.repository_url,
        provider: row.provider,
        container_id: row.container_id,
        materialized_at: row.materialized_at,
    }
}
