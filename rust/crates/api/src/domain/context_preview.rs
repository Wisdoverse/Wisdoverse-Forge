//! Context preview domain rules.
//!
//! Preview freshness is a business invariant: a publish request may only use
//! the exact task draft, workspace, agent capability, and resolved context that
//! the user previewed.

use agentforge_core::{AgentId, AppResult, ErrorKind};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use agentforge_db::entities::ContextPreview;

use super::context_resolver::{ResolvedContext, ResolvedItemRef};

pub(crate) const CONTEXT_PREVIEW_TTL_MINUTES: i64 = 15;

pub(crate) struct ContextPreviewTaskDraft<'a> {
    pub(crate) task_id: Uuid,
    pub(crate) title: &'a str,
    pub(crate) description: Option<&'a str>,
    pub(crate) params: Option<&'a Value>,
    pub(crate) priority: &'a str,
    pub(crate) group_id: Option<Uuid>,
    pub(crate) parent_task_id: Option<Uuid>,
}

impl ContextPreviewTaskDraft<'_> {
    pub(crate) fn hash(&self) -> String {
        let material = json!({
            "task_id": self.task_id,
            "title": self.title,
            "description": self.description,
            "params": self.params,
            "priority": self.priority,
            "group_id": self.group_id,
            "parent_task_id": self.parent_task_id,
        });
        hex::encode(Sha256::digest(material.to_string().as_bytes()))
    }
}

pub(crate) fn context_preview_hash(
    task_draft_hash: &str,
    agent_id: AgentId,
    resolved: &ResolvedContext,
) -> AppResult<String> {
    let material = json!({
        "task_draft_hash": task_draft_hash,
        "agent_id": agent_id.as_uuid(),
        "applied": resolved.applied,
        "suggested": resolved.suggested,
        "capability": resolved.capability,
        "degradation": resolved.degradation,
        "envelope_version": resolved.envelope_version,
    });
    serde_json::to_vec(&material)
        .map(|bytes| hex::encode(Sha256::digest(&bytes)))
        .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("serialize context preview hash: {err}")).into())
}

pub(crate) struct ContextPreviewFreshnessPolicy;

impl ContextPreviewFreshnessPolicy {
    pub(crate) fn ensure_request_hash_matches(
        stored_preview_hash: &str,
        requested_preview_hash: &str,
    ) -> AppResult<()> {
        if stored_preview_hash == requested_preview_hash { Ok(()) } else { Err(stale_preview_error().into()) }
    }

    pub(crate) fn ensure_workspace_matches(
        scope_workspace_id: Option<Uuid>,
        preview_workspace_id: Uuid,
    ) -> AppResult<()> {
        if scope_workspace_id == Some(preview_workspace_id) { Ok(()) } else { Err(stale_preview_error().into()) }
    }

    pub(crate) fn ensure_task_draft_matches(
        current_task_draft_hash: &str,
        preview_task_draft_hash: &str,
    ) -> AppResult<()> {
        if current_task_draft_hash == preview_task_draft_hash { Ok(()) } else { Err(stale_preview_error().into()) }
    }

    pub(crate) fn ensure_resolved_context_matches(
        current_preview_hash: &str,
        stored_preview_hash: &str,
    ) -> AppResult<()> {
        if current_preview_hash == stored_preview_hash { Ok(()) } else { Err(stale_preview_error().into()) }
    }
}

fn stale_preview_error() -> ErrorKind {
    ErrorKind::Conflict("preview_stale".into())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextPreviewItem {
    pub id: Uuid,
    pub item_kind: String,
    pub title: String,
    pub selected: bool,
    pub pinned: bool,
    pub scope_kind: Option<String>,
    pub scope_id: Option<Uuid>,
    pub sensitivity: Option<String>,
    pub estimated_tokens: u32,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub why: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextPreviewResponse {
    pub context_preview_id: Uuid,
    pub preview_hash: String,
    pub task_id: Uuid,
    pub agent_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub capability: Value,
    pub degradation: Vec<String>,
    pub items: Vec<ContextPreviewItem>,
    pub suggested_items: Vec<ContextPreviewItem>,
    pub previously_pinned: Vec<ContextPreviewItem>,
    pub warnings: Vec<String>,
}

pub(crate) fn context_preview_response(
    preview: &ContextPreview,
    resolved: ResolvedContext,
    warnings: Vec<String>,
) -> ContextPreviewResponse {
    let capability = serde_json::to_value(&resolved.capability).unwrap_or_else(|_| json!({}));
    let degradation = resolved.degradation.iter().map(|reason| reason.label().to_string()).collect();
    let items = resolved.applied.iter().map(|item| context_preview_item(item, true, false)).collect();
    let suggested_items = resolved.suggested.iter().map(|item| context_preview_item(item, false, false)).collect();
    ContextPreviewResponse {
        context_preview_id: preview.id,
        preview_hash: preview.preview_hash.clone(),
        task_id: preview.task_id,
        agent_id: preview.agent_id.as_uuid(),
        expires_at: preview.expires_at,
        capability,
        degradation,
        items,
        suggested_items,
        previously_pinned: Vec::new(),
        warnings,
    }
}

pub(crate) fn context_preview_item(item: &ResolvedItemRef, selected: bool, pinned: bool) -> ContextPreviewItem {
    ContextPreviewItem {
        id: item.id,
        item_kind: item.kind.label().to_string(),
        title: item.title.clone(),
        selected,
        pinned,
        scope_kind: item.scope_kind.clone(),
        scope_id: item.scope_id,
        sensitivity: item.sensitivity.clone(),
        estimated_tokens: item.estimated_tokens,
        last_used_at: item.last_used_at,
        last_verified_at: item.last_verified_at,
        why: item.why.clone(),
    }
}

pub(crate) fn selected_items_payload(resolved: &ResolvedContext) -> AppResult<Value> {
    serde_json::to_value(&resolved.applied)
        .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("serialize context preview selected items: {err}")).into())
}

