//! Analytics repository — database queries for the analytics_events table.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::AnalyticsEvent;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::observability::{AnalyticsSummary, AnalyticsTopEvent};

/// Database access layer for analytics events.
/// One (agent, model) usage group: assistant replies and their tokens.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct AgentUsageRow {
    pub(crate) agent_id: Uuid,
    pub(crate) name: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) requests: i64,
    pub(crate) tokens_in: i64,
    pub(crate) tokens_out: i64,
}

/// Database access layer for analytics.
pub struct AnalyticsRepository {
    pool: PgPool,
}

impl AnalyticsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Track (insert) a new analytics event.
    pub async fn track(
        &self,
        scope: &TenantScope,
        event_name: &str,
        properties: &serde_json::Value,
    ) -> AppResult<AnalyticsEvent> {
        sqlx::query_as::<_, AnalyticsEvent>(
            r#"INSERT INTO analytics_events (organization_id, user_id, event_name, properties)
               VALUES ($1, $2, $3, $4)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(event_name)
        .bind(properties)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// List analytics events with optional filters.
    pub async fn list(
        &self,
        scope: &TenantScope,
        event_name: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<AnalyticsEvent>> {
        let events = sqlx::query_as::<_, AnalyticsEvent>(
            r#"SELECT * FROM analytics_events
               WHERE organization_id = $1
                 AND ($2::TEXT IS NULL OR event_name = $2)
               ORDER BY created_at DESC
               LIMIT $3 OFFSET $4"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(event_name)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(events)
    }

    /// Get aggregate summary stats.
    pub(crate) async fn summary(&self, scope: &TenantScope) -> AppResult<AnalyticsSummary> {
        let row = sqlx::query_as::<_, (i64,)>(r#"SELECT COUNT(*) FROM analytics_events WHERE organization_id = $1"#)
            .bind(scope.org_id().as_uuid())
            .fetch_one(&self.pool)
            .await?;
        let total_events = row.0;

        let unique_users = sqlx::query_as::<_, (i64,)>(
            r#"SELECT COUNT(DISTINCT user_id) FROM analytics_events WHERE organization_id = $1"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_one(&self.pool)
        .await?
        .0;

        // Top 10 event names by count
        let top_events: Vec<(String, i64)> = sqlx::query_as(
            r#"SELECT event_name, COUNT(*) as cnt
               FROM analytics_events
               WHERE organization_id = $1
               GROUP BY event_name
               ORDER BY cnt DESC
               LIMIT 10"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;

        let top_events =
            top_events.into_iter().map(|(event_name, count)| AnalyticsTopEvent { event_name, count }).collect();

        Ok(AnalyticsSummary { total_events, unique_users, top_events })
    }

    /// Per-agent finished-run counts (`completed` + `failed`) over the rolling
    /// window. One row per agent, newest-finish last updated, tenant-scoped.
    pub(crate) async fn agent_reliability_rows(
        &self,
        scope: &TenantScope,
        hours: i64,
    ) -> AppResult<Vec<(Uuid, Option<String>, i64, i64)>> {
        let rows: Vec<(Uuid, Option<String>, i64, i64)> = sqlx::query_as(
            r#"SELECT a.id, a.name,
                      COUNT(*)::bigint AS total,
                      COUNT(*) FILTER (WHERE t.status = 'completed')::bigint AS succeeded
               FROM orchestration_tasks t
               JOIN agents a ON a.id = t.assigned_agent_id
               WHERE t.organization_id = $1
                 AND t.status IN ('completed', 'failed')
                 AND t.updated_at >= NOW() - ($2 || ' hours')::interval
               GROUP BY a.id, a.name
               ORDER BY total DESC, a.name ASC NULLS LAST
               LIMIT 20"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(hours)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Per-agent, per-model assistant-message usage over the rolling window,
    /// most input-heavy first. Assistant rows without token usage (or old)
    /// are excluded; other orgs never leak in. Model is kept so the service
    /// can apply per-model `LLM_PRICING` rates.
    pub(crate) async fn agent_usage_rows(&self, scope: &TenantScope, hours: i64) -> AppResult<Vec<AgentUsageRow>> {
        let rows = sqlx::query_as::<_, AgentUsageRow>(
            r#"SELECT a.id AS agent_id, a.name, m.model,
                      COUNT(*)::bigint AS requests,
                      COALESCE(SUM(m.tokens_in), 0)::bigint AS tokens_in,
                      COALESCE(SUM(m.tokens_out), 0)::bigint AS tokens_out
               FROM agent_messages m
               JOIN agents a ON a.id = m.agent_id
               WHERE m.organization_id = $1
                 AND m.role = 'assistant'
                 AND m.tokens_in IS NOT NULL
                 AND m.created_at >= NOW() - ($2 || ' hours')::interval
               GROUP BY a.id, a.name, m.model
               ORDER BY tokens_in DESC, a.name ASC NULLS LAST
               LIMIT 10"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(hours)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tenant_scope_for_ids;

    #[sqlx::test(migrations = "../db/migrations")]
    async fn agent_reliability_rows_are_windowed_and_tenant_scoped(pool: sqlx::PgPool) {
        let org_id = Uuid::new_v4();
        let other_org = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let other_user = Uuid::new_v4();
        let reliable = Uuid::new_v4();
        let flaky = Uuid::new_v4();
        let other_agent = Uuid::new_v4();

        for (org, user, name) in [(org_id, user_id, "Rel"), (other_org, other_user, "Other")] {
            sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
                .bind(org)
                .bind(format!("{name} Org"))
                .bind(format!("{name}-org"))
                .execute(&pool)
                .await
                .expect("seed org");
            sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
                .bind(user)
                .bind(format!("{name}@example.com"))
                .execute(&pool)
                .await
                .expect("seed user");
            sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
                .bind(org)
                .execute(&pool)
                .await
                .expect("seed workspace");
        }

        for (org, agent, name) in
            [(org_id, reliable, "Reliable"), (org_id, flaky, "Flaky"), (other_org, other_agent, "Other")]
        {
            sqlx::query(
                "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status)
                 VALUES ($1, $2, $2, $3, $4, 'idle')",
            )
            .bind(agent)
            .bind(org)
            .bind(user_id)
            .bind(name)
            .execute(&pool)
            .await
            .expect("seed agent");
        }

        for (status, age_hours, org, agent) in [
            ("completed", 1, org_id, reliable),
            ("completed", 2, org_id, reliable),
            ("completed", 3, org_id, reliable),
            ("completed", 4, org_id, reliable),
            ("failed", 5, org_id, reliable),
            ("completed", 960, org_id, reliable),
            ("queued", 1, org_id, reliable),
            ("failed", 1, org_id, flaky),
            ("failed", 2, org_id, flaky),
            ("completed", 3, org_id, flaky),
            ("completed", 1, other_org, other_agent),
        ] {
            sqlx::query(
                "INSERT INTO orchestration_tasks (id, organization_id, title, status, priority,
                 created_by, assigned_agent_id, progress, created_at, updated_at)
                 VALUES ($1, $2, 'T', $3, 'normal', $4, $5, 0,
                         NOW() - ($6 || ' hours')::interval, NOW() - ($6 || ' hours')::interval)",
            )
            .bind(Uuid::new_v4())
            .bind(org)
            .bind(status)
            .bind(user_id)
            .bind(agent)
            .bind(age_hours)
            .execute(&pool)
            .await
            .expect("seed task");
        }

        let repo = AnalyticsRepository::new(pool.clone());
        let scope = tenant_scope_for_ids(org_id, user_id);
        let rows = repo.agent_reliability_rows(&scope, 720).await.expect("reliability rows");
        let row = |name: &str| {
            rows.iter()
                .find(|(_, n, _, _)| n.as_deref() == Some(name))
                .unwrap_or_else(|| panic!("missing agent {name}: {rows:?}"))
        };
        let (_, _, total, succeeded) = row("Reliable");
        assert_eq!(*total, 5, "old run excluded, queued excluded");
        assert_eq!(*succeeded, 4);
        let (_, _, total, succeeded) = row("Flaky");
        assert_eq!(*total, 3);
        assert_eq!(*succeeded, 1);
        assert!(rows.iter().all(|(_, n, _, _)| n.as_deref() != Some("Other")), "other-org agents are excluded");

        let rows_wide = repo.agent_reliability_rows(&scope, 8_760).await.expect("wide rows");
        let (_, _, total, _) =
            rows_wide.iter().find(|(_, n, _, _)| n.as_deref() == Some("Reliable")).expect("reliable in wide window");
        assert_eq!(*total, 6, "the 40-day-old finished run counts inside a 1-year window");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn agent_usage_rows_are_windowed_and_tenant_scoped(pool: sqlx::PgPool) {
        let org_id = Uuid::new_v4();
        let other_org = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let other_user = Uuid::new_v4();
        let agent = Uuid::new_v4();
        let other_agent = Uuid::new_v4();

        for (org, user, slug) in [(org_id, user_id, "usage-org"), (other_org, other_user, "usage-other")] {
            sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Usage Org', $2)")
                .bind(org)
                .bind(slug)
                .execute(&pool)
                .await
                .expect("seed org");
            sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
                .bind(user)
                .bind(format!("usage-{slug}@example.com"))
                .execute(&pool)
                .await
                .expect("seed user");
            sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
                .bind(org)
                .execute(&pool)
                .await
                .expect("seed workspace");
        }

        for (org, agent_id) in [(org_id, agent), (other_org, other_agent)] {
            sqlx::query("INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status) VALUES ($1, $2, $2, $3, 'Worker', 'idle')")
                .bind(agent_id)
                .bind(org)
                .bind(user_id)
                .execute(&pool)
                .await
                .expect("seed agent");
        }

        for (role, tokens_in, tokens_out, age_hours, org, agent_id) in [
            ("assistant", 100_i32, 20_i32, 1, org_id, agent),
            ("assistant", 150_i32, 30_i32, 2, org_id, agent),
            ("assistant", 999_i32, 999_i32, 1, org_id, agent),
            ("assistant", 0_i32, 0_i32, 1, other_org, other_agent),
            ("user", 50_i32, 0_i32, 1, org_id, agent),
        ] {
            let tokens_in: Option<i32> = if tokens_in == 999 { None } else { Some(tokens_in) };
            sqlx::query(
                "INSERT INTO agent_messages (id, organization_id, agent_id, role, content,
                 tokens_in, tokens_out, created_at)
                 VALUES ($1, $2, $3, $4, 'hello', $5, $6,
                         NOW() - ($7 || ' hours')::interval)",
            )
            .bind(Uuid::new_v4())
            .bind(org)
            .bind(agent_id)
            .bind(role)
            .bind(tokens_in)
            .bind(tokens_out)
            .bind(age_hours)
            .execute(&pool)
            .await
            .expect("seed message");
        }
        // Old row inside the 1-year window only.
        sqlx::query(
            "INSERT INTO agent_messages (id, organization_id, agent_id, role, content,
             tokens_in, tokens_out, created_at)
             VALUES ($1, $2, $3, 'assistant', 'hello', 200, 40, NOW() - interval '40 days')",
        )
        .bind(Uuid::new_v4())
        .bind(org_id)
        .bind(agent)
        .execute(&pool)
        .await
        .expect("seed old message");

        let repo = AnalyticsRepository::new(pool.clone());
        let scope = tenant_scope_for_ids(org_id, user_id);
        let rows = repo.agent_usage_rows(&scope, 720).await.expect("usage rows");
        let worker = rows.iter().find(|row| row.name.as_deref() == Some("Worker")).expect("worker row");
        assert_eq!(worker.requests, 2, "null-token assistant and user rows are excluded");
        assert_eq!(worker.tokens_in, 250);
        assert_eq!(worker.tokens_out, 50);
        assert_eq!(rows.len(), 1, "other-org agents are excluded");

        let rows_wide = repo.agent_usage_rows(&scope, 8_760).await.expect("wide rows");
        let worker = rows_wide.iter().find(|row| row.name.as_deref() == Some("Worker")).expect("worker wide");
        assert_eq!(worker.requests, 3, "the 40-day-old message counts inside a 1-year window");
        assert_eq!(worker.tokens_in, 450);
    }
}
