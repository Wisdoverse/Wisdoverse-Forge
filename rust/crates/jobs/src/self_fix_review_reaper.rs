//! Self-fix review reaper — backstop that flips `in_review` self-fix tasks to
//! `changes_requested` when the PR has been open longer than the configured
//! review deadline.
//!
//! The happy path is a human reviewer (or an automated review bot) updating
//! `review_status` via the review API before the deadline expires. This worker
//! handles the unhappy path: PRs that are never reviewed, where the reviewer
//! disappears, or where the GitHub webhook delivery that would have updated the
//! status was dropped.
//!
//! When a task is reaped, its `review_status` moves from `in_review` to
//! `changes_requested`. The self-fix loop's dispatch logic treats
//! `changes_requested` as a signal to re-queue the task for another fix
//! attempt, so a reaped task is not permanently dead — it re-enters the queue
//! for a fresh pass.

use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::watch;

/// Describe + prime self-fix review reaper metrics at zero so Prometheus scrape
/// returns the metric even before any event fires. `describe_*` sets help
/// text only; an explicit `increment(0)` primes the sample so dashboards
/// render from t=0 instead of "metric not found".
pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_self_fix_review_reaped_total",
        "Self-fix tasks whose in_review status was reaped past the review deadline (rate>0 means PRs are not being reviewed in time)."
    );
    metrics::describe_counter!(
        "agentforge_self_fix_review_reaper_tick_errors_total",
        "Self-fix review reaper tick failures (DB errors during the reap pass)."
    );
    // Prime so the metric exists on /metrics before first event.
    metrics::counter!("agentforge_self_fix_review_reaped_total").increment(0);
    metrics::counter!("agentforge_self_fix_review_reaper_tick_errors_total").increment(0);
}

/// Reaper SQL — kept as a `pub(crate) const` so the query-shape unit test can
/// pin the filter conditions. The `make_interval(secs => $1)` form accepts a
/// `FLOAT8` binding and works for sub-second values in tests; production passes
/// the configured deadline in whole seconds (default 604800 = 7 days).
///
/// Only `in_review` tasks are touched — `approved` and `merged` tasks must
/// never be regressed, and `changes_requested` tasks are already in the
/// re-queue pipeline.
pub(crate) const REAP_STUCK_REVIEWS_SQL: &str = "UPDATE orchestration_tasks \
    SET review_status = 'changes_requested', updated_at = NOW() \
    WHERE self_fix = TRUE \
    AND review_status = 'in_review' \
    AND review_opened_at IS NOT NULL \
    AND review_opened_at < NOW() - make_interval(secs => $1)";

/// Default reaper cadence. 5 minutes matches `dependency_reconcile` — slow
/// enough not to add measurable load (<1 query/5 min per server) and fast
/// enough to detect expired reviews within a reasonable operator reaction
/// window. The actual deadline is driven by `AppConfig::self_fix_review_deadline_secs`
/// (default 7 days), so the check cadence is irrelevant to correctness.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(300);

pub struct SelfFixReviewReaperWorker {
    pool: PgPool,
    deadline_secs: u64,
    interval: Duration,
}

