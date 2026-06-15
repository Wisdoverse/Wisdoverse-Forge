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

use crate::domain::project_clone::{CloneAttemptStatus, CloneStatus};

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

    /// Finalize an attempt as `ready`: persist branch/head_sha/bytes/duration and
    /// mirror `projects.clone_status='ready'`. The `status='cloning'` predicate
    /// keeps the transition legal (only an in-flight clone may become ready).
    pub async fn finalize_ready(&self, attempt_id: Uuid, success: &CloneSuccess) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as::<_, ProjectCloneAttempt>(
            r#"UPDATE project_clone_attempts
                  SET status = $2,
                      resolved_branch = $3,
                      head_sha = $4,
                      bytes_cloned = $5,
                      duration_ms = $6,
                      lease_expires_at = NULL,
                      finished_at = now(),
                      updated_at = now()
                WHERE id = $1 AND status = $7
                RETURNING *"#,
        )
        .bind(attempt_id)
        .bind(CloneAttemptStatus::Ready.as_str())
        .bind(success.resolved_branch.as_deref())
        .bind(&success.head_sha)
        .bind(success.bytes_cloned)
        .bind(success.duration_ms)
        .bind(CloneAttemptStatus::Cloning.as_str())
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(ref attempt_row) = row {
            Self::set_project_clone_status_tx(&mut tx, attempt_row.project_id.as_uuid(), CloneStatus::Ready).await?;
        }
        tx.commit().await?;
        Ok(())
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
    /// the project is retrying. `run_after` backs off the outbox/job via the
    /// outbox row's natural ordering (the publisher relays oldest-first; the
    /// backoff is applied on the job's `run_at` at relay time is NOT available —
    /// so we defer by writing the outbox row and letting the worker's own
    /// dequeue `run_at` govern; here `run_after` documents the intended backoff
    /// the caller computed, applied by NOT inserting the retry until the worker's
    /// failure handler decides to).
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
        // job_queue (same contract as the M2 create path).
        let payload = crate::domain::project_clone::CloneOutboxPayload { project_id, attempt: attempt_num };
        let payload_json =
            serde_json::to_value(&payload).map_err(|e| agentforge_core::AppError::from(anyhow::Error::from(e)))?;
        sqlx::query(
            r#"INSERT INTO orchestration_outbox
                   (id, organization_id, aggregate_type, aggregate_id, event_type, payload)
               VALUES (gen_random_uuid(), $1, $2, $3, $4, $5)"#,
        )
        .bind(organization_id)
        .bind(crate::domain::project_clone::CLONE_OUTBOX_AGGREGATE_TYPE)
        .bind(project_id)
        .bind(crate::domain::project_clone::CLONE_OUTBOX_EVENT_TYPE)
        .bind(payload_json)
        .execute(&mut *tx)
        .await?;

        // Mirror the project summary back to `queued` so the UI shows a retry.
        Self::set_project_clone_status_tx(&mut tx, project_id, CloneStatus::Queued).await?;
        tx.commit().await?;
        Ok(Some(attempt_num))
    }

    /// Reconciler scan (a): attempts stuck in `cloning` whose lease has expired
    /// (the worker crashed mid-clone). Bounded by `limit`.
    pub async fn find_expired_cloning(&self, now: DateTime<Utc>, limit: i64) -> AppResult<Vec<ReconcileCandidate>> {
        let rows = sqlx::query_as::<_, ProjectCloneAttempt>(
            r#"SELECT * FROM project_clone_attempts
                WHERE status = $1
                  AND lease_expires_at IS NOT NULL
                  AND lease_expires_at < $2
                ORDER BY lease_expires_at ASC
                LIMIT $3"#,
        )
        .bind(CloneAttemptStatus::Cloning.as_str())
        .bind(now)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(reconcile_candidate).collect())
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
        let payload = crate::domain::project_clone::CloneOutboxPayload { project_id, attempt };
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
    }
}
