//! Governed context usage analytics read model.

use std::time::Instant;

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::domain::observability::ContextUsageQueryBounds;

const REFRESH_LOCK_CLASS: i32 = 72;
const REFRESH_LOCK_ID: i32 = 5101;
const STALE_AFTER_HOURS: i64 = 24;
const DEFAULT_LIMIT: i64 = 10;
const DEFAULT_MIN_APPLIED: i64 = 10;
const DEFAULT_STALE_AFTER_DAYS: i64 = 30;
const DEFAULT_MIN_SUCCESS_RATE: f64 = 0.70;
const DEFAULT_NEGATIVE_RATE: f64 = 0.30;

#[derive(Debug, Clone, Copy)]
pub struct ContextUsageQuery {
    pub limit: i64,
    pub min_applied: i64,
    pub stale_after_days: i64,
    pub min_success_rate: f64,
    pub negative_rate: f64,
}

impl Default for ContextUsageQuery {
    fn default() -> Self {
        Self {
            limit: DEFAULT_LIMIT,
            min_applied: DEFAULT_MIN_APPLIED,
            stale_after_days: DEFAULT_STALE_AFTER_DAYS,
            min_success_rate: DEFAULT_MIN_SUCCESS_RATE,
            negative_rate: DEFAULT_NEGATIVE_RATE,
        }
    }
}

