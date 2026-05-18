//! Context preview domain rules.
//!
//! Preview freshness is a business invariant: a publish request may only use
//! the exact task draft, workspace, agent capability, and resolved context that
//! the user previewed.

use agentforge_core::{AgentId, AppResult, ErrorKind};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::context_resolver::ResolvedContext;

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
