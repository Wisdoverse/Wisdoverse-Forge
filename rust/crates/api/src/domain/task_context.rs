//! Task detail Context tab response shape.

use agentforge_core::{AppError, AppResult, ErrorKind, SkillId, TenantScope, UserId, WorkspaceId};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::domain::context::AppliedContextSource;

pub(crate) struct TaskContextAccessPolicy;

impl TaskContextAccessPolicy {
    pub(crate) fn required_workspace(scope: &TenantScope) -> AppResult<WorkspaceId> {
        scope.workspace_id().ok_or_else(Self::forbidden)
    }

    fn forbidden() -> AppError {
        ErrorKind::Forbidden.into()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContextResponse {
    pub task_id: Uuid,
    pub runs: Vec<TaskContextRun>,
    pub applied_items: Vec<AppliedContextItem>,
    pub suggested_memory_updates: Vec<TaskContextCandidate>,
    pub skill_candidates: Vec<TaskContextCandidate>,
    pub evidence: Vec<TaskContextEvidence>,
    pub provenance: Vec<TaskContextProvenance>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContextRun {
    pub id: Uuid,
    pub status: String,
    pub agent_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub capability_profile: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedContextItem {
    pub injection_id: Uuid,
    pub run_id: Uuid,
    pub item_id: Uuid,
    pub item_kind: String,
    pub position: i32,
    pub title: String,
    pub content_preview: String,
    pub content_truncated: bool,
    pub content_ref: Option<String>,
    pub scope_kind: Option<String>,
    pub scope_id: Option<Uuid>,
    pub sensitivity: Option<String>,
    pub state: Option<String>,
    pub revoked: bool,
    pub source_task_id: Option<Uuid>,
    pub source_run_id: Option<Uuid>,
    pub source: Option<AppliedContextSource>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub applied_at: DateTime<Utc>,
    pub adapter: String,
    pub envelope_version: String,
    pub capability_profile: Value,
    pub degradation_reason: Option<String>,
    pub feedback: Option<AppliedContextFeedback>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedContextFeedback {
    pub label: String,
    pub note: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContextCandidate {
    pub id: Uuid,
    pub item_kind: String,
    pub state: String,
    pub owner_user_id: UserId,
    pub source_run_id: Option<Uuid>,
    pub target_skill_id: Option<SkillId>,
    pub proposed_preview: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContextEvidence {
    pub run_id: Option<Uuid>,
    pub source_type: String,
    pub source_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContextProvenance {
    pub run_id: Uuid,
    pub item_id: Uuid,
    pub item_kind: String,
    pub title: String,
    pub source: Option<AppliedContextSource>,
    pub adapter: String,
    pub envelope_version: String,
    pub applied_at: DateTime<Utc>,
    pub state: Option<String>,
    pub revoked: bool,
}

pub(crate) fn task_context_provenance(item: &AppliedContextItem) -> TaskContextProvenance {
    TaskContextProvenance {
        run_id: item.run_id,
        item_id: item.item_id,
        item_kind: item.item_kind.clone(),
        title: item.title.clone(),
        source: item.source.clone(),
        adapter: item.adapter.clone(),
        envelope_version: item.envelope_version.clone(),
        applied_at: item.applied_at,
        state: item.state.clone(),
        revoked: item.revoked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn applied_item_fixture() -> AppliedContextItem {
        let run_id = Uuid::from_u128(0x1111_1111_1111_1111_1111_1111_1111_1111);
        let item_id = Uuid::from_u128(0x2222_2222_2222_2222_2222_2222_2222_2222);
        let applied_at = DateTime::parse_from_rfc3339("2026-05-18T10:00:00Z").unwrap().with_timezone(&Utc);
        AppliedContextItem {
            injection_id: Uuid::nil(),
            run_id,
            item_id,
            item_kind: "memory".to_string(),
            position: 0,
            title: "Title".to_string(),
            content_preview: "preview".to_string(),
            content_truncated: false,
            content_ref: None,
            scope_kind: Some("team".to_string()),
            scope_id: None,
            sensitivity: Some("internal".to_string()),
            state: Some("active".to_string()),
            revoked: false,
            source_task_id: None,
            source_run_id: None,
            source: None,
            last_used_at: None,
            last_verified_at: None,
            applied_at,
            adapter: "v1".to_string(),
            envelope_version: "envelope-v1".to_string(),
            capability_profile: json!({}),
            degradation_reason: None,
            feedback: None,
        }
    }

    #[test]
    fn task_context_provenance_copies_applied_item_audit_fields() {
        let item = applied_item_fixture();
        let provenance = task_context_provenance(&item);

        assert_eq!(provenance.run_id, item.run_id);
        assert_eq!(provenance.item_id, item.item_id);
        assert_eq!(provenance.item_kind, "memory");
        assert_eq!(provenance.title, "Title");
        assert_eq!(provenance.adapter, "v1");
        assert_eq!(provenance.envelope_version, "envelope-v1");
        assert_eq!(provenance.applied_at, item.applied_at);
        assert_eq!(provenance.state.as_deref(), Some("active"));
        assert!(!provenance.revoked);
    }

    #[test]
    fn task_context_access_policy_requires_workspace_scope() {
        let workspace_id = WorkspaceId::new();
        let scope =
            TenantScope::with_axes(agentforge_core::OrgId::new(), UserId::new(), Some(workspace_id), None, None);
        let missing_workspace = crate::test_support::tenant_scope();

        assert_eq!(TaskContextAccessPolicy::required_workspace(&scope).unwrap(), workspace_id);
        assert!(matches!(
            TaskContextAccessPolicy::required_workspace(&missing_workspace).unwrap_err().kind,
            ErrorKind::Forbidden
        ));
    }
}