impl ContextUsageQuery {
    pub fn normalized(self) -> Self {
        let query = ContextUsageQueryBounds::normalize(
            self.limit,
            self.min_applied,
            self.stale_after_days,
            self.min_success_rate,
            self.negative_rate,
        );

        Self {
            limit: query.limit(),
            min_applied: query.min_applied(),
            stale_after_days: query.stale_after_days(),
            min_success_rate: query.min_success_rate(),
            negative_rate: query.negative_rate(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageAnalyticsResponse {
    pub last_refreshed_at: DateTime<Utc>,
    pub last_refresh_started_at: Option<DateTime<Utc>>,
    pub last_refresh_error: Option<String>,
    pub stale_after_hours: i64,
    pub is_stale: bool,
    pub query: ContextUsageQuerySummary,
    pub summary: ContextUsageSummary,
    pub top_useful: Vec<ContextUsageItem>,
    pub stale_items: Vec<ContextUsageItem>,
    pub needs_review: Vec<ContextUsageItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageQuerySummary {
    pub limit: i64,
    pub min_applied: i64,
    pub stale_after_days: i64,
    pub min_success_rate: f64,
    pub negative_rate: f64,
}

impl From<ContextUsageQuery> for ContextUsageQuerySummary {
    fn from(query: ContextUsageQuery) -> Self {
        Self {
            limit: query.limit,
            min_applied: query.min_applied,
            stale_after_days: query.stale_after_days,
            min_success_rate: query.min_success_rate,
            negative_rate: query.negative_rate,
        }
    }
}

#[derive(Debug, Clone, Default, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageSummary {
    pub row_count: i64,
    pub distinct_items: i64,
    pub distinct_agents: i64,
    pub applied_count: i64,
    pub completed_count: i64,
    pub success_rate: f64,
    pub feedback_useful_count: i64,
    pub feedback_negative_count: i64,
}

#[derive(Debug, Clone, FromRow)]
struct ContextUsageRefreshRow {
    last_refreshed_at: DateTime<Utc>,
    last_refresh_started_at: Option<DateTime<Utc>>,
    last_refresh_error: Option<String>,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageItem {
    pub item_id: Uuid,
    pub item_kind: String,
    pub item_title: String,
    pub scope_kind: Option<String>,
    pub scope_id: Option<Uuid>,
    pub item_state: Option<String>,
    pub sensitivity: Option<String>,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub task_kind: String,
    pub runtime: String,
    pub agent_id: Uuid,
    pub agent_name: String,
    pub applied_count: i64,
    pub completed_count: i64,
    pub success_rate: f64,
    pub feedback_total_count: i64,
    pub feedback_useful_count: i64,
    pub feedback_negative_count: i64,
    pub negative_feedback_rate: f64,
    pub last_used_at: DateTime<Utc>,
    pub last_feedback_at: Option<DateTime<Utc>>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    Refreshed,
    SkippedLocked,
}

#[derive(Debug, Clone)]
pub struct UsageAnalyticsService {
    pool: PgPool,
}

impl UsageAnalyticsService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn context_usage(
        &self,
        scope: &TenantScope,
        query: ContextUsageQuery,
    ) -> AppResult<ContextUsageAnalyticsResponse> {
        let Some(workspace_id) = scope.workspace_id() else {
            return Err(ErrorKind::Forbidden.into());
        };
        let query = query.normalized();
        let refresh = self.refresh_status().await?;
        let summary = self.summary(scope, workspace_id.as_uuid()).await?;
        let top_useful = self.top_useful(scope, workspace_id.as_uuid(), query).await?;
        let stale_items = self.stale_items(scope, workspace_id.as_uuid(), query).await?;
        let needs_review = self.needs_review(scope, workspace_id.as_uuid(), query).await?;
        let is_stale = refresh.last_refresh_error.is_some()
            || Utc::now().signed_duration_since(refresh.last_refreshed_at).num_hours() > STALE_AFTER_HOURS;

        Ok(ContextUsageAnalyticsResponse {
            last_refreshed_at: refresh.last_refreshed_at,
            last_refresh_started_at: refresh.last_refresh_started_at,
            last_refresh_error: refresh.last_refresh_error,
            stale_after_hours: STALE_AFTER_HOURS,
            is_stale,
            query: query.into(),
            summary,
            top_useful,
            stale_items,
            needs_review,
        })
    }

    pub async fn refresh_context_usage_snapshot(&self) -> AppResult<RefreshOutcome> {
        let got_lock = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1, $2)")
            .bind(REFRESH_LOCK_CLASS)
            .bind(REFRESH_LOCK_ID)
            .fetch_one(&self.pool)
            .await?;
        if !got_lock {
            metrics::counter!("context_usage_analytics_refresh_total", "outcome" => "skipped_locked").increment(1);
            return Ok(RefreshOutcome::SkippedLocked);
        }

        let started = Instant::now();
        let result = self.refresh_context_usage_snapshot_locked().await;
        let unlock_result = sqlx::query("SELECT pg_advisory_unlock($1, $2)")
            .bind(REFRESH_LOCK_CLASS)
            .bind(REFRESH_LOCK_ID)
            .execute(&self.pool)
            .await;
        if let Err(err) = unlock_result {
            tracing::warn!(error = %err, "failed to unlock context usage analytics refresh advisory lock");
        }

        match result {
            Ok(()) => {
                metrics::histogram!("context_usage_analytics_refresh_seconds").record(started.elapsed().as_secs_f64());
                metrics::counter!("context_usage_analytics_refresh_total", "outcome" => "success").increment(1);
                Ok(RefreshOutcome::Refreshed)
            }
            Err(err) => {
                metrics::counter!("context_usage_analytics_refresh_total", "outcome" => "error").increment(1);
                Err(err)
            }
        }
    }

    async fn refresh_context_usage_snapshot_locked(&self) -> AppResult<()> {
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
            return Err(ErrorKind::Internal(anyhow::anyhow!("refresh context usage analytics: {err}")).into());
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

    async fn refresh_status(&self) -> AppResult<ContextUsageRefreshRow> {
        let row = sqlx::query_as::<_, ContextUsageRefreshRow>(
            r#"SELECT last_refreshed_at, last_refresh_started_at, last_refresh_error
                 FROM context_usage_analytics_refreshes
                WHERE name = 'context_usage_analytics'"#,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    async fn summary(&self, scope: &TenantScope, workspace_id: Uuid) -> AppResult<ContextUsageSummary> {
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
        .bind(workspace_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    async fn top_useful(
        &self,
        scope: &TenantScope,
        workspace_id: Uuid,
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

    async fn stale_items(
        &self,
        scope: &TenantScope,
        workspace_id: Uuid,
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

    async fn needs_review(
        &self,
        scope: &TenantScope,
        workspace_id: Uuid,
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
        workspace_id: Uuid,
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
            .bind(workspace_id)
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

pub fn register_usage_analytics_metrics() {
    metrics::describe_counter!(
        "context_usage_analytics_refresh_total",
        "Count of context usage analytics materialized view refresh outcomes."
    );
    metrics::describe_histogram!(
        "context_usage_analytics_refresh_seconds",
        "Duration of successful context usage analytics materialized view refreshes."
    );
    metrics::counter!("context_usage_analytics_refresh_total", "outcome" => "success").increment(0);
    metrics::counter!("context_usage_analytics_refresh_total", "outcome" => "error").increment(0);
    metrics::counter!("context_usage_analytics_refresh_total", "outcome" => "skipped_locked").increment(0);
}
