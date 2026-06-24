//! Shared review-verdict core (#841).
//!
//! Both the HTTP review handlers (`review/handler.rs`) and the orchestrator MCP
//! review tools (`mcp/mod.rs`) apply a review verdict (approve / reject) the same
//! way: fetch → state-transition guard → (self-approval guard) → atomic
//! `apply_verdict` → fail-closed audit. Keeping that logic in one async fn here
//! means the two entry points cannot diverge again (the HTTP path used to write
//! `changes_requested` + audit while the MCP stub wrote `rejected` with no audit).

use serde_json::json;

use crate::audit::{AuditAction, AuditLog, Store as AuditStore};
use crate::task::TaskState;

use super::errors::ReviewError;
use super::model::{ReviewComment, ReviewState, can_transition};
use super::store::Store;

/// Failure modes of [`apply_review_verdict`], independent of any transport.
///
/// The HTTP layer maps these to status codes; the MCP layer maps them to a
/// tool-error string. Audit failures are fail-closed (`Audit`) so neither entry
/// point can silently drop the audit record after the verdict commits.
#[derive(Debug)]
pub enum VerdictError {
    /// The review does not exist for the caller's org (also covers cross-tenant
    /// access — a caller in org B never sees org A's review).
    NotFound,
    /// The verdict is not a legal transition from the review's current state.
    IllegalTransition,
    /// The verdict is `Approved` and the actor is the review's creator.
    SelfApproval,
    /// The verdict or its task side-effect failed inside the store.
    Review(ReviewError),
    /// The verdict committed but the fail-closed audit write failed.
    ///
    /// The `String` carries the raw internal detail for server-side logging only.
    /// The transport mappers (`review/handler.rs`, `mcp/mod.rs`) MUST return a
    /// generic client message and never interpolate this string into a client or
    /// LLM-agent-facing response.
    Audit(String),
}

/// Apply a review verdict (approve / reject) and write its audit record.
///
/// This is the single shared verdict path for the HTTP and MCP entry points.
/// Behavior mirrors the original HTTP handler exactly:
///
/// 1. `get_by_id(review_id, org_id)` (org-scoped → cross-tenant access is `NotFound`).
/// 2. `can_transition(current, verdict)` guard (else `IllegalTransition`).
/// 3. Self-approval guard for `Approved` only: the creator cannot approve their own review.
/// 4. For a reject (`ChangesRequested`) the `feedback` comment is written inside the
///    verdict transaction (`apply_verdict`) so a rollback leaves no orphan comment.
/// 5. Fail-closed audit: `ReviewApprove` / `ReviewReject` with the caller's
///    `actor_id` + `actor_type`; for a reject the feedback is recorded in `changes`.
///
/// `actor_type` is a parameter (not hardcoded) so the audit actor is honest: the
/// HTTP path passes `"human"` (session user) and the MCP path passes `"human"`
/// (session JWT) or `"system"` (internal token).
#[allow(clippy::too_many_arguments)]
pub async fn apply_review_verdict(
    review_store: &dyn Store,
    audit_store: Option<&dyn AuditStore>,
    org_id: &str,
    actor_id: &str,
    actor_type: &str,
    review_id: &str,
    verdict: ReviewState,
    task_state: TaskState,
    feedback: Option<&str>,
) -> Result<ReviewState, VerdictError> {
    let review = match review_store.get_by_id(review_id, org_id).await {
        Ok(review) => review,
        Err(ReviewError::NotFound) => return Err(VerdictError::NotFound),
        Err(err) => return Err(VerdictError::Review(err)),
    };

    // State-machine guard: only legal transitions are allowed.
    if !can_transition(review.review.state, verdict) {
        return Err(VerdictError::IllegalTransition);
    }

    // Self-approval guard: the creator cannot approve their own review. Only the
    // approve path enforces this (the HTTP reject path never had a self-guard).
    if verdict == ReviewState::Approved && review.review.created_by == actor_id {
        return Err(VerdictError::SelfApproval);
    }

    // The feedback comment (reject path) is written inside the verdict transaction
    // so a rollback cannot leave an orphan comment on a still-pending review.
    let feedback_comment = feedback.map(|body| ReviewComment {
        id: String::new(),
        review_id: review_id.to_string(),
        author_id: actor_id.to_string(),
        body: body.to_string(),
        file_path: None,
        line: None,
        created_at: chrono::Utc::now(),
    });

    review_store
        .apply_verdict(review_id, org_id, verdict, &review.review.task_id, task_state, feedback_comment.as_ref())
        .await
        .map_err(VerdictError::Review)?;

    // Audit -- fail-closed: an audit write error is surfaced as `VerdictError::Audit`.
    // NOTE: the verdict (apply_verdict) has already committed by this point, so an
    // audit failure means the audit record failed, NOT the state change -- the review
    // IS in its new state. Audit is a separate write (not in the verdict tx) because
    // audit policy lives at the entry point, not in the store.
    //
    // The action mapping is exhaustive on purpose: only the two verdicts this path
    // is ever called with (`Approved` -> review.approve, `ChangesRequested` /
    // `Rejected` -> review.reject) may produce an audit row. Any other verdict state
    // is refused before the audit write so an unintended verdict can never produce a
    // misattributed audit row.
    let action = match verdict {
        ReviewState::Approved => AuditAction::ReviewApprove,
        ReviewState::ChangesRequested | ReviewState::Rejected => AuditAction::ReviewReject,
        other => {
            return Err(VerdictError::Review(ReviewError::InvalidInput(format!(
                "unsupported verdict state: {other:?}"
            ))));
        }
    };
    let changes = feedback.map(|body| json!({ "feedback": body }));
    record_verdict_audit(audit_store, action, review_id, org_id, actor_id, actor_type, changes).await?;

    Ok(verdict)
}

/// Emit the verdict audit record with a parameterized `actor_type`.
///
/// When no audit store is configured this is a no-op (parity with the HTTP
/// handler's previous `record_audit`). When a store is present, a write failure
/// is fail-closed (`VerdictError::Audit`).
async fn record_verdict_audit(
    audit_store: Option<&dyn AuditStore>,
    action: AuditAction,
    review_id: &str,
    org_id: &str,
    actor_id: &str,
    actor_type: &str,
    changes: Option<serde_json::Value>,
) -> Result<(), VerdictError> {
    let Some(audit_store) = audit_store else {
        return Ok(());
    };
    let mut log = AuditLog {
        id: String::new(),
        action,
        actor_id: actor_id.to_string(),
        actor_type: actor_type.to_string(),
        resource: "review".to_string(),
        resource_id: Some(review_id.to_string()),
        org_id: org_id.to_string(),
        changes,
        ip_address: None,
        user_agent: None,
        created_at: chrono::Utc::now(),
    };
    audit_store.create(&mut log).await.map_err(|err| {
        // The verdict has already committed; this audit write failed, so the audit
        // trail is now missing this verdict. For a governed workbench a
        // state-changed-but-no-audit gap must be reconstructable from the logs, so
        // emit the full context here BEFORE returning (the client gets only a
        // generic message -- the raw detail never leaves the server).
        tracing::error!(
            review_id = %review_id,
            org_id = %org_id,
            actor_id = %actor_id,
            actor_type = %actor_type,
            action = ?action,
            error = %err,
            "review verdict committed but audit write FAILED — audit trail is missing this verdict"
        );
        // Carry the raw detail in the variant for any downstream logging; the
        // transport mappers (HTTP / MCP) return a generic client string instead of
        // interpolating this so the internal error never reaches a client or LLM.
        VerdictError::Audit(format!("audit log failed: {err}"))
    })
}
