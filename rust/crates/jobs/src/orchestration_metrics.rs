use std::time::Duration;

use anyhow::Result;
use sqlx::PgPool;
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;

pub const DEFAULT_CONTROL_PLANE_METRICS_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq)]
pub struct OrchestrationControlPlaneSnapshot {
    pub assignment_outbox_backlog: i64,
    pub assignment_outbox_oldest_age_seconds: f64,
    pub stale_participants: i64,
    pub expired_working_leases: i64,
    pub busy_participants_without_work: i64,
    pub working_tasks_without_busy_participant: i64,
}

pub struct OrchestrationMetricsWorker {
    pool: PgPool,
    stale_after: Duration,
    interval: Duration,
}

impl OrchestrationMetricsWorker {
    pub fn new(pool: PgPool, stale_after: Duration) -> Self {
        Self { pool, stale_after, interval: DEFAULT_CONTROL_PLANE_METRICS_INTERVAL }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_ok() && *shutdown.borrow() {
                        tracing::info!("Orchestration metrics worker shutting down");
                        break;
                    }
                }
                _ = ticker.tick() => {
                    match collect_control_plane_snapshot(&self.pool, self.stale_after).await {
                        Ok(snapshot) => record_control_plane_snapshot(&snapshot),
                        Err(err) => {
                            metrics::counter!("agentforge_orchestration_control_plane_metrics_errors_total").increment(1);
                            tracing::warn!(error = %err, "orchestration control-plane metrics collection failed");
                        }
                    }
                }
            }
        }
    }
}

pub async fn collect_control_plane_snapshot(
    pool: &PgPool,
    stale_after: Duration,
) -> Result<OrchestrationControlPlaneSnapshot> {
    let stale_after_secs = stale_after.as_secs().min(i32::MAX as u64) as i32;

    let assignment_outbox_backlog: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM orchestration_outbox
           WHERE published_at IS NULL
             AND event_type = 'assignment'"#,
    )
    .fetch_one(pool)
    .await?;

    let assignment_outbox_oldest_age_seconds: f64 = sqlx::query_scalar(
        r#"SELECT COALESCE(
               CAST(EXTRACT(EPOCH FROM (NOW() - MIN(created_at))) AS DOUBLE PRECISION),
               0.0
           )
           FROM orchestration_outbox
           WHERE published_at IS NULL
             AND event_type = 'assignment'"#,
    )
    .fetch_one(pool)
    .await?;

    let stale_participants: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM participants
           WHERE status <> 'offline'
             AND (
                 last_heartbeat_at IS NULL
                 OR last_heartbeat_at < NOW() - ($1::int * INTERVAL '1 second')
             )"#,
    )
    .bind(stale_after_secs)
    .fetch_one(pool)
    .await?;

    let expired_working_leases: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM orchestration_tasks
           WHERE status = 'working'
             AND lease_expires_at IS NOT NULL
             AND lease_expires_at < NOW()"#,
    )
    .fetch_one(pool)
    .await?;

    let busy_participants_without_work: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM participants p
           WHERE p.status = 'busy'
             AND NOT EXISTS (
                 SELECT 1
                 FROM orchestration_tasks t
                 WHERE t.organization_id = p.organization_id
                   AND t.assigned_agent_id = p.agent_id
                   AND t.status = 'working'
             )"#,
    )
    .fetch_one(pool)
    .await?;

    let working_tasks_without_busy_participant: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM orchestration_tasks t
           WHERE t.status = 'working'
             AND NOT EXISTS (
                 SELECT 1
                 FROM participants p
                 WHERE p.organization_id = t.organization_id
                   AND p.agent_id = t.assigned_agent_id
                   AND p.status = 'busy'
             )"#,
    )
    .fetch_one(pool)
    .await?;

    Ok(OrchestrationControlPlaneSnapshot {
        assignment_outbox_backlog,
        assignment_outbox_oldest_age_seconds,
        stale_participants,
        expired_working_leases,
        busy_participants_without_work,
        working_tasks_without_busy_participant,
    })
}

pub fn record_control_plane_snapshot(snapshot: &OrchestrationControlPlaneSnapshot) {
    metrics::gauge!("agentforge_orchestration_outbox_backlog").set(snapshot.assignment_outbox_backlog as f64);
    metrics::gauge!("agentforge_orchestration_outbox_oldest_age_seconds")
        .set(snapshot.assignment_outbox_oldest_age_seconds);
    metrics::gauge!("agentforge_orchestration_stale_participants").set(snapshot.stale_participants as f64);
    metrics::gauge!("agentforge_orchestration_expired_working_leases").set(snapshot.expired_working_leases as f64);
    metrics::gauge!("agentforge_orchestration_busy_participants_without_work")
        .set(snapshot.busy_participants_without_work as f64);
    metrics::gauge!("agentforge_orchestration_working_tasks_without_busy_participant")
        .set(snapshot.working_tasks_without_busy_participant as f64);
}

