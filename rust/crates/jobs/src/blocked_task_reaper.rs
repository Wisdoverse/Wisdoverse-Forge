//! Blocked-task TTL reaper (issue #810).
//!
//! Tasks parked in `status='blocked'` with `blocked_reason='waiting_agent'`
//! (set by `try_auto_dispatch` when no agent is free) have no built-in
//! deadline — they sit eligible forever. This worker ages them out after a
//! configurable TTL so the queue does not accumulate indefinitely stale entries.
//!
//! Scope: strictly `blocked/waiting_agent` rows only. Tasks blocked for other
//! reasons (`waiting_dependency`, `waiting_approval`, `waiting_input`) are
//! governed by their own lifecycle and are not touched here.

use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::watch;

/// SQL that ages out `blocked/waiting_agent` tasks whose `updated_at` has
/// exceeded the TTL. Kept as a `pub(crate) const` so the SQL-pin unit test
/// can assert the scope predicates without a live database.
pub(crate) const REAP_BLOCKED_TASKS_SQL: &str = "
    UPDATE orchestration_tasks
    SET status = 'canceled',
        failure_code = 'waiting_agent_timeout',
        retryable = FALSE,
        updated_at = NOW()
    WHERE status = 'blocked'
      AND blocked_reason = 'waiting_agent'
      AND updated_at < NOW() - make_interval(secs => $1)
";

/// Describe + prime blocked-task-reaper metrics at zero so Prometheus scrape
/// returns the metric even before any event fires. `describe_*` sets help
/// text only; an explicit `increment(0)` primes the sample so dashboards
/// render from t=0 instead of "metric not found" (idempotent under
/// re-registration, unlike `absolute(0)`).
pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_orchestration_blocked_tasks_reaped_total",
        "Total number of blocked waiting_agent tasks aged out by the TTL reaper"
    );
    metrics::describe_counter!(
        "agentforge_orchestration_blocked_task_reaper_tick_errors_total",
        "Total number of blocked task reaper tick errors"
    );
    metrics::counter!("agentforge_orchestration_blocked_tasks_reaped_total").increment(0);
    metrics::counter!("agentforge_orchestration_blocked_task_reaper_tick_errors_total").increment(0);
}

pub struct BlockedTaskReaperWorker {
    pool: PgPool,
    ttl_secs: u64,
    interval: Duration,
}

impl BlockedTaskReaperWorker {
    /// Create a new reaper. The sweep interval defaults to 60 seconds.
    pub fn new(pool: PgPool, ttl_secs: u64) -> Self {
        Self { pool, ttl_secs, interval: Duration::from_secs(60) }
    }

