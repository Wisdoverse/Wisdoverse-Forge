//! Governed context usage analytics response shape.

use agentforge_core::{AppError, AppResult, ErrorKind, TenantScope, WorkspaceId};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use super::observability::ContextUsageQueryBounds;

const DEFAULT_LIMIT: i64 = 10;
const DEFAULT_MIN_APPLIED: i64 = 10;
const DEFAULT_STALE_AFTER_DAYS: i64 = 30;
const DEFAULT_MIN_SUCCESS_RATE: f64 = 0.70;
const DEFAULT_NEGATIVE_RATE: f64 = 0.30;

pub(crate) struct ContextUsageAccessPolicy;

impl ContextUsageAccessPolicy {
    pub(crate) fn required_workspace(scope: &TenantScope) -> AppResult<WorkspaceId> {
        scope.workspace_id().ok_or_else(Self::forbidden)
    }

    fn forbidden() -> AppError {
        ErrorKind::Forbidden.into()
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_usage_access_policy_requires_workspace_scope() {
        let workspace_id = WorkspaceId::new();
        let scope = TenantScope::with_axes(
            agentforge_core::OrgId::new(),
            agentforge_core::UserId::new(),
            Some(workspace_id),
            None,
            None,
        );
        let missing_workspace = TenantScope::new(agentforge_core::OrgId::new(), agentforge_core::UserId::new());

        assert_eq!(ContextUsageAccessPolicy::required_workspace(&scope).unwrap(), workspace_id);
        assert!(matches!(
            ContextUsageAccessPolicy::required_workspace(&missing_workspace).unwrap_err().kind,
            ErrorKind::Forbidden
        ));
    }
}