impl SelfFixReviewReaperWorker {
    pub fn new(pool: PgPool, deadline_secs: u64) -> Self {
        Self { pool, deadline_secs, interval: DEFAULT_INTERVAL }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Run the reaper loop until shutdown is signalled. Each tick scans once
    /// and logs how many tasks it reaped.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        tracing::info!(
            interval_secs = self.interval.as_secs(),
            deadline_secs = self.deadline_secs,
            "self-fix review reaper worker started"
        );
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() { break; }
                }
                _ = ticker.tick() => {
                    match Self::tick(&self.pool, self.deadline_secs).await {
                        Ok(0) => {}
                        Ok(n) => {
                            tracing::warn!(
                                reaped = n,
                                deadline_secs = self.deadline_secs,
                                "self-fix review reaper flipped stuck in_review tasks to changes_requested"
                            );
                            metrics::counter!("agentforge_self_fix_review_reaped_total").increment(n);
                        }
                        Err(err) => {
                            tracing::error!(error = ?err, "self-fix review reaper tick failed");
                            metrics::counter!("agentforge_self_fix_review_reaper_tick_errors_total").increment(1);
                        }
                    }
                }
            }
        }
        tracing::info!("self-fix review reaper worker shut down");
    }

    /// Single-shot reaper pass. Exposed for tests and one-off backfill.
    /// Returns the number of tasks reaped.
    pub async fn tick(pool: &PgPool, deadline_secs: u64) -> sqlx::Result<u64> {
        let result = sqlx::query(REAP_STUCK_REVIEWS_SQL)
            .bind(deadline_secs as f64)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::{REAP_STUCK_REVIEWS_SQL, register_metrics};

    /// Verify that `register_metrics` can be called without panicking.
    #[test]
    fn register_metrics_primes_series() {
        register_metrics();
    }

    /// Pin the SQL shape so a future "simplification" cannot accidentally
    /// include approved/merged tasks or drop the self_fix guard.
    #[test]
    fn reap_sql_pins_filter_conditions() {
        assert!(
            REAP_STUCK_REVIEWS_SQL.contains("review_status = 'in_review'"),
            "reaper must only touch in_review rows"
        );
        assert!(
            REAP_STUCK_REVIEWS_SQL.contains("self_fix = TRUE"),
            "reaper must only touch self_fix tasks"
        );
        assert!(
            REAP_STUCK_REVIEWS_SQL.contains("review_opened_at"),
            "reaper must use review_opened_at as the age predicate"
        );
        assert!(
            !REAP_STUCK_REVIEWS_SQL.contains("'approved'"),
            "reaper must NOT touch approved tasks"
        );
        assert!(
            !REAP_STUCK_REVIEWS_SQL.contains("'merged'"),
            "reaper must NOT touch merged tasks"
        );
    }

    /// Integration test: seed three tasks and verify only the stale `in_review`
    /// task is reaped. Requires a live PostgreSQL database wired via `DATABASE_URL`.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn tick_ages_out_stuck_in_review_only(pool: sqlx::PgPool) {
        use chrono::Utc;
        use sqlx::types::Uuid;

        // Seed the minimum required rows for FK constraints.
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org_id)
            .bind(format!("Reaper Test Org {org_id}"))
            .bind(format!("reaper-test-{org_id}"))
            .execute(&pool)
            .await
            .expect("seed org");

        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("reaper-{user_id}@example.com"))
            .execute(&pool)
            .await
            .expect("seed user");

        // Helper: insert a minimal self_fix orchestration_task.
        let insert_task = |pool: sqlx::PgPool,
                           id: Uuid,
                           review_status: &'static str,
                           review_opened_at: Option<chrono::DateTime<Utc>>| async move {
            sqlx::query(
                r#"INSERT INTO orchestration_tasks
                   (id, organization_id, title, status, created_by, self_fix, review_status, review_opened_at)
                   VALUES ($1, $2, $3, 'pending', $4, TRUE, $5, $6)"#,
            )
            .bind(id)
            .bind(org_id)
            .bind(format!("task-{id}"))
            .bind(user_id)
            .bind(review_status)
            .bind(review_opened_at)
            .execute(&pool)
            .await
            .expect("seed task");
        };

        let stale_id = Uuid::new_v4();
        let fresh_id = Uuid::new_v4();
        let approved_id = Uuid::new_v4();

        // 1. Stale in_review: opened 8 days ago — should be reaped.
        let eight_days_ago = Utc::now() - chrono::Duration::days(8);
        insert_task(pool.clone(), stale_id, "in_review", Some(eight_days_ago)).await;

        // 2. Fresh in_review: opened just now — must NOT be reaped.
        insert_task(pool.clone(), fresh_id, "in_review", Some(Utc::now())).await;

        // 3. Stale approved: opened 8 days ago — must NOT be reaped (wrong status).
        insert_task(pool.clone(), approved_id, "approved", Some(eight_days_ago)).await;

        // Tick with a 7-day (604800 s) deadline.
        let reaped = super::SelfFixReviewReaperWorker::tick(&pool, 604800).await.unwrap();
        assert_eq!(reaped, 1, "exactly one stale in_review task should be reaped");

        let stale_status: String =
            sqlx::query_scalar("SELECT review_status FROM orchestration_tasks WHERE id = $1")
                .bind(stale_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stale_status, "changes_requested", "stale in_review must flip to changes_requested");

        let fresh_status: String =
            sqlx::query_scalar("SELECT review_status FROM orchestration_tasks WHERE id = $1")
                .bind(fresh_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(fresh_status, "in_review", "fresh in_review must remain in_review");

        let approved_status: String =
            sqlx::query_scalar("SELECT review_status FROM orchestration_tasks WHERE id = $1")
                .bind(approved_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(approved_status, "approved", "approved task must remain approved");
    }
}
