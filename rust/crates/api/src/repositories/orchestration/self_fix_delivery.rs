//! Self-fix delivery queries on orchestration tasks: the per-task merge mutex,
//! the decision-transaction lock, budget and row lock, and the review-status,
//! merge-attempt and pull-request head writes. Weakening any of them would
//! bypass the Forge merge gate or the head-reconcile checks, so this file is
//! on the self-fix sensitive-path list (`SensitivePathPolicy`).

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::OrchestrationTask;
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

use super::OrchestrationTaskRepository;
use crate::domain::orchestration::OrchestrationRepositoryPolicy;

/// Advisory-lock namespace for the per-task Forge merge mutex. Distinct from
/// the `maintenance:`, `agent-lifecycle:`, `cli-image-roll:` and `participant:`
/// keys, which use the same `hashtextextended(<namespace>:..., 0)` convention.
const SELF_FIX_MERGE_LOCK_NAMESPACE: &str = "self-fix-merge";

fn merge_lock_key(scope: &TenantScope, id: Uuid) -> String {
    format!("{SELF_FIX_MERGE_LOCK_NAMESPACE}:{}:{id}", scope.org_id().as_uuid())
}

impl OrchestrationTaskRepository {
    /// The task row behind a self-fix delivery decision (merge entry and head
    /// reconcile): its review status, head, attempt count and `updated_at`.
    /// Same row type and tenant predicate as the shared `find_by_id`, kept here
    /// so the read the gate decides on is on the protected path.
    pub(crate) async fn find_for_delivery_decision<'e>(
        db: impl sqlx::PgExecutor<'e>,
        scope: &TenantScope,
        id: Uuid,
    ) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(
            "SELECT * FROM orchestration_tasks WHERE id = $1 AND organization_id = $2",
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(db)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(id))
    }

    pub(crate) async fn lock_by_id_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        id: Uuid,
    ) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(
            "SELECT * FROM orchestration_tasks WHERE organization_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(scope.org_id().as_uuid())
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(id))
    }
    /// Try the per-task Forge merge mutex: a SESSION-level advisory lock on the
    /// caller's dedicated connection (#1314). Not a transaction.
    pub(crate) async fn try_lock_merge(conn: &mut PgConnection, scope: &TenantScope, id: Uuid) -> AppResult<bool> {
        Ok(sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1, 0))")
            .bind(merge_lock_key(scope, id))
            .fetch_one(&mut *conn)
            .await?)
    }

    pub(crate) async fn unlock_merge(conn: &mut PgConnection, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
            .bind(merge_lock_key(scope, id))
            .fetch_one(&mut *conn)
            .await?;
        Ok(())
    }

    /// Store a reconciled pull-request head (tenant-scoped). The review window
    /// (`review_opened_at`) and the attempt counter are left untouched.
    pub(crate) async fn set_pr_head_sha_on(
        conn: &mut PgConnection,
        scope: &TenantScope,
        id: Uuid,
        head_sha: &str,
    ) -> AppResult<()> {
        sqlx::query("UPDATE orchestration_tasks SET pr_head_sha = $1, updated_at = NOW() WHERE id = $2 AND organization_id = $3")
            .bind(head_sha)
            .bind(id)
            .bind(scope.org_id().as_uuid())
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Update only the review status on a self-fix task (tenant-scoped). Mirrors
    /// the orchestrator `ReviewState` vocabulary but is driven API-side.
    pub async fn set_review_status(&self, scope: &TenantScope, id: Uuid, status: &str) -> AppResult<()> {
        Self::set_review_status_on(&mut *self.pool.acquire().await?, scope, id, status).await
    }

    /// Set the review status on a caller-held connection or transaction (tenant-scoped).
    pub(crate) async fn set_review_status_on(
        conn: &mut PgConnection,
        scope: &TenantScope,
        id: Uuid,
        status: &str,
    ) -> AppResult<()> {
        sqlx::query("UPDATE orchestration_tasks SET review_status = $1, updated_at = NOW() WHERE id = $2 AND organization_id = $3")
            .bind(status)
            .bind(id)
            .bind(scope.org_id().as_uuid())
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Move the review status from `from` to `to` only if it still reads `from`,
    /// so a concurrent transition (for example to `merged`) is never overwritten.
    pub(crate) async fn transition_review_status_on(
        conn: &mut PgConnection,
        scope: &TenantScope,
        id: Uuid,
        from: &str,
        to: &str,
    ) -> AppResult<()> {
        sqlx::query(
            "UPDATE orchestration_tasks SET review_status = $1, updated_at = NOW() \
             WHERE id = $2 AND organization_id = $3 AND review_status = $4",
        )
        .bind(to)
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(from)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Increment the merge-attempt counter on a self-fix task (tenant-scoped).
    ///
    /// Called immediately before each `run_merge_executor` invocation so every
    /// attempt is counted even when the executor fails. The increment is
    /// unconditional (no CAS); the cap check happens BEFORE this call, so this
    /// only runs when the attempt is within the allowed budget.
    pub async fn bump_merge_attempts(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        Self::bump_merge_attempts_in_tx(&mut tx, scope, id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// In-transaction variant used by the merge path under the task-row lock.
    pub(crate) async fn bump_merge_attempts_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        id: Uuid,
    ) -> AppResult<()> {
        sqlx::query(
            "UPDATE orchestration_tasks \
             SET merge_attempts = merge_attempts + 1, updated_at = NOW() \
             WHERE id = $1 AND organization_id = $2",
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Transaction-local `lock_timeout` for the caller's transaction: every
    /// later lock wait in it fails with SQLSTATE 55P03 after `timeout_ms`.
    pub(crate) async fn set_lock_timeout_on(conn: &mut PgConnection, timeout_ms: u64) -> AppResult<()> {
        sqlx::query("SELECT set_config('lock_timeout', $1, true)")
            .bind(format!("{timeout_ms}ms"))
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Age of the caller's transaction in milliseconds, computed by PostgreSQL
    /// (`clock_timestamp()` minus the transaction start that `NOW()` returns).
    pub(crate) async fn transaction_age_ms_on(conn: &mut PgConnection) -> AppResult<i64> {
        Ok(sqlx::query_scalar(
            "SELECT (EXTRACT(EPOCH FROM clock_timestamp() - transaction_timestamp()) * 1000)::bigint",
        )
        .fetch_one(&mut *conn)
        .await?)
    }

    /// Record the draft-PR linkage (number, URL, head SHA) and the initial
    /// review status on a self-fix task (tenant-scoped).
    pub async fn set_pr_metadata(
        &self,
        scope: &TenantScope,
        id: Uuid,
        pr_number: i32,
        pr_url: &str,
        pr_head_sha: &str,
        review_status: &str,
    ) -> AppResult<()> {
        sqlx::query(
            r#"UPDATE orchestration_tasks
               SET pr_number = $1, pr_url = $2, pr_head_sha = $3, review_status = $4,
                   -- Stamp once: never reset the review-window clock if this is re-called
                   -- (e.g. to refresh pr_head_sha after a force-push), so the stuck-review
                   -- reaper deadline cannot be silently extended.
                   review_opened_at = COALESCE(review_opened_at, NOW()), updated_at = NOW()
               WHERE id = $5 AND organization_id = $6"#,
        )
        .bind(pr_number)
        .bind(pr_url)
        .bind(pr_head_sha)
        .bind(review_status)
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
