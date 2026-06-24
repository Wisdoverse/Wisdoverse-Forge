//! Dispatch TTL reaper (issue #868).
//!
//! Rows in `task_dispatches` stuck in `queued` or `starting` status have no
//! built-in deadline (the detached spawn that drives them can die mid-flight,
//! e.g. on an orchestrator crash). This worker ages them out after a
//! configurable TTL so the table does not accumulate indefinitely stale entries.
//!
//! Scope: strictly `queued` and `starting` rows only. `started` and `failed`
//! rows are terminal and are never touched here.
//!
//! Observability: the orchestrator does not install a Prometheus recorder (its
//! `/metrics` surface is the query-based dashboard JSON, not a scrape endpoint),
//! so this reaper surfaces activity via `tracing` warnings (`reaped = N`) rather
//! than a Prometheus counter that would silently no-op.

use std::time::Duration;

use sqlx::PgPool;

/// SQL that, in one atomic statement, (1) ages out `task_dispatches` rows in
/// non-terminal states past the TTL, and (2) resets the owning `assigned` task
/// back to `pending` so it is re-dispatchable instead of silently dropped.
///
/// Both data-modifying CTEs run under a single snapshot, so a dispatch and its
/// task move together. `task_dispatches.task_id` holds the task UUID as text,
/// hence the `t.id::text = r.task_id` join. The `t.state = 'assigned'` guard
/// leaves `working` tasks (agent actively on them) alone. Returns the dispatch
/// and reconciled-task counts. Kept as a `pub(crate) const` so the SQL-pin unit
/// test can assert the scope predicates without a live database.
pub(crate) const REAP_STUCK_DISPATCHES_SQL: &str = "
    WITH reaped AS (
        UPDATE task_dispatches
        SET status = 'failed',
            last_error = 'dispatch_timeout',
            updated_at = NOW()
        WHERE status IN ('queued', 'starting')
          AND updated_at < NOW() - make_interval(secs => $1)
        RETURNING task_id, org_id
    ),
    reconciled AS (
        UPDATE tasks t
        SET state = 'pending',
            updated_at = NOW()
        FROM reaped r
        WHERE t.id::text = r.task_id
          AND t.org_id = r.org_id
          AND t.state = 'assigned'
        RETURNING t.id
    )
    SELECT
        (SELECT COUNT(*) FROM reaped)::bigint AS dispatches_reaped,
        (SELECT COUNT(*) FROM reconciled)::bigint AS tasks_reconciled
";

/// Outcome of one reap pass: how many stuck dispatches were failed and how many
/// owning tasks were reset to `pending` for re-dispatch (F039).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReapOutcome {
    pub dispatches_reaped: u64,
    pub tasks_reconciled: u64,
}

pub struct DispatchReaperWorker {
    pool: PgPool,
    ttl_secs: u64,
    interval: Duration,
}

impl DispatchReaperWorker {
    /// Create a new reaper. The sweep interval defaults to 60 seconds.
    pub fn new(pool: PgPool, ttl_secs: u64) -> Self {
        Self { pool, ttl_secs, interval: Duration::from_secs(60) }
    }

    /// Run the reaper loop. The orchestrator has no shutdown watch channel —
    /// the process exit stops the spawned task naturally, and each tick is an
    /// idempotent single `UPDATE`, so an abrupt stop loses no work.
    pub async fn run(self) {
        tracing::info!(ttl_secs = self.ttl_secs, interval_secs = self.interval.as_secs(), "dispatch reaper started");
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut consecutive_failures: u32 = 0;
        loop {
            ticker.tick().await;
            match Self::tick(&self.pool, self.ttl_secs).await {
                Ok(outcome) if outcome.dispatches_reaped == 0 => {
                    consecutive_failures = 0;
                }
                Ok(outcome) => {
                    consecutive_failures = 0;
                    tracing::warn!(
                        reaped = outcome.dispatches_reaped,
                        tasks_reconciled = outcome.tasks_reconciled,
                        ttl_secs = self.ttl_secs,
                        "dispatch reaper aged out stuck task_dispatches and reset their tasks to pending — dispatch did not complete within the TTL"
                    );
                }
                Err(err) => {
                    consecutive_failures += 1;
                    if consecutive_failures >= 3 {
                        tracing::error!(
                            error = ?err,
                            consecutive_failures,
                            "dispatch reaper tick failed repeatedly — stuck task_dispatches are not being aged out"
                        );
                    } else {
                        tracing::warn!(error = ?err, consecutive_failures, "dispatch reaper tick failed");
                    }
                }
            }
        }
    }

    /// Single-shot reap pass. Exposed for tests. Returns the number of
    /// dispatches timed out and owning tasks reset to pending.
    pub async fn tick(pool: &PgPool, ttl_secs: u64) -> sqlx::Result<ReapOutcome> {
        let (dispatches_reaped, tasks_reconciled): (i64, i64) =
            sqlx::query_as(REAP_STUCK_DISPATCHES_SQL).bind(ttl_secs as f64).fetch_one(pool).await?;
        Ok(ReapOutcome {
            dispatches_reaped: dispatches_reaped.max(0) as u64,
            tasks_reconciled: tasks_reconciled.max(0) as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::REAP_STUCK_DISPATCHES_SQL;

    /// Pin the SQL scope predicates so a future "simplification" that widens
    /// the WHERE clause (e.g. touching `started`/`failed`, or resetting a
    /// `working` task) fails before review.
    #[test]
    fn reap_dispatches_sql_pins_correct_predicates() {
        let sql = REAP_STUCK_DISPATCHES_SQL;
        // Dispatch reap scope.
        assert!(sql.contains("status IN ('queued', 'starting')"));
        assert!(sql.contains("updated_at <"));
        assert!(sql.contains("status = 'failed'"));
        assert!(sql.contains("last_error = 'dispatch_timeout'"));
        assert!(!sql.contains("'started'"));
        // Task reconciliation scope (F039): only assigned tasks -> pending.
        assert!(sql.contains("UPDATE tasks"));
        assert!(sql.contains("state = 'pending'"));
        assert!(sql.contains("t.state = 'assigned'"));
        assert!(sql.contains("t.id::text = r.task_id"));
        assert!(!sql.contains("t.state = 'working'"));
    }
}