    /// Run the reaper loop until shutdown is signalled. Each tick scans once
    /// and cancels any `blocked/waiting_agent` rows older than `ttl_secs`.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        tracing::info!(
            ttl_secs = self.ttl_secs,
            interval_secs = self.interval.as_secs(),
            "blocked task reaper started"
        );
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() { break; }
                }
                _ = ticker.tick() => {
                    match Self::tick(&self.pool, self.ttl_secs).await {
                        Ok(0) => {}
                        Ok(n) => {
                            tracing::warn!(
                                reaped = n,
                                ttl_secs = self.ttl_secs,
                                "blocked task reaper aged out waiting_agent tasks — no agent was free within the TTL"
                            );
                            metrics::counter!("agentforge_orchestration_blocked_tasks_reaped_total").increment(n);
                        }
                        Err(err) => {
                            tracing::warn!(error = ?err, "blocked task reaper tick failed");
                            metrics::counter!("agentforge_orchestration_blocked_task_reaper_tick_errors_total")
                                .increment(1);
                        }
                    }
                }
            }
        }
        tracing::info!("blocked task reaper shut down");
    }

    /// Single-shot reap pass. Exposed for tests. Returns the number of tasks
    /// canceled.
    pub(crate) async fn tick(pool: &PgPool, ttl_secs: u64) -> sqlx::Result<u64> {
        let result = sqlx::query(REAP_BLOCKED_TASKS_SQL).bind(ttl_secs as f64).execute(pool).await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::{REAP_BLOCKED_TASKS_SQL, register_metrics};

    /// Verify that `register_metrics` can be called without panicking.
    #[test]
    fn register_metrics_primes_series() {
        register_metrics();
    }

    /// Pin the SQL scope predicates so a future "simplification" that widens
    /// the WHERE clause (e.g. dropping `blocked_reason`) fails before review.
    #[test]
    fn reap_blocked_sql_pins_correct_predicates() {
        let sql = REAP_BLOCKED_TASKS_SQL;
        assert!(sql.contains("status = 'blocked'"));
        assert!(sql.contains("blocked_reason = 'waiting_agent'"));
        assert!(sql.contains("updated_at <"));
        assert!(sql.contains("status = 'canceled'"));
        assert!(!sql.contains("waiting_dependency"));
        assert!(!sql.contains("waiting_approval"));
        assert!(!sql.contains("waiting_input"));
        assert!(!sql.contains("quota_exceeded"));
    }

    /// Integration test: only stale `waiting_agent` blocked tasks are reaped;
    /// fresh ones and those with a different blocked_reason are left alone.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn reaps_only_stale_waiting_agent_blocked(pool: sqlx::PgPool) {
        use uuid::Uuid;

        // Seed a minimal org row (required FK for orchestration_tasks).
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org_id)
            .bind("Reaper Test Org")
            .bind(format!("reaper-org-{org_id}"))
            .execute(&pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_id)
            .execute(&pool)
            .await
            .expect("seed workspace");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(&pool)
            .await
            .expect("seed user");

        // Task A: stale waiting_agent blocked — should be reaped with ttl=3600.
        let task_a = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, blocked_reason, created_by, updated_at)
               VALUES ($1, $2, 'Task A', 'blocked', 'waiting_agent', $3,
                       NOW() - INTERVAL '2 hours')"#,
        )
        .bind(task_a)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed task A");

        // Task B: fresh waiting_agent blocked — should NOT be reaped.
        let task_b = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, blocked_reason, created_by, updated_at)
               VALUES ($1, $2, 'Task B', 'blocked', 'waiting_agent', $3, NOW())"#,
        )
        .bind(task_b)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed task B");

        // Task C: stale but wrong blocked_reason — should NOT be reaped.
        let task_c = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, blocked_reason, created_by, updated_at)
               VALUES ($1, $2, 'Task C', 'blocked', 'waiting_dependency', $3,
                       NOW() - INTERVAL '2 hours')"#,
        )
        .bind(task_c)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed task C");

        // Run the reaper with a 3600s TTL — only Task A qualifies.
        let reaped = super::BlockedTaskReaperWorker::tick(&pool, 3600).await.expect("tick should succeed");
        assert_eq!(reaped, 1, "expected exactly 1 task reaped");

        // Task A must now be 'canceled'.
        let status_a: String = sqlx::query_scalar("SELECT status FROM orchestration_tasks WHERE id = $1")
            .bind(task_a)
            .fetch_one(&pool)
            .await
            .expect("fetch task A");
        assert_eq!(status_a, "canceled", "task A should be canceled");

        // Task B must still be 'blocked'.
        let status_b: String = sqlx::query_scalar("SELECT status FROM orchestration_tasks WHERE id = $1")
            .bind(task_b)
            .fetch_one(&pool)
            .await
            .expect("fetch task B");
        assert_eq!(status_b, "blocked", "task B should still be blocked");

        // Task C must still be 'blocked'.
        let status_c: String = sqlx::query_scalar("SELECT status FROM orchestration_tasks WHERE id = $1")
            .bind(task_c)
            .fetch_one(&pool)
            .await
            .expect("fetch task C");
        assert_eq!(status_c, "blocked", "task C should still be blocked");
    }
}