#[cfg(test)]
mod tests {
    use agentforge_core::{CliToolKind, RuntimeCapability, RuntimeKind};
    use serde_json::json;

    use super::*;

    fn assert_preview_stale(result: AppResult<()>) {
        let err = result.expect_err("policy should reject stale preview");
        assert!(matches!(err.kind, ErrorKind::Conflict(message) if message == "preview_stale"));
    }

    #[test]
    fn freshness_policy_accepts_matching_preview_state() {
        let workspace_id = Uuid::new_v4();

        assert!(ContextPreviewFreshnessPolicy::ensure_request_hash_matches("preview", "preview").is_ok());
        assert!(ContextPreviewFreshnessPolicy::ensure_workspace_matches(Some(workspace_id), workspace_id).is_ok());
        assert!(ContextPreviewFreshnessPolicy::ensure_task_draft_matches("draft", "draft").is_ok());
        assert!(ContextPreviewFreshnessPolicy::ensure_resolved_context_matches("resolved", "resolved").is_ok());
    }

    #[test]
    fn freshness_policy_rejects_stale_preview_state() {
        let workspace_id = Uuid::new_v4();
        let other_workspace_id = Uuid::new_v4();

        assert_preview_stale(ContextPreviewFreshnessPolicy::ensure_request_hash_matches("preview", "old"));
        assert_preview_stale(ContextPreviewFreshnessPolicy::ensure_workspace_matches(None, workspace_id));
        assert_preview_stale(ContextPreviewFreshnessPolicy::ensure_workspace_matches(
            Some(other_workspace_id),
            workspace_id,
        ));
        assert_preview_stale(ContextPreviewFreshnessPolicy::ensure_task_draft_matches("draft", "old"));
        assert_preview_stale(ContextPreviewFreshnessPolicy::ensure_resolved_context_matches("resolved", "old"));
    }

    #[test]
    fn preview_ttl_is_stable_for_client_publish_window() {
        assert_eq!(CONTEXT_PREVIEW_TTL_MINUTES, 15);
    }

    #[test]
    fn task_draft_hash_owns_freshness_material() {
        let task_id = Uuid::from_u128(0x11111111111141118111111111111111);
        let params = json!({ "message": "use memory", "priority": "high" });
        let draft = ContextPreviewTaskDraft {
            task_id,
            title: "Run analysis",
            description: Some("Use current context"),
            params: Some(&params),
            priority: "high",
            group_id: Some(Uuid::from_u128(0x22222222222242228222222222222222)),
            parent_task_id: None,
        };
        let changed = ContextPreviewTaskDraft {
            task_id,
            title: "Run analysis",
            description: Some("Use changed context"),
            params: Some(&params),
            priority: "high",
            group_id: Some(Uuid::from_u128(0x22222222222242228222222222222222)),
            parent_task_id: None,
        };

        assert_eq!(draft.hash().len(), 64);
        assert_eq!(draft.hash(), draft.hash());
        assert_ne!(draft.hash(), changed.hash());
    }

