//! Context preview domain rules.
//!
//! Preview freshness is a business invariant: a publish request may only use
//! the exact task draft, workspace, agent capability, and resolved context that
//! the user previewed.

use agentforge_core::{AppResult, ErrorKind};
use uuid::Uuid;

pub(crate) const CONTEXT_PREVIEW_TTL_MINUTES: i64 = 15;

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
}
