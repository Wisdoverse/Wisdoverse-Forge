//! Analytics repository — database queries for the analytics_events table.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::AnalyticsEvent;
use sqlx::PgPool;

/// Database access layer for analytics events.
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
    pub async fn summary(&self, scope: &TenantScope) -> AppResult<serde_json::Value> {
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

        let top: Vec<serde_json::Value> = top_events
            .into_iter()
            .map(|(name, count)| serde_json::json!({"event_name": name, "count": count}))
            .collect();

        Ok(serde_json::json!({
            "total_events": total_events,
            "unique_users": unique_users,
            "top_events": top,
        }))
    }
}