    #[test]
    fn context_preview_response_serializes_resolved_context_with_degradation_labels() {
        use agentforge_core::{CliToolKind, OrgId, RuntimeCapability, RuntimeKind, UserId, WorkspaceId};
        use chrono::TimeZone;

        use crate::domain::context_resolver::{ContextItemKind, DegradationReason, ResolvedItemRef};

        let preview_id = Uuid::from_u128(0x66666666666666668666666666666666);
        let task_id = Uuid::from_u128(0x77777777777777778777777777777777);
        let agent_uuid = Uuid::from_u128(0x88888888888888888888888888888888);
        let expires_at = chrono::Utc.timestamp_millis_opt(1_700_000_900_000).unwrap();
        let created_at = chrono::Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();

        let preview = ContextPreview {
            id: preview_id,
            organization_id: OrgId::new(),
            workspace_id: WorkspaceId::new(),
            task_id,
            agent_id: AgentId::from(agent_uuid),
            created_by_user_id: UserId::new(),
            task_draft_hash: "draft".to_string(),
            preview_hash: "preview-hash".to_string(),
            selected_items: json!([]),
            removed_item_ids: Vec::new(),
            pinned_item_ids: Vec::new(),
            expires_at,
            created_at,
        };
        let applied = ResolvedItemRef {
            id: Uuid::from_u128(0x99999999999999999999999999999999),
            kind: ContextItemKind::Memory,
            title: "Applied".to_string(),
            scope_kind: None,
            scope_id: None,
            sensitivity: None,
            estimated_tokens: 5,
            last_used_at: None,
            last_verified_at: None,
            why: "matched".to_string(),
        };
        let suggested = ResolvedItemRef {
            id: Uuid::from_u128(0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA),
            kind: ContextItemKind::Skill,
            title: "Suggested".to_string(),
            scope_kind: None,
            scope_id: None,
            sensitivity: None,
            estimated_tokens: 3,
            last_used_at: None,
            last_verified_at: None,
            why: "matched".to_string(),
        };
        let resolved = ResolvedContext {
            applied: vec![applied],
            suggested: vec![suggested],
            capability: RuntimeCapability::for_cli_tool(CliToolKind::Codex, RuntimeKind::Container),
            degradation: vec![DegradationReason::BudgetTruncated, DegradationReason::RuntimeCapabilityFallback],
            envelope_version: "v1".to_string(),
        };

        let response = context_preview_response(&preview, resolved, vec!["resolver fallback".to_string()]);

        assert_eq!(response.context_preview_id, preview_id);
        assert_eq!(response.preview_hash, "preview-hash");
        assert_eq!(response.task_id, task_id);
        assert_eq!(response.agent_id, agent_uuid);
        assert_eq!(response.expires_at, expires_at);
        assert_eq!(response.degradation, vec!["budget_truncated", "runtime_capability_fallback"]);
        assert_eq!(response.items.len(), 1);
        assert!(response.items[0].selected);
        assert!(!response.items[0].pinned);
        assert_eq!(response.suggested_items.len(), 1);
        assert!(!response.suggested_items[0].selected);
        assert!(response.previously_pinned.is_empty());
        assert_eq!(response.warnings, vec!["resolver fallback"]);
    }

    #[test]
    fn context_preview_item_projects_resolved_item_with_selection_flags() {
        use crate::domain::context_resolver::{ContextItemKind, ResolvedItemRef};

        let id = Uuid::from_u128(0x44444444444444448444444444444444);
        let scope_id = Uuid::from_u128(0x55555555555555558555555555555555);
        let item = ResolvedItemRef {
            id,
            kind: ContextItemKind::Skill,
            title: "Review skill".to_string(),
            scope_kind: Some("project".to_string()),
            scope_id: Some(scope_id),
            sensitivity: Some("internal".to_string()),
            estimated_tokens: 42,
            last_used_at: None,
            last_verified_at: None,
            why: "matched".to_string(),
        };

        let selected = context_preview_item(&item, true, false);
        let suggested = context_preview_item(&item, false, true);

        assert_eq!(selected.id, id);
        assert_eq!(selected.item_kind, "skill");
        assert_eq!(selected.title, "Review skill");
        assert!(selected.selected);
        assert!(!selected.pinned);
        assert_eq!(selected.scope_kind.as_deref(), Some("project"));
        assert_eq!(selected.scope_id, Some(scope_id));
        assert_eq!(selected.sensitivity.as_deref(), Some("internal"));
        assert_eq!(selected.estimated_tokens, 42);
        assert_eq!(selected.why, "matched");

        assert!(!suggested.selected);
        assert!(suggested.pinned);
    }

    #[test]
    fn selected_items_payload_serializes_applied_items() {
        use crate::domain::context_resolver::ContextItemKind;

        let resolved = ResolvedContext {
            applied: vec![ResolvedItemRef {
                id: Uuid::from_u128(0x11111111111141118111111111111111),
                kind: ContextItemKind::Memory,
                title: "Applied".to_string(),
                scope_kind: None,
                scope_id: None,
                sensitivity: None,
                estimated_tokens: 5,
                last_used_at: None,
                last_verified_at: None,
                why: "matched".to_string(),
            }],
            suggested: Vec::new(),
            capability: RuntimeCapability::for_cli_tool(CliToolKind::Codex, RuntimeKind::Container),
            degradation: Vec::new(),
            envelope_version: "v1".to_string(),
        };

        let payload = selected_items_payload(&resolved).expect("selected items payload");

        assert_eq!(payload.as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn preview_hash_owns_resolved_context_material() {
        let agent_id = AgentId::from(Uuid::from_u128(0x33333333333343338333333333333333));
        let resolved = ResolvedContext {
            applied: Vec::new(),
            suggested: Vec::new(),
            capability: RuntimeCapability::for_cli_tool(CliToolKind::Codex, RuntimeKind::Container),
            degradation: Vec::new(),
            envelope_version: "v1".to_string(),
        };

        let hash = context_preview_hash("draft", agent_id, &resolved).unwrap();
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, context_preview_hash("draft", agent_id, &resolved).unwrap());
        assert_ne!(hash, context_preview_hash("changed", agent_id, &resolved).unwrap());
    }
}
