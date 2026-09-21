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
//! parent and explicit prerequisites are all `completed`, then flips them back
//! to `queued`. Every lookup is constrained to the dependent's organization.

use agentforge_db::entities::OrchestrationTask;
use async_nats::Client;
use sqlx::{PgPool, Postgres, Transaction};
use std::time::Duration;
use tokio::sync::watch;

/// Describe + prime dependency-reconcile metrics at zero so Prometheus scrape
/// returns the metric even before any event fires. `describe_*` sets help
/// text only; an explicit `increment(0)` primes the sample so dashboards
/// render from t=0 instead of "metric not found".
pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_dependency_reconcile_unblocked_total",
        "Orphan dependency blocks released by the reconcile backstop (rate>0 means the happy-path complete_task tx is failing)."
    );
    metrics::describe_counter!(
        "agentforge_dependency_reconcile_tick_errors_total",
        "Dependency-reconcile tick failures (DB errors during the reconcile pass)."
    );
    // Prime so the metric exists on /metrics before first event.
    metrics::counter!("agentforge_dependency_reconcile_unblocked_total").increment(0);
    metrics::counter!("agentforge_dependency_reconcile_tick_errors_total").increment(0);
}

/// Reconcile SQL — kept as a `pub(crate) const` so the query-shape unit
/// test pins the tenant and current-state predicates.
pub(crate) const RECONCILE_SQL: &str = r#"UPDATE orchestration_tasks AS child
        SET status = 'queued',
            blocked_reason = NULL,
            blocked_metadata = NULL,
            updated_at = NOW()
        WHERE child.status = 'blocked'
          AND child.blocked_reason = 'waiting_dependency'
          AND child.assigned_agent_id IS NULL
          AND child.requires_approval = FALSE
          AND (
                (
                  child.parent_task_id IS NOT NULL
                  AND EXISTS (
                        SELECT 1
                          FROM orchestration_tasks parent
                         WHERE parent.id = child.parent_task_id
                           AND parent.organization_id = child.organization_id
                           AND parent.status = 'completed'
                  )
                )
                OR (
                  jsonb_typeof(child.params->'dependency_ids') = 'array'
                  AND jsonb_array_length(child.params->'dependency_ids') > 0
                )
          )
          AND (
                child.parent_task_id IS NULL
                OR EXISTS (
                     SELECT 1
                       FROM orchestration_tasks parent
                      WHERE parent.id = child.parent_task_id
                        AND parent.organization_id = child.organization_id
                        AND parent.status = 'completed'
                )
          )
          AND NOT EXISTS (
                SELECT 1
                  FROM jsonb_array_elements_text(
                           CASE
                             WHEN jsonb_typeof(child.params->'dependency_ids') = 'array'
                               THEN child.params->'dependency_ids'
                             ELSE '[]'::jsonb
                           END
                       ) declared(id)
                  LEFT JOIN orchestration_tasks prerequisite
                    ON prerequisite.organization_id = child.organization_id
                   AND prerequisite.id::text = declared.id
                 WHERE prerequisite.status IS DISTINCT FROM 'completed'
          )
        RETURNING child.*"#;

/// Release dependents of one completed task inside the completion transaction.
/// Shared by HTTP and sidecar-result completion paths so neither can strand
/// ready work until the periodic reconciler runs.
pub const RELEASE_TASK_DEPENDENTS_SQL: &str = r#"UPDATE orchestration_tasks dependent
        SET status = 'queued',
            blocked_reason = NULL,
            blocked_metadata = NULL,
            updated_at = NOW()
        WHERE dependent.organization_id = $1
          AND (dependent.parent_task_id = $2 OR dependent.params->'dependency_ids' ? $2::text)
          AND dependent.status = 'blocked'
          AND dependent.blocked_reason = 'waiting_dependency'
          AND dependent.assigned_agent_id IS NULL
          AND dependent.requires_approval = FALSE
          AND (
                dependent.parent_task_id IS NULL
                OR EXISTS (
                     SELECT 1
                       FROM orchestration_tasks parent
                      WHERE parent.id = dependent.parent_task_id
                        AND parent.organization_id = dependent.organization_id
                        AND parent.status = 'completed'
                )
          )
          AND NOT EXISTS (
                SELECT 1
                  FROM jsonb_array_elements_text(
                           CASE
                             WHEN jsonb_typeof(dependent.params->'dependency_ids') = 'array'
                               THEN dependent.params->'dependency_ids'
                             ELSE '[]'::jsonb
                           END
                       ) declared(id)
                  LEFT JOIN orchestration_tasks prerequisite
                    ON prerequisite.organization_id = dependent.organization_id
                   AND prerequisite.id::text = declared.id
                 WHERE prerequisite.status IS DISTINCT FROM 'completed'
          )
        RETURNING dependent.*"#;

pub async fn release_task_dependents_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    organization_id: uuid::Uuid,
    completed_task_id: uuid::Uuid,
) -> sqlx::Result<Vec<OrchestrationTask>> {
    sqlx::query_as(RELEASE_TASK_DEPENDENTS_SQL).bind(organization_id).bind(completed_task_id).fetch_all(&mut **tx).await
}

/// Default reconcile cadence. Picked to be slow enough not to add measurable
/// load (<1 query/min per server) and fast enough that an orphaned child is
/// observed within a few minutes — within the typical operator alerting
/// reaction window.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(60);

