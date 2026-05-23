//! Context usage analytics read-model repository.

use agentforge_core::{AppResult, TenantScope, WorkspaceId};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::domain::usage_analytics::{
    ContextUsageItem, ContextUsageQuery, ContextUsageRepositoryPolicy, ContextUsageSummary,
};

const REFRESH_LOCK_CLASS: i32 = 72;
const REFRESH_LOCK_ID: i32 = 5101;

#[derive(Debug, Clone, FromRow)]
pub struct ContextUsageRefreshRow {
    pub last_refreshed_at: DateTime<Utc>,
    pub last_refresh_started_at: Option<DateTime<Utc>>,
    pub last_refresh_error: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct ContextUsageItemRow {
    item_id: Uuid,
    item_kind: String,
    item_title: String,
    scope_kind: Option<String>,
    scope_id: Option<Uuid>,
    item_state: Option<String>,
    sensitivity: Option<String>,
    last_verified_at: Option<DateTime<Utc>>,
    task_kind: String,
    runtime: String,
    agent_id: Uuid,
    agent_name: String,
    applied_count: i64,
    completed_count: i64,
    success_rate: f64,
    feedback_total_count: i64,
    feedback_useful_count: i64,
    feedback_negative_count: i64,
    negative_feedback_rate: f64,
    last_used_at: DateTime<Utc>,
    last_feedback_at: Option<DateTime<Utc>>,
}

impl From<ContextUsageItemRow> for ContextUsageItem {
    fn from(row: ContextUsageItemRow) -> Self {
        Self {
            item_id: row.item_id,
            item_kind: row.item_kind,
            item_title: row.item_title,
            scope_kind: row.scope_kind,
            scope_id: row.scope_id,
            item_state: row.item_state,
            sensitivity: row.sensitivity,
            last_verified_at: row.last_verified_at,
            task_kind: row.task_kind,
            runtime: row.runtime,
            agent_id: row.agent_id,
            agent_name: row.agent_name,
            applied_count: row.applied_count,
            completed_count: row.completed_count,
            success_rate: row.success_rate,
            feedback_total_count: row.feedback_total_count,
            feedback_useful_count: row.feedback_useful_count,
            feedback_negative_count: row.feedback_negative_count,
            negative_feedback_rate: row.negative_feedback_rate,
            last_used_at: row.last_used_at,
            last_feedback_at: row.last_feedback_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UsageAnalyticsRepository {
    pool: PgPool,
}

impl UsageAnalyticsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn try_acquire_refresh_lock(&self) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1, $2)")
            .bind(REFRESH_LOCK_CLASS)
            .bind(REFRESH_LOCK_ID)
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn release_refresh_lock(&self) -> AppResult<()> {
        sqlx::query("SELECT pg_advisory_unlock($1, $2)")
            .bind(REFRESH_LOCK_CLASS)
            .bind(REFRESH_LOCK_ID)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn refresh_snapshot(&self) -> AppResult<()> {
        sqlx::query(
            r#"UPDATE context_usage_analytics_refreshes
                  SET last_refresh_started_at = now(),
                      updated_at = now()
                WHERE name = 'context_usage_analytics'"#,
        )
        .execute(&self.pool)
        .await?;

        if let Err(err) =
            sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY context_usage_analytics").execute(&self.pool).await
        {
            let message = err.to_string();
            let _ = sqlx::query(
                r#"UPDATE context_usage_analytics_refreshes
                      SET last_refresh_error = $1,
                          updated_at = now()
                    WHERE name = 'context_usage_analytics'"#,
            )
            .bind(message)
            .execute(&self.pool)
            .await;
            return Err(ContextUsageRepositoryPolicy::refresh_failed(err));
        }

        sqlx::query(
            r#"UPDATE context_usage_analytics_refreshes
                  SET last_refreshed_at = now(),
                      last_refresh_error = NULL,
                      updated_at = now()
                WHERE name = 'context_usage_analytics'"#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn refresh_status(&self) -> AppResult<ContextUsageRefreshRow> {
        let row = sqlx::query_as::<_, ContextUsageRefreshRow>(
            r#"SELECT last_refreshed_at, last_refresh_started_at, last_refresh_error
                 FROM context_usage_analytics_refreshes
                WHERE name = 'context_usage_analytics'"#,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn summary(&self, scope: &TenantScope, workspace_id: WorkspaceId) -> AppResult<ContextUsageSummary> {
        let row = sqlx::query_as::<_, ContextUsageSummary>(
            r#"SELECT
                    COUNT(*)::bigint AS row_count,
                    COUNT(DISTINCT item_kind || ':' || item_id::text)::bigint AS distinct_items,
                    COUNT(DISTINCT agent_id)::bigint AS distinct_agents,
                    COALESCE(SUM(applied_count), 0)::bigint AS applied_count,
                    COALESCE(SUM(completed_count), 0)::bigint AS completed_count,
                    CASE
                        WHEN COALESCE(SUM(applied_count), 0) = 0 THEN 0::double precision
                        ELSE COALESCE(SUM(completed_count), 0)::double precision
                            / COALESCE(SUM(applied_count), 0)::double precision
                    END AS success_rate,
                    COALESCE(SUM(feedback_useful_count), 0)::bigint AS feedback_useful_count,
                    COALESCE(SUM(feedback_negative_count), 0)::bigint AS feedback_negative_count
               FROM context_usage_analytics
              WHERE organization_id = $1
                AND workspace_id = $2"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn top_useful(
        &self,
        scope: &TenantScope,
        workspace_id: WorkspaceId,
        query: ContextUsageQuery,
    ) -> AppResult<Vec<ContextUsageItem>> {
        self.items(
            scope,
            workspace_id,
            r#"applied_count >= $3
               AND success_rate >= $4"#,
            query,
            "success_rate DESC, feedback_useful_count DESC, applied_count DESC, last_used_at DESC",
        )
        .await
    }

    pub async fn stale_items(
        &self,
        scope: &TenantScope,
        workspace_id: WorkspaceId,
        query: ContextUsageQuery,
    ) -> AppResult<Vec<ContextUsageItem>> {
        self.items(
            scope,
            workspace_id,
            r#"last_used_at < now() - ($5::text || ' days')::interval"#,
            query,
            "last_used_at ASC, applied_count DESC",
        )
        .await
    }

    pub async fn needs_review(
        &self,
        scope: &TenantScope,
        workspace_id: WorkspaceId,
        query: ContextUsageQuery,
    ) -> AppResult<Vec<ContextUsageItem>> {
        self.items(
            scope,
            workspace_id,
            r#"feedback_negative_count > 0
               AND negative_feedback_rate >= $6"#,
            query,
            "negative_feedback_rate DESC, feedback_negative_count DESC, last_feedback_at DESC NULLS LAST",
        )
        .await
    }

    async fn items(
        &self,
        scope: &TenantScope,
        workspace_id: WorkspaceId,
        predicate: &str,
        query: ContextUsageQuery,
        order_by: &str,
    ) -> AppResult<Vec<ContextUsageItem>> {
        let sql = format!(
            r#"SELECT
                    item_id,
                    item_kind,
                    item_title,
                    scope_kind,
                    scope_id,
                    item_state,
                    sensitivity,
                    last_verified_at,
                    task_kind,
                    runtime,
                    agent_id,
                    agent_name,
                    applied_count,
                    completed_count,
                    success_rate,
                    feedback_total_count,
                    feedback_useful_count,
                    feedback_negative_count,
                    negative_feedback_rate,
                    last_used_at,
                    last_feedback_at
              FROM context_usage_analytics
              WHERE organization_id = $1
                AND workspace_id = $2
                AND $3::bigint IS NOT NULL
                AND $4::double precision IS NOT NULL
                AND $5::bigint IS NOT NULL
                AND $6::double precision IS NOT NULL
                AND ({predicate})
              ORDER BY {order_by}
              LIMIT $7"#
        );
        let rows = sqlx::query_as::<_, ContextUsageItemRow>(&sql)
            .bind(scope.org_id().as_uuid())
            .bind(workspace_id.as_uuid())
            .bind(query.min_applied)
            .bind(query.min_success_rate)
            .bind(query.stale_after_days)
            .bind(query.negative_rate)
            .bind(query.limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}
