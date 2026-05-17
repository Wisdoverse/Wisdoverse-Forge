//! Context candidate and feedback input policies.

use agentforge_core::{AppError, AppResult, ErrorKind, ScopeKind, SkillId, WorkspaceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::context_governance::{
    ContextGovernancePolicy, ContextScopeKind, ScopeExpansionRejection, ScopeExpansionRequest, SecretPattern,
    Sensitivity,
};
use crate::domain::memory::MemoryScopeKind;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextCandidateKind {
    Memory,
    Skill,
}

impl ContextCandidateKind {
    pub(crate) fn as_label(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Skill => "skill",
        }
    }

    pub(crate) fn from_label(value: &str) -> AppResult<Self> {
        match value {
            "memory" => Ok(Self::Memory),
            "skill" => Ok(Self::Skill),
            other => Err(ErrorKind::Validation(format!("unsupported candidate item kind `{other}`")).into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextItemKind {
    Memory,
    Skill,
}

impl ContextItemKind {
    pub(crate) fn as_label(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Skill => "skill",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextFeedbackLabel {
    Useful,
    Stale,
    Wrong,
    TooSensitive,
    DoNotUseAgain,
}

impl ContextFeedbackLabel {
    pub(crate) fn as_label(self) -> &'static str {
        match self {
            Self::Useful => "useful",
            Self::Stale => "stale",
            Self::Wrong => "wrong",
            Self::TooSensitive => "too_sensitive",
            Self::DoNotUseAgain => "do_not_use_again",
        }
    }
}

const STALE_REVOKE_THRESHOLD: i64 = 3;
const WRONG_REVOKE_THRESHOLD: i64 = 2;

pub(crate) struct ContextFeedbackPolicy;

impl ContextFeedbackPolicy {
    pub(crate) fn ensure_run_terminal(status: &str) -> AppResult<()> {
        if matches!(status, "completed" | "failed" | "canceled") {
            Ok(())
        } else {
            Err(ErrorKind::Unprocessable("context feedback requires a terminal run".into()).into())
        }
    }

    pub(crate) fn should_revoke_after_label(label: ContextFeedbackLabel, matching_feedback_count: i64) -> bool {
        match label {
            ContextFeedbackLabel::Stale => matching_feedback_count >= STALE_REVOKE_THRESHOLD,
            ContextFeedbackLabel::Wrong => matching_feedback_count >= WRONG_REVOKE_THRESHOLD,
            ContextFeedbackLabel::Useful | ContextFeedbackLabel::TooSensitive | ContextFeedbackLabel::DoNotUseAgain => {
                false
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextFeedbackRecordedAudit {
    run_id: Uuid,
    item_id: Uuid,
    item_kind: String,
    label: String,
    item_state_changed: bool,
}

impl ContextFeedbackRecordedAudit {
    pub(crate) fn new(
        run_id: Uuid,
        item_id: Uuid,
        item_kind: impl Into<String>,
        label: impl Into<String>,
        item_state_changed: bool,
    ) -> Self {
        Self { run_id, item_id, item_kind: item_kind.into(), label: label.into(), item_state_changed }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.feedback.recorded"
    }

    pub(crate) fn audit_resource_type(&self) -> &'static str {
        "context_feedback"
    }

    pub(crate) fn audit_payload(&self) -> Value {
        json!({
            "run_id": self.run_id,
            "item_id": self.item_id,
            "item_kind": self.item_kind,
            "label": self.label,
            "item_state_changed": self.item_state_changed
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContextCandidateCreatedAudit {
    workspace_id: WorkspaceId,
    has_source_run: bool,
    has_target_skill: bool,
}

impl ContextCandidateCreatedAudit {
    pub(crate) fn new(workspace_id: WorkspaceId, has_source_run: bool, has_target_skill: bool) -> Self {
        Self { workspace_id, has_source_run, has_target_skill }
    }

    pub(crate) fn audit_action(self) -> &'static str {
        "governance.context.candidate.created"
    }

    pub(crate) fn audit_payload(self, item_kind: &str) -> Value {
        json!({
            "item_kind": item_kind,
            "workspace_id": self.workspace_id,
            "has_source_run": self.has_source_run,
            "has_target_skill": self.has_target_skill
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextCandidateManualRejectionAudit {
    reason: Option<String>,
    self_approval: bool,
}

impl ContextCandidateManualRejectionAudit {
    pub(crate) fn new(reason: Option<String>, self_approval: bool) -> Self {
        Self { reason, self_approval }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.candidate.rejected"
    }

    pub(crate) fn audit_payload(&self, item_kind: &str) -> Value {
        json!({
            "item_kind": item_kind,
            "reason": self.reason,
            "self_approval": self.self_approval
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ContextCandidateApprovalAudit {
    Memory {
        scope_kind: String,
        sensitivity: String,
        self_approval: bool,
        classification: Value,
    },
    Skill {
        scope_kind: Option<String>,
        sensitivity: String,
        self_approval: bool,
        from_version: i32,
        resulting_version: i32,
        skill_version_id: Uuid,
    },
}

impl ContextCandidateApprovalAudit {
    pub(crate) fn memory(
        scope_kind: impl Into<String>,
        sensitivity: impl Into<String>,
        self_approval: bool,
        classification: Value,
    ) -> Self {
        Self::Memory { scope_kind: scope_kind.into(), sensitivity: sensitivity.into(), self_approval, classification }
    }

    pub(crate) fn skill(
        scope_kind: Option<String>,
        sensitivity: impl Into<String>,
        self_approval: bool,
        from_version: i32,
        resulting_version: i32,
        skill_version_id: Uuid,
    ) -> Self {
        Self::Skill {
            scope_kind,
            sensitivity: sensitivity.into(),
            self_approval,
            from_version,
            resulting_version,
            skill_version_id,
        }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.candidate.approved"
    }

    pub(crate) fn audit_payload(&self, item_kind: &str) -> Value {
        match self {
            Self::Memory { scope_kind, sensitivity, self_approval, classification } => json!({
                "item_kind": item_kind,
                "result_kind": "memory_item",
                "scope_kind": scope_kind,
                "sensitivity": sensitivity,
                "self_approval": self_approval,
                "classification": classification
            }),
            Self::Skill {
                scope_kind,
                sensitivity,
                self_approval,
                from_version,
                resulting_version,
                skill_version_id,
            } => json!({
                "item_kind": item_kind,
                "result_kind": "skill",
                "scope_kind": scope_kind,
                "sensitivity": sensitivity,
                "self_approval": self_approval,
                "from_version": from_version,
                "resulting_version": resulting_version,
                "skill_version_id": skill_version_id
            }),
        }
    }
}

pub(crate) fn context_candidate_subject(org_id: Uuid, scope_kind: &str, scope_id: Uuid, event: &str) -> String {
    format!("broadcast.{org_id}.scope.{scope_kind}.{scope_id}.context_candidate.{event}")
}

pub(crate) fn redacted_proposal_preview(value: &Value) -> Value {
    let Some(map) = value.as_object() else {
        return json!({});
    };
    let mut out = serde_json::Map::new();
    for key in ["title", "name", "description", "scope_kind", "visibility"] {
        if let Some(value) = map.get(key)
            && value.is_string()
        {
            out.insert(key.to_string(), value.clone());
        }
    }
    if let Some(content) = map.get("content").and_then(Value::as_str) {
        let classification = ContextGovernancePolicy::classify_sensitivity(content);
        let preview = classification.redacted_preview.unwrap_or_else(|| content.chars().take(160).collect());
        out.insert("content_preview".to_string(), json!(preview));
    }
    Value::Object(out)
}

pub(crate) fn normalize_context_candidate_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

pub(crate) fn normalize_candidate_state_filter(value: Option<&str>) -> AppResult<Option<&str>> {
    match value.unwrap_or("pending") {
        "all" => Ok(None),
        "pending" => Ok(Some("pending")),
        "approved" => Ok(Some("approved")),
        "rejected" => Ok(Some("rejected")),
        "superseded" => Ok(Some("superseded")),
        other => Err(ErrorKind::Validation(format!("unsupported context candidate state filter `{other}`")).into()),
    }
}

pub(crate) fn normalize_candidate_kind_filter(value: Option<&str>) -> AppResult<Option<&str>> {
    match value.unwrap_or("all") {
        "all" => Ok(None),
        "memory" => Ok(Some("memory")),
        "skill" => Ok(Some("skill")),
        other => Err(ErrorKind::Validation(format!("unsupported context candidate kind filter `{other}`")).into()),
    }
}

pub(crate) fn normalize_scope_kind_filter(value: Option<&str>) -> AppResult<Option<&str>> {
    match value.unwrap_or("all") {
        "all" => Ok(None),
        "user" => Ok(Some("user")),
        "team" => Ok(Some("team")),
        "project" => Ok(Some("project")),
        other => Err(ErrorKind::Validation(format!("unsupported context candidate scope filter `{other}`")).into()),
    }
}

pub(crate) fn ensure_pending_candidate(candidate_id: Uuid, state: &str) -> AppResult<()> {
    if state == "pending" {
        Ok(())
    } else {
        Err(ErrorKind::Conflict(format!("context candidate {candidate_id} is already {state}")).into())
    }
}

pub(crate) fn validate_candidate_content(value: &Value) -> AppResult<()> {
    if value.as_object().is_some() {
        Ok(())
    } else {
        Err(ErrorKind::Validation("proposed_content must be a JSON object".into()).into())
    }
}

#[derive(Debug, Deserialize)]
struct MemoryCandidateContent {
    title: String,
    content: String,
    #[serde(default)]
    redacted: bool,
    visibility: Option<String>,
    confidence: Option<f64>,
    source_task_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedMemoryCandidate {
    pub(crate) title: String,
    pub(crate) content: String,
    pub(crate) content_redacted: bool,
    pub(crate) sensitivity: String,
    pub(crate) visibility: String,
    pub(crate) confidence: Option<f64>,
    pub(crate) source_task_id: Option<Uuid>,
    pub(crate) classification_payload: Value,
}

/// A memory-backed context candidate whose proposed content cannot be approved.
#[derive(Debug)]
pub(crate) struct ContextInvalidMemoryCandidateRejection {
    error: AppError,
}

impl ContextInvalidMemoryCandidateRejection {
    fn from_error(error: AppError) -> Self {
        Self { error }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.candidate.approval_rejected"
    }

    pub(crate) fn audit_payload(&self, item_kind: &str) -> Value {
        json!({
            "item_kind": item_kind,
            "reason": "invalid_memory_candidate"
        })
    }

    pub(crate) fn into_app_error(self) -> AppError {
        self.error
    }
}

/// A skill-backed context candidate that must be audited before returning an application error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextSkillContentRejection {
    matched_patterns: Vec<SecretPattern>,
    redacted_preview: Option<String>,
}

impl ContextSkillContentRejection {
    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.candidate.approval_rejected"
    }

    pub(crate) fn audit_payload(&self, item_kind: &str) -> Value {
        json!({
            "item_kind": item_kind,
            "reason": "secret_detected",
            "matched_patterns": self.matched_patterns,
            "redacted_preview": self.redacted_preview
        })
    }

    pub(crate) fn into_app_error(self) -> AppError {
        ErrorKind::Unprocessable("secret detected in skill content; submit redacted content".into()).into()
    }
}

/// A context candidate whose source run no longer supports approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContextSourceRunRejection;

impl ContextSourceRunRejection {
    pub(crate) fn audit_action(self) -> &'static str {
        "governance.context.candidate.auto_rejected"
    }

    pub(crate) fn audit_payload(self, item_kind: &str) -> Value {
        json!({
            "item_kind": item_kind,
            "reason": "source_run_unavailable"
        })
    }

    pub(crate) fn into_app_error(self) -> AppError {
        ErrorKind::Unprocessable("context candidate source run is unavailable".into()).into()
    }
}

/// A candidate owner attempted to approve their own proposal into a wider scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContextSelfApprovalRejection {
    target_scope_kind: ScopeKind,
}

impl ContextSelfApprovalRejection {
    pub(crate) fn audit_action(self) -> &'static str {
        "governance.context.candidate.approval_rejected"
    }

    pub(crate) fn audit_payload(self, item_kind: &str) -> Value {
        json!({
            "item_kind": item_kind,
            "reason": "self_approval_wider_scope",
            "scope_kind": self.target_scope_kind.as_label()
        })
    }

    pub(crate) fn into_app_error(self) -> AppError {
        ErrorKind::Forbidden.into()
    }
}

/// A context candidate approval blocked by an unconfirmed scope expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextCandidateScopeExpansionRejection {
    rejection: ScopeExpansionRejection,
    confirm_expansion: bool,
}

impl ContextCandidateScopeExpansionRejection {
    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.candidate.scope_expansion_rejected"
    }

    pub(crate) fn audit_payload(&self, item_kind: &str) -> Value {
        json!({
            "item_kind": item_kind,
            "from_scope_kind": self.rejection.from_kind.as_label(),
            "to_scope_kind": self.rejection.to_kind.as_label(),
            "reason": self.rejection.reason.as_label(),
            "confirm_expansion": self.confirm_expansion
        })
    }

    pub(crate) fn into_app_error(self) -> AppError {
        self.rejection.into_app_error()
    }
}

/// A wider-scope secret memory approval missing explicit user attestation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextSecretMemoryAttestationRejection {
    target_scope_kind: ScopeKind,
    sensitivity: String,
}

impl ContextSecretMemoryAttestationRejection {
    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.candidate.approval_rejected"
    }

    pub(crate) fn audit_payload(&self, item_kind: &str) -> Value {
        json!({
            "item_kind": item_kind,
            "reason": "user_attest_required",
            "scope_kind": self.target_scope_kind.as_label(),
            "sensitivity": self.sensitivity
        })
    }

    pub(crate) fn into_app_error(self) -> AppError {
        ErrorKind::Unprocessable("wider-scope secret memory approval requires user attestation".into()).into()
    }
}

pub(crate) struct ContextCandidatePolicy;

impl ContextCandidatePolicy {
    pub(crate) fn validate_create(
        item_kind: ContextCandidateKind,
        target_skill_id: Option<Uuid>,
        proposed_content: &Value,
    ) -> AppResult<()> {
        validate_candidate_content(proposed_content)?;
        if item_kind == ContextCandidateKind::Skill && target_skill_id.is_none() {
            return Err(ErrorKind::Validation("skill context candidates require target_skill_id".into()).into());
        }
        Ok(())
    }

    pub(crate) fn resolve_approval_scope(
        scope_kind: MemoryScopeKind,
        scope_id: Option<Uuid>,
        user_id: Uuid,
    ) -> AppResult<(ScopeKind, Uuid)> {
        let scope_kind = scope_kind.as_scope_kind();
        let scope_id = match scope_kind {
            ScopeKind::User => scope_id.unwrap_or(user_id),
            ScopeKind::Team | ScopeKind::Project => scope_id.ok_or_else(|| {
                ErrorKind::Validation(format!("scope_id is required for {} context approval", scope_kind.as_label()))
            })?,
        };
        Ok((scope_kind, scope_id))
    }

    pub(crate) fn ensure_self_approval_scope(
        self_approval: bool,
        target_scope_kind: ScopeKind,
    ) -> Result<(), ContextSelfApprovalRejection> {
        if self_approval && target_scope_kind != ScopeKind::User {
            return Err(ContextSelfApprovalRejection { target_scope_kind });
        }
        Ok(())
    }

    pub(crate) fn ensure_memory_scope_expansion(
        target_scope_kind: ScopeKind,
        confirm_expansion: bool,
    ) -> Result<(), ContextCandidateScopeExpansionRejection> {
        Self::ensure_scope_expansion(ContextScopeKind::User, target_scope_kind, confirm_expansion)
    }

    pub(crate) fn ensure_scope_expansion(
        from_kind: ContextScopeKind,
        target_scope_kind: ScopeKind,
        confirm_expansion: bool,
    ) -> Result<(), ContextCandidateScopeExpansionRejection> {
        ContextGovernancePolicy::gate_scope_expansion(ScopeExpansionRequest {
            from_kind,
            to_kind: ContextScopeKind::from_scope_kind(target_scope_kind),
            confirm_expansion,
        })
        .map(|_| ())
        .map_err(|rejection| ContextCandidateScopeExpansionRejection { rejection, confirm_expansion })
    }

    pub(crate) fn prepare_memory_candidate(
        proposed_content: &Value,
        requested_sensitivity: Option<&str>,
        redacted: bool,
    ) -> Result<PreparedMemoryCandidate, ContextInvalidMemoryCandidateRejection> {
        let proposed: MemoryCandidateContent = serde_json::from_value(proposed_content.clone())
            .map_err(|err| ErrorKind::Validation(format!("invalid memory candidate proposed_content: {err}")))
            .map_err(AppError::from)
            .map_err(ContextInvalidMemoryCandidateRejection::from_error)?;
        let title = validate_memory_title(&proposed.title)
            .map_err(ContextInvalidMemoryCandidateRejection::from_error)?
            .to_string();
        let content = proposed.content.trim();
        if content.is_empty() {
            return Err(ContextInvalidMemoryCandidateRejection::from_error(
                ErrorKind::Validation("memory candidate content must not be empty".into()).into(),
            ));
        }
        validate_confidence(proposed.confidence).map_err(ContextInvalidMemoryCandidateRejection::from_error)?;
        let visibility = validate_memory_visibility(proposed.visibility.as_deref())
            .map_err(ContextInvalidMemoryCandidateRejection::from_error)?
            .to_string();
        let classification = ContextGovernancePolicy::classify_sensitivity(content);
        if matches!(classification.sensitivity, Sensitivity::SecretDetected) && !(redacted || proposed.redacted) {
            return Err(ContextInvalidMemoryCandidateRejection::from_error(
                ErrorKind::Unprocessable("secret detected in memory candidate content; approve with redaction".into())
                    .into(),
            ));
        }

        let content_redacted = matches!(classification.sensitivity, Sensitivity::SecretDetected);
        let stored_content = if content_redacted {
            classification.redacted_preview.clone().unwrap_or_else(|| "[REDACTED]".to_string())
        } else {
            content.to_string()
        };
        let sensitivity = if content_redacted {
            "secret_detected"
        } else {
            requested_sensitivity.unwrap_or(sensitivity_label(classification.sensitivity))
        }
        .to_string();
        Ok(PreparedMemoryCandidate {
            title,
            content: stored_content,
            content_redacted,
            sensitivity: sensitivity.clone(),
            visibility,
            confidence: proposed.confidence,
            source_task_id: proposed.source_task_id,
            classification_payload: json!({
                "sensitivity": sensitivity,
                "matched_patterns": classification.matched_patterns,
                "redacted": content_redacted
            }),
        })
    }

    pub(crate) fn ensure_wider_secret_memory_attestation(
        sensitivity: &str,
        target_scope_kind: ScopeKind,
        user_attested: bool,
    ) -> Result<(), ContextSecretMemoryAttestationRejection> {
        if sensitivity == "secret_detected" && target_scope_kind != ScopeKind::User && !user_attested {
            return Err(ContextSecretMemoryAttestationRejection {
                target_scope_kind,
                sensitivity: sensitivity.to_string(),
            });
        }
        Ok(())
    }

    pub(crate) fn require_skill_target_id(target_skill_id: Option<SkillId>) -> AppResult<SkillId> {
        target_skill_id.ok_or_else(|| ErrorKind::Validation("skill candidate missing target_skill_id".into()).into())
    }

    pub(crate) fn ensure_skill_candidate_approvable(skill_id: SkillId, state: &str, is_revoked: bool) -> AppResult<()> {
        if state != "candidate" {
            return Err(ErrorKind::Conflict(format!("skill {skill_id} is not a candidate")).into());
        }
        if is_revoked {
            return Err(ErrorKind::Unprocessable(format!("skill {skill_id} is revoked")).into());
        }
        Ok(())
    }

    pub(crate) fn ensure_skill_content_approvable(content: &str) -> Result<(), ContextSkillContentRejection> {
        let classification = ContextGovernancePolicy::classify_sensitivity(content);
        if matches!(classification.sensitivity, Sensitivity::SecretDetected) {
            return Err(ContextSkillContentRejection {
                matched_patterns: classification.matched_patterns,
                redacted_preview: classification.redacted_preview,
            });
        }
        Ok(())
    }

    pub(crate) fn ensure_source_run_approvable(
        source_run_id: Option<Uuid>,
        source_run_status: Option<&str>,
    ) -> Result<(), ContextSourceRunRejection> {
        if source_run_id.is_some() && matches!(source_run_status, Some("completed")) {
            Ok(())
        } else {
            Err(ContextSourceRunRejection)
        }
    }

    pub(crate) fn resolve_skill_candidate_scope_kind(scope_kind: Option<&str>) -> AppResult<ContextScopeKind> {
        scope_kind
            .and_then(ContextScopeKind::from_label)
            .ok_or_else(|| ErrorKind::Validation("skill candidate has unsupported scope_kind".into()).into())
    }
}

pub(crate) fn validate_memory_title(title: &str) -> AppResult<&str> {
    let title = title.trim();
    if title.is_empty() || title.len() > 255 {
        return Err(ErrorKind::Validation("memory title must be 1-255 characters".into()).into());
    }
    Ok(title)
}

pub(crate) fn validate_memory_visibility(visibility: Option<&str>) -> AppResult<&str> {
    match visibility.unwrap_or("shared") {
        "private" => Ok("private"),
        "shared" => Ok("shared"),
        other => Err(ErrorKind::Validation(format!("unsupported memory visibility `{other}`")).into()),
    }
}

pub(crate) fn validate_confidence(confidence: Option<f64>) -> AppResult<()> {
    if let Some(value) = confidence
        && !(0.0..=1.0).contains(&value)
    {
        return Err(ErrorKind::Validation("confidence must be between 0 and 1".into()).into());
    }
    Ok(())
}

pub(crate) fn validate_ttl(ttl_at: Option<DateTime<Utc>>) -> AppResult<()> {
    if let Some(ttl) = ttl_at
        && ttl <= Utc::now()
    {
        return Err(ErrorKind::Validation("ttl_at must be in the future".into()).into());
    }
    Ok(())
}

pub(crate) fn validate_context_sensitivity(value: &str) -> AppResult<&str> {
    match value {
        "public" | "internal" | "confidential" | "secret_detected" => Ok(value),
        other => Err(ErrorKind::Validation(format!("unsupported context sensitivity `{other}`")).into()),
    }
}

pub(crate) fn normalize_reason(reason: Option<String>) -> AppResult<Option<String>> {
    let Some(reason) = reason else {
        return Ok(None);
    };
    let reason = reason.trim().to_string();
    if reason.len() > 500 {
        return Err(ErrorKind::Validation("rejection reason must be at most 500 characters".into()).into());
    }
    Ok((!reason.is_empty()).then_some(reason))
}

pub(crate) fn normalize_feedback_note(note: Option<String>) -> AppResult<Option<String>> {
    let Some(note) = note else {
        return Ok(None);
    };
    let note = note.trim().to_string();
    if note.len() > 4000 {
        return Err(ErrorKind::Validation("feedback note must be at most 4000 characters".into()).into());
    }
    Ok((!note.is_empty()).then_some(note))
}

pub(crate) fn sensitivity_label(sensitivity: Sensitivity) -> &'static str {
    match sensitivity {
        Sensitivity::Public => "public",
        Sensitivity::Internal => "internal",
        Sensitivity::Confidential => "confidential",
        Sensitivity::SecretDetected => "secret_detected",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn candidate_subject_is_scope_keyed() {
        let org_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let scope_id = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();

        assert_eq!(
            context_candidate_subject(org_id, "team", scope_id, "approved"),
            "broadcast.11111111-1111-4111-8111-111111111111.scope.team.22222222-2222-4222-8222-222222222222.context_candidate.approved"
        );
    }

    #[test]
    fn context_candidate_kind_labels_are_stable() {
        assert_eq!(ContextCandidateKind::Memory.as_label(), "memory");
        assert_eq!(ContextCandidateKind::Skill.as_label(), "skill");
    }

    #[test]
    fn context_item_kind_labels_are_stable() {
        assert_eq!(ContextItemKind::Memory.as_label(), "memory");
        assert_eq!(ContextItemKind::Skill.as_label(), "skill");
    }

    #[test]
    fn context_feedback_labels_are_stable() {
        assert_eq!(ContextFeedbackLabel::Useful.as_label(), "useful");
        assert_eq!(ContextFeedbackLabel::Stale.as_label(), "stale");
        assert_eq!(ContextFeedbackLabel::Wrong.as_label(), "wrong");
        assert_eq!(ContextFeedbackLabel::TooSensitive.as_label(), "too_sensitive");
        assert_eq!(ContextFeedbackLabel::DoNotUseAgain.as_label(), "do_not_use_again");
    }

    #[test]
    fn context_feedback_policy_requires_terminal_runs_and_applies_revoke_thresholds() {
        assert!(ContextFeedbackPolicy::ensure_run_terminal("completed").is_ok());
        assert!(ContextFeedbackPolicy::ensure_run_terminal("failed").is_ok());
        assert!(ContextFeedbackPolicy::ensure_run_terminal("canceled").is_ok());
        assert!(ContextFeedbackPolicy::ensure_run_terminal("running").is_err());

        assert!(!ContextFeedbackPolicy::should_revoke_after_label(ContextFeedbackLabel::Stale, 2));
        assert!(ContextFeedbackPolicy::should_revoke_after_label(ContextFeedbackLabel::Stale, 3));
        assert!(!ContextFeedbackPolicy::should_revoke_after_label(ContextFeedbackLabel::Wrong, 1));
        assert!(ContextFeedbackPolicy::should_revoke_after_label(ContextFeedbackLabel::Wrong, 2));
        assert!(!ContextFeedbackPolicy::should_revoke_after_label(ContextFeedbackLabel::Useful, 99));
    }

    #[test]
    fn feedback_recorded_audit_owns_action_resource_type_and_payload() {
        let run_id = Uuid::parse_str("99999999-9999-4999-8999-999999999999").unwrap();
        let item_id = Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap();
        let audit = ContextFeedbackRecordedAudit::new(run_id, item_id, "memory", "stale", true);
        let payload = audit.audit_payload();

        assert_eq!(audit.audit_action(), "governance.context.feedback.recorded");
        assert_eq!(audit.audit_resource_type(), "context_feedback");
        assert_eq!(payload["run_id"], run_id.to_string());
        assert_eq!(payload["item_id"], item_id.to_string());
        assert_eq!(payload["item_kind"], "memory");
        assert_eq!(payload["label"], "stale");
        assert_eq!(payload["item_state_changed"], true);
    }

    #[test]
    fn proposal_preview_redacts_content() {
        let preview = redacted_proposal_preview(&json!({
            "title": "Token",
            "content": "api_key=1234567890abcdef1234567890abcdef"
        }));

        assert_eq!(preview["title"], "Token");
        assert!(!preview["content_preview"].as_str().unwrap().contains("1234567890abcdef1234567890abcdef"));
    }

    #[test]
    fn candidate_filters_validate_allowed_values() {
        assert_eq!(normalize_context_candidate_limit(None), 50);
        assert_eq!(normalize_context_candidate_limit(Some(999)), 200);
        assert_eq!(normalize_candidate_state_filter(Some("all")).unwrap(), None);
        assert_eq!(normalize_candidate_state_filter(Some("pending")).unwrap(), Some("pending"));
        assert!(normalize_candidate_state_filter(Some("unknown")).is_err());
        assert_eq!(normalize_candidate_kind_filter(Some("memory")).unwrap(), Some("memory"));
        assert!(normalize_candidate_kind_filter(Some("other")).is_err());
        assert_eq!(normalize_scope_kind_filter(Some("project")).unwrap(), Some("project"));
        assert!(normalize_scope_kind_filter(Some("org")).is_err());
    }

    #[test]
    fn pending_candidate_policy_preserves_conflict_message() {
        let id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
        assert!(ensure_pending_candidate(id, "pending").is_ok());
        let err = ensure_pending_candidate(id, "approved").unwrap_err();
        assert!(format!("{}", err.kind).contains("already approved"));
    }

    #[test]
    fn memory_candidate_fields_are_validated_and_normalized() {
        assert_eq!(validate_memory_title("  hello  ").unwrap(), "hello");
        assert!(validate_memory_title("").is_err());
        assert_eq!(validate_memory_visibility(None).unwrap(), "shared");
        assert_eq!(validate_memory_visibility(Some("private")).unwrap(), "private");
        assert!(validate_memory_visibility(Some("org")).is_err());
        assert!(validate_confidence(Some(0.0)).is_ok());
        assert!(validate_confidence(Some(1.0)).is_ok());
        assert!(validate_confidence(Some(1.1)).is_err());
    }

    #[test]
    fn ttl_and_sensitivity_are_validated() {
        assert!(validate_ttl(Some(Utc::now() + Duration::seconds(60))).is_ok());
        assert!(validate_ttl(Some(Utc::now() - Duration::seconds(60))).is_err());
        assert_eq!(validate_context_sensitivity("internal").unwrap(), "internal");
        assert!(validate_context_sensitivity("private").is_err());
        assert_eq!(sensitivity_label(Sensitivity::SecretDetected), "secret_detected");
    }

    #[test]
    fn reasons_and_notes_are_trimmed_and_bounded() {
        assert_eq!(normalize_reason(Some("  no  ".to_string())).unwrap().as_deref(), Some("no"));
        assert_eq!(normalize_reason(Some("   ".to_string())).unwrap(), None);
        assert!(normalize_reason(Some("x".repeat(501))).is_err());
        assert_eq!(normalize_feedback_note(Some("  useful  ".to_string())).unwrap().as_deref(), Some("useful"));
        assert_eq!(normalize_feedback_note(Some("   ".to_string())).unwrap(), None);
        assert!(normalize_feedback_note(Some("x".repeat(4001))).is_err());
    }

    #[test]
    fn manual_rejection_audit_owns_action_and_payload() {
        let audit = ContextCandidateManualRejectionAudit::new(Some("not useful".to_string()), true);
        let payload = audit.audit_payload("memory");

        assert_eq!(audit.audit_action(), "governance.context.candidate.rejected");
        assert_eq!(payload["item_kind"], "memory");
        assert_eq!(payload["reason"], "not useful");
        assert_eq!(payload["self_approval"], true);
    }

    #[test]
    fn created_audit_owns_action_and_payload() {
        let workspace_id = WorkspaceId::from(Uuid::parse_str("88888888-8888-4888-8888-888888888888").unwrap());
        let audit = ContextCandidateCreatedAudit::new(workspace_id, true, false);
        let payload = audit.audit_payload("memory");

        assert_eq!(audit.audit_action(), "governance.context.candidate.created");
        assert_eq!(payload["item_kind"], "memory");
        assert_eq!(payload["workspace_id"], workspace_id.to_string());
        assert_eq!(payload["has_source_run"], true);
        assert_eq!(payload["has_target_skill"], false);
    }

    #[test]
    fn memory_approval_audit_owns_action_and_payload() {
        let audit = ContextCandidateApprovalAudit::memory(
            "team",
            "secret_detected",
            false,
            json!({
                "sensitivity": "secret_detected",
                "redacted": true
            }),
        );
        let payload = audit.audit_payload("memory");

        assert_eq!(audit.audit_action(), "governance.context.candidate.approved");
        assert_eq!(payload["item_kind"], "memory");
        assert_eq!(payload["result_kind"], "memory_item");
        assert_eq!(payload["scope_kind"], "team");
        assert_eq!(payload["sensitivity"], "secret_detected");
        assert_eq!(payload["self_approval"], false);
        assert_eq!(payload["classification"]["sensitivity"], "secret_detected");
        assert_eq!(payload["classification"]["redacted"], true);
    }

    #[test]
    fn skill_approval_audit_owns_action_and_payload() {
        let skill_version_id = Uuid::parse_str("77777777-7777-4777-8777-777777777777").unwrap();
        let audit =
            ContextCandidateApprovalAudit::skill(Some("project".to_string()), "internal", true, 2, 3, skill_version_id);
        let payload = audit.audit_payload("skill");

        assert_eq!(audit.audit_action(), "governance.context.candidate.approved");
        assert_eq!(payload["item_kind"], "skill");
        assert_eq!(payload["result_kind"], "skill");
        assert_eq!(payload["scope_kind"], "project");
        assert_eq!(payload["sensitivity"], "internal");
        assert_eq!(payload["self_approval"], true);
        assert_eq!(payload["from_version"], 2);
        assert_eq!(payload["resulting_version"], 3);
        assert_eq!(payload["skill_version_id"], skill_version_id.to_string());
    }

    #[test]
    fn candidate_content_must_be_object() {
        assert!(validate_candidate_content(&json!({"title": "x"})).is_ok());
        assert!(validate_candidate_content(&json!("x")).is_err());
    }

    #[test]
    fn candidate_create_policy_requires_skill_target_id() {
        assert!(
            ContextCandidatePolicy::validate_create(ContextCandidateKind::Memory, None, &json!({"title": "x"})).is_ok()
        );
        assert!(
            ContextCandidatePolicy::validate_create(ContextCandidateKind::Skill, None, &json!({"title": "x"})).is_err()
        );
        assert!(ContextCandidateKind::from_label("unknown").is_err());
        assert!(ContextCandidatePolicy::require_skill_target_id(None).is_err());
        assert!(ContextCandidatePolicy::resolve_skill_candidate_scope_kind(Some("user")).is_ok());
        assert!(ContextCandidatePolicy::resolve_skill_candidate_scope_kind(None).is_err());
    }

    #[test]
    fn skill_candidate_approval_policy_requires_candidate_and_active_target() {
        let skill_id = SkillId::from(Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap());

        assert!(ContextCandidatePolicy::ensure_skill_candidate_approvable(skill_id, "candidate", false).is_ok());
        assert!(ContextCandidatePolicy::ensure_skill_candidate_approvable(skill_id, "active", false).is_err());
        assert!(ContextCandidatePolicy::ensure_skill_candidate_approvable(skill_id, "candidate", true).is_err());
    }

    #[test]
    fn skill_candidate_content_policy_rejects_secret_content_with_audit_payload() {
        let fake_value = "not-a-real-secret-value";
        let fake_secret = format!("{}{}", "api_key=", fake_value);
        let rejection = ContextCandidatePolicy::ensure_skill_content_approvable(&fake_secret).unwrap_err();
        let payload = rejection.audit_payload("skill");

        assert_eq!(rejection.audit_action(), "governance.context.candidate.approval_rejected");
        assert_eq!(payload["item_kind"], "skill");
        assert_eq!(payload["reason"], "secret_detected");
        assert!(!payload["matched_patterns"].as_array().unwrap().is_empty());
        assert!(!payload["redacted_preview"].as_str().unwrap().contains(fake_value));
        assert!(matches!(rejection.into_app_error().kind, ErrorKind::Unprocessable(_)));
        assert!(ContextCandidatePolicy::ensure_skill_content_approvable("use cargo test").is_ok());
    }

    #[test]
    fn source_run_policy_requires_completed_source_run() {
        let source_run_id = Uuid::parse_str("66666666-6666-4666-8666-666666666666").unwrap();

        assert!(ContextCandidatePolicy::ensure_source_run_approvable(Some(source_run_id), Some("completed")).is_ok());
        for status in [None, Some("running"), Some("failed"), Some("canceled")] {
            let rejection = ContextCandidatePolicy::ensure_source_run_approvable(Some(source_run_id), status)
                .expect_err("non-completed source run should reject");
            let payload = rejection.audit_payload("memory");

            assert_eq!(rejection.audit_action(), "governance.context.candidate.auto_rejected");
            assert_eq!(payload["item_kind"], "memory");
            assert_eq!(payload["reason"], "source_run_unavailable");
            assert!(matches!(rejection.into_app_error().kind, ErrorKind::Unprocessable(_)));
        }

        assert!(ContextCandidatePolicy::ensure_source_run_approvable(None, Some("completed")).is_err());
    }

    #[test]
    fn candidate_approval_scope_policy_defaults_user_and_requires_group_scope_id() {
        let user_id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
        assert_eq!(
            ContextCandidatePolicy::resolve_approval_scope(MemoryScopeKind::User, None, user_id).unwrap(),
            (ScopeKind::User, user_id)
        );
        assert!(ContextCandidatePolicy::resolve_approval_scope(MemoryScopeKind::Team, None, user_id).is_err());
    }

    #[test]
    fn candidate_approval_policy_rejects_self_approval_to_wider_scope() {
        assert!(ContextCandidatePolicy::ensure_self_approval_scope(false, ScopeKind::Team).is_ok());
        assert!(ContextCandidatePolicy::ensure_self_approval_scope(true, ScopeKind::User).is_ok());

        let rejection = ContextCandidatePolicy::ensure_self_approval_scope(true, ScopeKind::Project)
            .expect_err("self-approval into project scope should reject");
        let payload = rejection.audit_payload("memory");

        assert_eq!(rejection.audit_action(), "governance.context.candidate.approval_rejected");
        assert_eq!(payload["item_kind"], "memory");
        assert_eq!(payload["reason"], "self_approval_wider_scope");
        assert_eq!(payload["scope_kind"], "project");
        assert!(matches!(rejection.into_app_error().kind, ErrorKind::Forbidden));
    }

    #[test]
    fn candidate_scope_expansion_policy_emits_auditable_rejection() {
        assert!(ContextCandidatePolicy::ensure_memory_scope_expansion(ScopeKind::Team, true).is_ok());

        let rejection = ContextCandidatePolicy::ensure_memory_scope_expansion(ScopeKind::Project, false)
            .expect_err("unconfirmed memory expansion should reject");
        let payload = rejection.audit_payload("memory");

        assert_eq!(rejection.audit_action(), "governance.context.candidate.scope_expansion_rejected");
        assert_eq!(payload["item_kind"], "memory");
        assert_eq!(payload["from_scope_kind"], "user");
        assert_eq!(payload["to_scope_kind"], "project");
        assert_eq!(payload["reason"], "confirmation_required");
        assert_eq!(payload["confirm_expansion"], false);
        assert!(matches!(rejection.into_app_error().kind, ErrorKind::Unprocessable(_)));
    }

    #[test]
    fn memory_candidate_policy_prepares_secret_redaction_and_attestation() {
        let prepared = ContextCandidatePolicy::prepare_memory_candidate(
            &json!({
                "title": "  Token  ",
                "content": "api_key=1234567890abcdef1234567890abcdef",
                "redacted": true,
                "visibility": "private",
                "confidence": 0.75
            }),
            None,
            false,
        )
        .unwrap();

        assert_eq!(prepared.title, "Token");
        assert_eq!(prepared.sensitivity, "secret_detected");
        assert!(prepared.content_redacted);
        assert_eq!(prepared.visibility, "private");
        assert_eq!(prepared.confidence, Some(0.75));
        assert!(!prepared.content.contains("1234567890abcdef1234567890abcdef"));
        assert!(
            ContextCandidatePolicy::ensure_wider_secret_memory_attestation(
                &prepared.sensitivity,
                ScopeKind::Team,
                false
            )
            .is_err()
        );
        let rejection = ContextCandidatePolicy::ensure_wider_secret_memory_attestation(
            &prepared.sensitivity,
            ScopeKind::Team,
            false,
        )
        .expect_err("team-scoped secret memory requires attestation");
        let payload = rejection.audit_payload("memory");
        assert_eq!(rejection.audit_action(), "governance.context.candidate.approval_rejected");
        assert_eq!(payload["item_kind"], "memory");
        assert_eq!(payload["reason"], "user_attest_required");
        assert_eq!(payload["scope_kind"], "team");
        assert_eq!(payload["sensitivity"], "secret_detected");
        assert!(matches!(rejection.into_app_error().kind, ErrorKind::Unprocessable(_)));
        assert!(
            ContextCandidatePolicy::ensure_wider_secret_memory_attestation(
                &prepared.sensitivity,
                ScopeKind::Team,
                true
            )
            .is_ok()
        );
    }

    #[test]
    fn memory_candidate_policy_returns_auditable_invalid_rejection() {
        let rejection = ContextCandidatePolicy::prepare_memory_candidate(
            &json!({
                "title": "Token",
                "content": "   "
            }),
            None,
            false,
        )
        .expect_err("empty candidate memory content should be rejected");
        let payload = rejection.audit_payload("memory");

        assert_eq!(rejection.audit_action(), "governance.context.candidate.approval_rejected");
        assert_eq!(payload["item_kind"], "memory");
        assert_eq!(payload["reason"], "invalid_memory_candidate");
        assert!(matches!(rejection.into_app_error().kind, ErrorKind::Validation(_)));
    }
}
