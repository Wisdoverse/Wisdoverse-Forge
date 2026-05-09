//! Dependency reconcile worker — defense-in-depth backstop for `complete_task`.
//!
//! Issue #37 makes `complete_task` transactional, so the parent-completion +
//! children-unblock pair is atomic in the happy path. This worker exists for
//! the unhappy paths the tx alone can't catch:
//!
//! - Crashes between commit and the application observing the commit.
//! - Children that were inserted *after* the parent completed (race: parent
//!   finishes, then a late `add_subtask` lands with `parent_task_id` pointing
//!   at the just-completed parent and `status='blocked'/blocked_reason='waiting_dependency'`
//!   because the child-side check ran against pre-commit state).
//! - Historical orphan rows from before the transactional fix (one-time backfill).
//!
//! The worker periodically scans for `blocked/waiting_dependency` rows whose
//! parent is already `completed` (within the same `organization_id`) and flips
//! them back to `queued`. Cross-tenant safe by construction: the join
//! condition forces parent and child to share `organization_id`.

use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::watch;

/// Reconcile SQL — kept as a `pub(crate) const` so the query-shape unit
/// test pins the cross-tenant join condition. Drop the
/// `child.organization_id = parent.organization_id` predicate and one
/// tenant's "completed" parent could unblock another tenant's children.
pub(crate) const RECONCILE_SQL: &str = r#"UPDATE orchestration_tasks AS child
        SET status = 'queued',
            blocked_reason = NULL,
            blocked_metadata = NULL,
            updated_at = NOW()
        FROM orchestration_tasks AS parent
        WHERE child.parent_task_id = parent.id
          AND child.organization_id = parent.organization_id
          AND child.status = 'blocked'
          AND child.blocked_reason = 'waiting_dependency'
          AND parent.status = 'completed'
        RETURNING child.id"#;

/// Default reconcile cadence. Picked to be slow enough not to add measurable
/// load (<1 query/min per server) and fast enough that an orphaned child is
/// observed within a few minutes — within the typical operator alerting
/// reaction window.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(60);

pub struct DependencyReconcileWorker {
    pool: PgPool,
    interval: Duration,
}

impl DependencyReconcileWorker {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, interval: DEFAULT_INTERVAL }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Run the reconcile loop until shutdown is signalled. Each tick scans
    /// once and logs how many rows it unblocked.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        tracing::info!(interval_secs = self.interval.as_secs(), "dependency reconcile worker started");
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() { break; }
                }
                _ = ticker.tick() => {
                    match Self::tick(&self.pool).await {
                        Ok(0) => {}
                        Ok(n) => tracing::warn!(
                            unblocked = n,
                            "dependency reconcile unblocked orphan children — investigate why complete_task tx didn't"
                        ),
                        Err(err) => tracing::error!(error = ?err, "dependency reconcile tick failed"),
                    }
                }
            }
        }
        tracing::info!("dependency reconcile worker shut down");
    }

    /// Single-shot reconcile pass. Exposed for tests and one-off backfill.
    /// Returns the number of rows unblocked.
    ///
    /// Note: this only flips children to `queued`. Pickup is event-driven —
    /// the next participant heartbeat, task creation, or sweep call from the
    /// API path will claim them via `next_dispatchable`. We intentionally
    /// don't `pg_notify` here: reconcile firing means the happy-path tx
    /// already failed once, so the operator alert is more valuable than
    /// shaving seconds off pickup latency.
    pub async fn tick(pool: &PgPool) -> sqlx::Result<u64> {
        let result = sqlx::query(RECONCILE_SQL).execute(pool).await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::RECONCILE_SQL;

    // We don't have a DB integration harness in this crate. Pin the
    // tenant-isolation predicate so a future "simplification" (e.g. dropping
    // the FROM-join cross-org predicate) fails before review.
    #[test]
    fn reconcile_sql_pins_cross_tenant_join() {
        assert!(
            RECONCILE_SQL.contains("child.organization_id = parent.organization_id"),
            "reconcile must require parent + child to share organization_id"
        );
        assert!(
            RECONCILE_SQL.contains("child.parent_task_id = parent.id"),
            "reconcile must restrict to direct parent->child relationships"
        );
        assert!(RECONCILE_SQL.contains("child.status = 'blocked'"), "reconcile must only touch rows currently blocked");
        assert!(
            RECONCILE_SQL.contains("child.blocked_reason = 'waiting_dependency'"),
            "reconcile must only touch rows blocked on dependency"
        );
        assert!(
            RECONCILE_SQL.contains("parent.status = 'completed'"),
            "reconcile must only fire when the parent has completed"
        );
    }
}