pub struct DependencyReconcileWorker {
    pool: PgPool,
    interval: Duration,
    realtime_client: Option<Client>,
}

impl DependencyReconcileWorker {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, interval: DEFAULT_INTERVAL, realtime_client: None }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    pub fn with_realtime_client(mut self, client: Option<Client>) -> Self {
        self.realtime_client = client;
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
                    match Self::reconcile(&self.pool).await {
                        Ok(tasks) if tasks.is_empty() => {}
                        Ok(tasks) => {
                            let n = tasks.len() as u64;
                            tracing::warn!(
                                unblocked = n,
                                "dependency reconcile released orphan blocks — investigate why complete_task tx didn't"
                            );
                            metrics::counter!("agentforge_dependency_reconcile_unblocked_total").increment(n);
                            if let Some(client) = self.realtime_client.as_ref() {
                                for task in &tasks {
                                    if let Err(err) = crate::orchestration_realtime::publish_task_update(
                                        client,
                                        task,
                                        None,
                                        "task.dependencies_ready",
                                    )
                                    .await
                                    {
                                        tracing::warn!(error = %err, task_id = %task.id, "Failed to broadcast reconciled dependency");
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            tracing::error!(error = ?err, "dependency reconcile tick failed");
                            metrics::counter!("agentforge_dependency_reconcile_tick_errors_total").increment(1);
                        }
                    }
                }
            }
        }
        tracing::info!("dependency reconcile worker shut down");
    }

    /// Single-shot reconcile pass. Exposed for tests and one-off backfill.
    /// Returns the number of rows unblocked.
    ///
    pub async fn tick(pool: &PgPool) -> sqlx::Result<u64> {
        Ok(Self::reconcile(pool).await?.len() as u64)
    }

    async fn reconcile(pool: &PgPool) -> sqlx::Result<Vec<OrchestrationTask>> {
        sqlx::query_as(RECONCILE_SQL).fetch_all(pool).await
    }
}

#[cfg(test)]
mod tests {
    use super::{DependencyReconcileWorker, RECONCILE_SQL, register_metrics};
    use sqlx::PgPool;
    use uuid::Uuid;

    /// Verify that `register_metrics` can be called without panicking.
    /// Mirrors the prime pattern used in other workers: the call must succeed
    /// even when no metrics recorder is installed (the `metrics` crate no-ops
    /// when no recorder is registered).
    #[test]
    fn register_metrics_primes_series() {
        register_metrics();
    }

    // Pin the tenant-isolation and readiness predicates so a future
    // simplification cannot release cross-org or partially-ready tasks.
    #[test]
    fn reconcile_sql_pins_cross_tenant_join() {
        assert!(
            RECONCILE_SQL.contains("parent.organization_id = child.organization_id"),
            "reconcile must require parent + child to share organization_id"
        );
        assert!(
            RECONCILE_SQL.contains("parent.id = child.parent_task_id"),
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
        assert!(
            RECONCILE_SQL.contains("prerequisite.organization_id = child.organization_id"),
            "explicit prerequisite lookups must remain tenant-scoped"
        );
        assert!(
            RECONCILE_SQL.contains("prerequisite.status IS DISTINCT FROM 'completed'"),
            "reconcile must retain tasks with unfinished or missing prerequisites"
        );
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn reconcile_releases_only_fully_ready_explicit_dependencies(pool: PgPool) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let prerequisite_id = Uuid::new_v4();
        let ready_id = Uuid::new_v4();
        let unfinished_id = Uuid::new_v4();
        let approval_id = Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Reconcile', $2)")
            .bind(org_id)
            .bind(format!("reconcile-{org_id}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("reconcile-{user_id}@example.com"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, title, status, created_by) VALUES ($1, $2, 'done', 'completed', $3)",
        )
        .bind(prerequisite_id)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
        for (id, reason, requires_approval, dependencies) in [
            (ready_id, "waiting_dependency", false, serde_json::json!([prerequisite_id])),
            (unfinished_id, "waiting_dependency", false, serde_json::json!([prerequisite_id, Uuid::new_v4()])),
            (approval_id, "waiting_approval", true, serde_json::json!([prerequisite_id])),
        ] {
            sqlx::query(
                r#"INSERT INTO orchestration_tasks
                       (id, organization_id, title, status, blocked_reason, requires_approval, params, created_by)
                   VALUES ($1, $2, 'dependent', 'blocked', $3, $4,
                           jsonb_build_object('dependency_ids', $5::jsonb), $6)"#,
            )
            .bind(id)
            .bind(org_id)
            .bind(reason)
            .bind(requires_approval)
            .bind(dependencies)
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        }

        assert_eq!(DependencyReconcileWorker::tick(&pool).await.unwrap(), 1);
        let rows: Vec<(Uuid, String, Option<String>)> =
            sqlx::query_as("SELECT id, status, blocked_reason FROM orchestration_tasks WHERE id = ANY($1) ORDER BY id")
                .bind([ready_id, unfinished_id, approval_id])
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(rows.contains(&(ready_id, "queued".into(), None)));
        assert!(rows.contains(&(unfinished_id, "blocked".into(), Some("waiting_dependency".into()))));
        assert!(rows.contains(&(approval_id, "blocked".into(), Some("waiting_approval".into()))));
    }
}
