//! Governed context usage analytics response shape.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

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
