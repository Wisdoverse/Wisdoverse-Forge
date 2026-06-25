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
///
/// F051: the sweep is BOUNDED. The `stuck` CTE selects at most
/// [`REAP_BATCH_LIMIT`] of the oldest eligible rows with `FOR UPDATE SKIP
/// LOCKED`, so one statement never locks the entire stuck set (which, after an
/// orchestrator crash, can be thousands of rows) in a single long transaction
/// that contends with live `update_dispatch()` writers and bloats WAL.
/// [`DispatchReaperWorker::tick`] loops this statement until a pass reaps zero,
/// matching the project's batched-backfill convention (migration 062).
pub(crate) const REAP_STUCK_DISPATCHES_SQL: &str = "
    WITH stuck AS (
        SELECT id
        FROM task_dispatches
        WHERE status IN ('queued', 'starting')
          AND updated_at < NOW() - make_interval(secs => $1)
        ORDER BY updated_at
        LIMIT 1000
        FOR UPDATE SKIP LOCKED
    ),
    reaped AS (
        UPDATE task_dispatches
        SET status = 'failed',
            last_error = 'dispatch_timeout',
            updated_at = NOW()
        WHERE id IN (SELECT id FROM stuck)
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

/// Per-statement cap for the bounded reap sweep (F051). Mirrors the literal
/// `LIMIT 1000` pinned in [`REAP_STUCK_DISPATCHES_SQL`]; kept here so the loop
/// in [`DispatchReaperWorker::tick`] and the SQL-pin test reference one value.
pub(crate) const REAP_BATCH_LIMIT: u64 = 1000;

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

    /// One reap pass. Exposed for tests. Returns the TOTAL dispatches timed out
    /// and owning tasks reset to pending across however many bounded batches the
    /// current stuck backlog needs.
    ///
    /// F051: each statement caps at [`REAP_BATCH_LIMIT`] rows
    /// (`FOR UPDATE SKIP LOCKED`), and this loops until a batch reaps zero — so a
    /// large stuck backlog drains in bounded transactions instead of one
    /// table-wide lock. Termination is guaranteed: each batch flips its rows to
    /// `failed` (leaving the `queued`/`starting` eligible set), and rows arriving
    /// after the first statement have `updated_at = NOW()` so they are not yet
    /// past the TTL.
    pub async fn tick(pool: &PgPool, ttl_secs: u64) -> sqlx::Result<ReapOutcome> {
        let mut total = ReapOutcome { dispatches_reaped: 0, tasks_reconciled: 0 };
        loop {
            let (dispatches_reaped, tasks_reconciled): (i64, i64) =
                sqlx::query_as(REAP_STUCK_DISPATCHES_SQL).bind(ttl_secs as f64).fetch_one(pool).await?;
            let batch = dispatches_reaped.max(0) as u64;
            total.dispatches_reaped += batch;
            total.tasks_reconciled += tasks_reconciled.max(0) as u64;
            if batch < REAP_BATCH_LIMIT {
                break;
            }
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::{REAP_BATCH_LIMIT, REAP_STUCK_DISPATCHES_SQL};

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

    /// F051: pin the bounded-sweep shape so a future edit cannot revert to the
    /// unbounded single-statement UPDATE that locks the whole stuck set.
    #[test]
    fn reap_dispatches_sql_is_bounded_and_skip_locked() {
        let sql = REAP_STUCK_DISPATCHES_SQL;
        assert!(sql.contains("FOR UPDATE SKIP LOCKED"), "reap must take row locks with SKIP LOCKED");
        assert!(sql.contains("LIMIT 1000"), "reap must cap each batch");
        assert!(sql.contains("ORDER BY updated_at"), "reap must take the oldest stuck rows first");
        // The cap constant and the SQL literal must agree.
        assert_eq!(REAP_BATCH_LIMIT, 1000);
        assert!(sql.contains(&format!("LIMIT {REAP_BATCH_LIMIT}")));
    }
}