pub fn register_metrics() {
    metrics::describe_gauge!(
        "agentforge_orchestration_outbox_backlog",
        "Unpublished orchestration assignment outbox rows"
    );
    metrics::describe_gauge!(
        "agentforge_orchestration_outbox_oldest_age_seconds",
        "Age in seconds of the oldest unpublished orchestration assignment outbox row"
    );
    metrics::describe_gauge!(
        "agentforge_orchestration_stale_participants",
        "Non-offline orchestration participants whose heartbeat is past the stale threshold"
    );
    metrics::describe_gauge!(
        "agentforge_orchestration_expired_working_leases",
        "Working orchestration tasks whose lease has expired"
    );
    metrics::describe_gauge!(
        "agentforge_orchestration_busy_participants_without_work",
        "Busy orchestration participants with no matching working task"
    );
    metrics::describe_gauge!(
        "agentforge_orchestration_working_tasks_without_busy_participant",
        "Working orchestration tasks whose assigned participant is not busy"
    );
    metrics::describe_counter!(
        "agentforge_orchestration_control_plane_metrics_errors_total",
        "Failures while collecting orchestration control-plane metrics"
    );
    metrics::describe_counter!(
        "agentforge_orchestration_working_lease_expired_total",
        "Working orchestration leases failed closed by the participant liveness sweeper"
    );

    record_control_plane_snapshot(&OrchestrationControlPlaneSnapshot {
        assignment_outbox_backlog: 0,
        assignment_outbox_oldest_age_seconds: 0.0,
        stale_participants: 0,
        expired_working_leases: 0,
        busy_participants_without_work: 0,
        working_tasks_without_busy_participant: 0,
    });
    metrics::counter!("agentforge_orchestration_control_plane_metrics_errors_total").increment(0);
    metrics::counter!("agentforge_orchestration_working_lease_expired_total").increment(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn seed_org_user_agent(pool: &PgPool, suffix: &str) -> (Uuid, Uuid, Uuid) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org_id)
            .bind(format!("Org {suffix}"))
            .bind(format!("org-{suffix}-{org_id}"))
            .execute(pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_id)
            .execute(pool)
            .await
            .expect("seed workspace");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("u-{suffix}-{user_id}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status) VALUES ($1, $2, $2, $3, $4, 'idle')",
        )
            .bind(agent_id)
            .bind(org_id)
            .bind(user_id)
            .bind(format!("agent-{suffix}"))
            .execute(pool)
            .await
            .expect("seed agent");

        (org_id, user_id, agent_id)
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn control_plane_snapshot_reports_release_gate_failure_modes(pool: PgPool) {
        let (org_id, user_id, busy_agent_id) = seed_org_user_agent(&pool, "busy").await;
        let stale_agent_id = Uuid::new_v4();
        let never_seen_agent_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status) VALUES ($1, $2, $2, $3, 'stale-agent', 'idle')",
        )
        .bind(stale_agent_id)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed stale agent");
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status) VALUES ($1, $2, $2, $3, 'never-seen-agent', 'idle')",
        )
        .bind(never_seen_agent_id)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed never-seen agent");

        sqlx::query(
            r#"INSERT INTO participants
               (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
               VALUES ($1, $2, 'busy-no-work', ARRAY['codex'], 'busy', NOW())"#,
        )
        .bind(org_id)
        .bind(busy_agent_id)
        .execute(&pool)
        .await
        .expect("seed busy participant");

        sqlx::query(
            r#"INSERT INTO participants
               (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
               VALUES ($1, $2, 'stale-with-work', ARRAY['codex'], 'available', NOW() - INTERVAL '10 minutes')"#,
        )
        .bind(org_id)
        .bind(stale_agent_id)
        .execute(&pool)
        .await
        .expect("seed stale participant");
        sqlx::query(
            r#"INSERT INTO participants
               (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
               VALUES ($1, $2, 'never-seen', ARRAY['codex'], 'available', NULL)"#,
        )
        .bind(org_id)
        .bind(never_seen_agent_id)
        .execute(&pool)
        .await
        .expect("seed null-heartbeat participant");

        let task_id = Uuid::new_v4();
        let orphan_task_id = Uuid::new_v4();
        let delivery_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, created_by, assigned_agent_id, priority,
                attempt, lease_expires_at, last_assignment_id, started_at)
               VALUES ($1, $2, 'expired work', 'working', $3, $4, 'normal',
                       1, NOW() - INTERVAL '1 minute', $5, NOW() - INTERVAL '20 minutes')"#,
        )
        .bind(task_id)
        .bind(org_id)
        .bind(user_id)
        .bind(stale_agent_id)
        .bind(delivery_id)
        .execute(&pool)
        .await
        .expect("seed working task");
        sqlx::query(
            r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, created_by, priority, started_at)
               VALUES ($1, $2, 'orphan work', 'working', $3, 'normal', NOW() - INTERVAL '20 minutes')"#,
        )
        .bind(orphan_task_id)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed null-assignee working task");

        sqlx::query(
            r#"INSERT INTO orchestration_outbox
               (id, organization_id, aggregate_type, aggregate_id, event_type, payload)
               VALUES ($1, $2, 'orchestration_task', $3, 'assignment', '{}'::jsonb)"#,
        )
        .bind(delivery_id)
        .bind(org_id)
        .bind(task_id)
        .execute(&pool)
        .await
        .expect("seed outbox");

        let snapshot = collect_control_plane_snapshot(&pool, Duration::from_secs(60)).await.unwrap();

        assert_eq!(snapshot.assignment_outbox_backlog, 1);
        assert!(snapshot.assignment_outbox_oldest_age_seconds >= 0.0);
        assert_eq!(snapshot.stale_participants, 2);
        assert_eq!(snapshot.expired_working_leases, 1);
        assert_eq!(snapshot.busy_participants_without_work, 1);
        assert_eq!(snapshot.working_tasks_without_busy_participant, 2);
    }
}
