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

/// SQL that ages out `task_dispatches` rows in non-terminal states whose
/// `updated_at` has exceeded the TTL. Kept as a `pub(crate) const` so the
/// SQL-pin unit test can assert the scope predicates without a live database.
pub(crate) const REAP_STUCK_DISPATCHES_SQL: &str = "
    UPDATE task_dispatches
    SET status = 'failed',
        last_error = 'dispatch_timeout',
        updated_at = NOW()
    WHERE status IN ('queued', 'starting')
      AND updated_at < NOW() - make_interval(secs => $1)
";

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
                Ok(0) => {
                    consecutive_failures = 0;
                }
                Ok(n) => {
                    consecutive_failures = 0;
                    tracing::warn!(
                        reaped = n,
                        ttl_secs = self.ttl_secs,
                        "dispatch reaper aged out stuck task_dispatches — dispatch did not complete within the TTL"
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
    /// dispatches timed out.
    pub async fn tick(pool: &PgPool, ttl_secs: u64) -> sqlx::Result<u64> {
        let result = sqlx::query(REAP_STUCK_DISPATCHES_SQL).bind(ttl_secs as f64).execute(pool).await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::REAP_STUCK_DISPATCHES_SQL;

    /// Pin the SQL scope predicates so a future "simplification" that widens
    /// the WHERE clause (e.g. touching `started`/`failed`) fails before review.
    #[test]
    fn reap_dispatches_sql_pins_correct_predicates() {
        let sql = REAP_STUCK_DISPATCHES_SQL;
        assert!(sql.contains("status IN ('queued', 'starting')"));
        assert!(sql.contains("updated_at <"));
        assert!(sql.contains("status = 'failed'"));
        assert!(sql.contains("last_error = 'dispatch_timeout'"));
        assert!(!sql.contains("'started'"));
    }
}
