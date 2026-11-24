//! Per-task Forge merge mutex (#1314): a session-level advisory lock held on a
//! dedicated pool connection for the whole approve call. It is not a
//! transaction, so nothing stays open across GitHub calls.

use agentforge_core::{AppResult, TenantScope};
use sqlx::pool::PoolConnection;
use sqlx::{PgConnection, PgPool, Postgres};
use uuid::Uuid;

use crate::repositories::orchestration::OrchestrationTaskRepository;

pub(crate) struct MergeLock {
    conn: PoolConnection<Postgres>,
    scope: TenantScope,
    task: Uuid,
}

impl MergeLock {
    /// `None` when another approve call holds the lock for this task.
    pub(crate) async fn try_acquire(pool: &PgPool, scope: &TenantScope, task: Uuid) -> AppResult<Option<Self>> {
        let mut conn = pool.acquire().await?;
        // Closed instead of pooled when dropped: a session lock must never go
        // back into the pool still held, including after a cancelled request
        // or a panic.
        conn.close_on_drop();
        if !OrchestrationTaskRepository::try_lock_merge(&mut conn, scope, task).await? {
            return Ok(None);
        }
        Ok(Some(Self { conn, scope: scope.clone(), task }))
    }

    /// The locked connection. The whole approve call runs its SQL here (the
    /// session lock survives transactions on it), so one call needs at most
    /// one pooled connection.
    pub(crate) fn conn(&mut self) -> &mut PgConnection {
        &mut self.conn
    }

    /// Explicit unlock on normal exits; closing the connection covers the rest.
    pub(crate) async fn release(mut self) {
        if let Err(err) = OrchestrationTaskRepository::unlock_merge(&mut self.conn, &self.scope, self.task).await {
            tracing::warn!(error = ?err, task_id = %self.task, "self-fix: merge lock unlock failed; closing its connection releases it");
        }
    }
}
