//! Retention repository — governed purging of telemetry tables.

use agentforge_core::AppResult;
use sqlx::PgPool;

/// Retention policy purge (instance-wide, idempotent).
pub struct RetentionRepository {
    pool: PgPool,
}

impl RetentionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Deletes rows older than `days` from `events` and `analytics_events`.
    /// Returns `(events_removed, analytics_removed)`; a no-op for 0 days.
    pub async fn purge_telemetry(&self, days: i64) -> AppResult<(u64, u64)> {
        if days <= 0 {
            return Ok((0, 0));
        }
        let events = sqlx::query("DELETE FROM events WHERE created_at < NOW() - ($1 || ' days')::interval")
            .bind(days)
            .execute(&self.pool)
            .await?
            .rows_affected();
        let analytics =
            sqlx::query("DELETE FROM analytics_events WHERE created_at < NOW() - ($1 || ' days')::interval")
                .bind(days)
                .execute(&self.pool)
                .await?
                .rows_affected();
        Ok((events, analytics))
    }

    /// Deletes finished run attempts of terminal tasks older than `days`.
    /// Run-scoped context injections cascade; event/message/attachment links
    /// are nulled (records preserved). Returns rows removed.
    pub async fn purge_finished_runs(&self, days: i64) -> AppResult<u64> {
        if days <= 0 {
            return Ok(0);
        }
        let removed = sqlx::query(
            "DELETE FROM task_runs r
             USING orchestration_tasks t
              WHERE r.orchestration_task_id = t.id
                AND t.status IN ('completed', 'failed', 'canceled')
                AND t.updated_at < NOW() - ($1 || ' days')::interval",
        )
        .bind(days)
        .execute(&self.pool)
        .await?;
        Ok(removed.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "../db/migrations")]
    async fn purge_removes_only_expired_telemetry(pool: PgPool) {
        let org_id = uuid::Uuid::new_v4();
        let user_id = uuid::Uuid::new_v4();
        let agent_id = uuid::Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Ret Org', 'ret-org')")
            .bind(org_id)
            .execute(&pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, 'ret@example.com')")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_id)
            .execute(&pool)
            .await
            .expect("seed workspace");
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, user_id, status) VALUES ($1, $2, $2, $3, 'idle')",
        )
        .bind(agent_id)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed agent");
        sqlx::query("INSERT INTO events (id, organization_id, agent_id, event_type, created_at) VALUES ($1, $2, $3, 'old', NOW() - interval '30 days')")
            .bind(uuid::Uuid::new_v4())
            .bind(org_id)
            .bind(agent_id)
            .execute(&pool)
            .await
            .expect("seed old event");
        sqlx::query("INSERT INTO events (id, organization_id, agent_id, event_type, created_at) VALUES ($1, $2, $3, 'new', NOW())")
            .bind(uuid::Uuid::new_v4())
            .bind(org_id)
            .bind(agent_id)
            .execute(&pool)
            .await
            .expect("seed fresh event");
        sqlx::query(
            "INSERT INTO analytics_events (id, organization_id, user_id, event_name, properties, created_at) VALUES
            ($1, $2, $3, 'page_view', '{}'::jsonb, NOW() - interval '30 days')",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed old analytics");
        sqlx::query(
            "INSERT INTO analytics_events (id, organization_id, user_id, event_name, properties, created_at) VALUES
            ($1, $2, $3, 'page_view', '{}'::jsonb, NOW())",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed fresh analytics");

        let repo = RetentionRepository::new(pool.clone());
        let (events_removed, analytics_removed) = repo.purge_telemetry(7).await.expect("purge");
        assert_eq!(events_removed, 1, "only the 30-day-old event is purged");
        assert_eq!(analytics_removed, 1, "only the 30-day-old analytics event is purged");
        let events_left: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM events").fetch_one(&pool).await.expect("count events");
        let analytics_left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM analytics_events")
            .fetch_one(&pool)
            .await
            .expect("count analytics");
        assert_eq!(events_left, 1);
        assert_eq!(analytics_left, 1);
        // Off = no-op.
        let (e, a) = repo.purge_telemetry(0).await.expect("off");
        assert_eq!((e, a), (0, 0));

        // Run retention: remove attempts of terminal tasks only.
        let terminal = uuid::Uuid::new_v4();
        let working = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, title, status, priority, created_by, updated_at)
             VALUES ($1, $2, 'Terminal', 'completed', 'normal', $3, NOW() - interval '30 days')",
        )
        .bind(terminal)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed terminal task");
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, title, status, priority, created_by)
             VALUES ($1, $2, 'Working', 'working', 'normal', $3)",
        )
        .bind(working)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed working task");
        for (task, status) in [(terminal, "completed"), (working, "working")] {
            sqlx::query(
                "INSERT INTO task_runs (id, organization_id, workspace_id, orchestration_task_id, agent_id, idempotency_key, status)
                 VALUES ($1, $2, $2, $3, $4, $5, $6)",
            )
            .bind(uuid::Uuid::new_v4())
            .bind(org_id)
            .bind(task)
            .bind(agent_id)
            .bind(format!("idem-{task}"))
            .bind(status)
            .execute(&pool)
            .await
            .expect("seed run");
        }
        let removed_runs = repo.purge_finished_runs(7).await.expect("purge runs");
        assert_eq!(removed_runs, 1, "only the terminal task's run is removed");
        let runs_left: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM task_runs").fetch_one(&pool).await.expect("count runs");
        assert_eq!(runs_left, 1, "the working task's run is kept");
    }
}
