//! Stale job-queue lock reaper (issue #892, finding F044).
//!
//! [`crate::queue::dequeue`] claims a job by flipping it to `status='running'`
//! and stamping `locked_at`. A worker that crashes (OOM, node reboot, deploy)
//! after claiming but before calling [`crate::queue::complete`] or
//! [`crate::queue::fail`] leaves the row wedged in `running` forever — no other
//! worker will ever pick it up.
//!
//! [`crate::queue::release_stale_locks`] fixes that, but until this worker was
//! added nothing ever called it. This reaper runs it on a fixed interval so
//! abandoned locks are returned to `pending` and re-dispatched. Releasing does
//! NOT consume a retry attempt (see `release_stale_locks`), so a job whose
//! worker died keeps its full retry budget.

use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::watch;

/// Describe + prime the reaper metrics at zero so a Prometheus scrape returns
/// the series even before any lock is released.
pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_job_queue_stale_locks_released_total",
        "Total number of abandoned running job_queue locks released back to pending by the reaper"
    );
    metrics::describe_counter!(
        "agentforge_job_queue_stale_lock_reaper_tick_errors_total",
        "Total number of stale-job-lock reaper tick errors"
    );
    metrics::counter!("agentforge_job_queue_stale_locks_released_total").increment(0);
    metrics::counter!("agentforge_job_queue_stale_lock_reaper_tick_errors_total").increment(0);
}

pub struct StaleJobLockReaperWorker {
    pool: PgPool,
    timeout_secs: u64,
    interval: Duration,
}

impl StaleJobLockReaperWorker {
    /// Create a new reaper. The sweep interval defaults to 60 seconds; a lock
    /// is reaped once it is older than `timeout_secs`.
    pub fn new(pool: PgPool, timeout_secs: u64) -> Self {
        Self { pool, timeout_secs, interval: Duration::from_secs(60) }
    }

    /// Run the reaper loop until shutdown is signalled.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        tracing::info!(
            timeout_secs = self.timeout_secs,
            interval_secs = self.interval.as_secs(),
            "stale job-lock reaper started"
        );
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() { break; }
                }
                _ = ticker.tick() => {
                    match crate::queue::release_stale_locks(&self.pool, self.timeout_secs).await {
                        Ok(0) => {}
                        Ok(n) => {
                            tracing::warn!(
                                released = n,
                                timeout_secs = self.timeout_secs,
                                "stale job-lock reaper released abandoned job locks — a worker likely crashed mid-job"
                            );
                            metrics::counter!("agentforge_job_queue_stale_locks_released_total").increment(n);
                        }
                        Err(err) => {
                            tracing::warn!(error = ?err, "stale job-lock reaper tick failed");
                            metrics::counter!("agentforge_job_queue_stale_lock_reaper_tick_errors_total")
                                .increment(1);
                        }
                    }
                }
            }
        }
        tracing::info!("stale job-lock reaper shut down");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_metrics_primes_series() {
        register_metrics();
    }

    /// A `running` job whose lock is older than the timeout is released back to
    /// `pending`; a freshly-locked job is left alone.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn reaps_only_stale_running_locks(pool: PgPool) {
        let stale = crate::queue::enqueue(&pool, "q", serde_json::json!({}), 0, None, None, 3).await.unwrap().unwrap();
        let fresh = crate::queue::enqueue(&pool, "q", serde_json::json!({}), 0, None, None, 3).await.unwrap().unwrap();

        crate::queue::dequeue(&pool, "q", "w1").await.unwrap().expect("claim stale");
        crate::queue::dequeue(&pool, "q", "w2").await.unwrap().expect("claim fresh");
        // Age only the stale job's lock past the timeout.
        sqlx::query("UPDATE job_queue SET locked_at = now() - interval '1 hour' WHERE id = $1")
            .bind(stale)
            .execute(&pool)
            .await
            .unwrap();

        let released = crate::queue::release_stale_locks(&pool, 60).await.unwrap();
        assert_eq!(released, 1, "only the aged lock should be released");

        let stale_status: String = sqlx::query_scalar("SELECT status FROM job_queue WHERE id = $1")
            .bind(stale)
            .fetch_one(&pool)
            .await
            .unwrap();
        let fresh_status: String = sqlx::query_scalar("SELECT status FROM job_queue WHERE id = $1")
            .bind(fresh)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stale_status, "pending", "stale lock returns to pending");
        assert_eq!(fresh_status, "running", "fresh lock is untouched");
    }
}
