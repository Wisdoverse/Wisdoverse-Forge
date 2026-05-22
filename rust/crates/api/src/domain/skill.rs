//! Skill domain rules.
//!
//! This module owns pure skill input, lifecycle, and version policies that are
//! independent of repositories, authorization, and audit emission.

use agentforge_core::{AppError, AppResult, ErrorKind, OrgId, SkillId, WorkspaceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::context_governance::{
    ContextAuditEvent, ContextGovernancePolicy, ContextScopeKind, ScopeExpansionRejection, ScopeExpansionRequest,
    SecretPattern, Sensitivity,
};

pub(crate) fn skill_audit_resource_type() -> &'static str {
    "skill"
}

pub(crate) fn skill_audit_event(
    action: &'static str,
    resource_id: Option<Uuid>,
    payload: Value,
) -> ContextAuditEvent<'static> {
    ContextAuditEvent { action, resource_type: skill_audit_resource_type(), resource_id, payload, ip_address: None }
}

pub(crate) fn skill_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

pub(crate) fn skill_delete_response() -> Value {
    json!({ "ok": true })
}

/// Validated skill name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SkillName<'a> {
    value: &'a str,
}

impl<'a> SkillName<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        let value = value.trim();
        if value.is_empty() || value.len() > 255 {
            return Err(ErrorKind::Validation("skill name must be 1-255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Supported skill visibility scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScopeKind {
    Org,
    User,
    Team,
    Project,
}

impl SkillScopeKind {
    pub(crate) fn from_label(value: &str) -> Option<Self> {
        match value {
            "org" => Some(Self::Org),
            "user" => Some(Self::User),
            "team" => Some(Self::Team),
            "project" => Some(Self::Project),
            _ => None,
        }
    }

    pub(crate) fn as_label(self) -> &'static str {
        match self {
            Self::Org => "org",
            Self::User => "user",
            Self::Team => "team",
            Self::Project => "project",
        }
    }
}

/// Supported skill lifecycle states for create requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillState {
    Candidate,
    Active,
    Deprecated,
}

impl SkillState {
    pub(crate) fn as_label(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::Active => "active",
            Self::Deprecated => "deprecated",
        }
    }
}

/// Resolves the target scope id for a skill write request.
pub(crate) struct SkillScopeTargetPolicy;

impl SkillScopeTargetPolicy {
    pub(crate) fn resolve(
        scope_kind: SkillScopeKind,
        scope_id: Option<Uuid>,
        org_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Uuid> {
        match scope_kind {
            SkillScopeKind::Org => Ok(scope_id.unwrap_or(org_id)),
            SkillScopeKind::User => Ok(scope_id.unwrap_or(user_id)),
            SkillScopeKind::Team | SkillScopeKind::Project => scope_id.ok_or_else(|| {
                ErrorKind::Validation(format!("scope_id is required for {} skill", scope_kind.as_label())).into()
            }),
        }
    }
}

/// Resolves create-state defaults and active-publish authorization outcome.
pub(crate) struct SkillCreateStatePolicy;

impl SkillCreateStatePolicy {
    pub(crate) fn resolve(state: Option<SkillState>, can_publish_active: bool) -> AppResult<SkillState> {
        let state = state.unwrap_or(SkillState::Active);
        if state == SkillState::Active && !can_publish_active {
            return Err(ErrorKind::Forbidden.into());
        }
        Ok(state)
    }
}

/// Skill TTL policy.
pub(crate) struct SkillTtlPolicy;

impl SkillTtlPolicy {
    pub(crate) fn validate(ttl_expires_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> AppResult<()> {
        if let Some(ttl) = ttl_expires_at
            && ttl <= now
        {
            return Err(ErrorKind::Validation("ttl_expires_at must be in the future".into()).into());
        }
        Ok(())
    }
}

/// Supported persisted skill sensitivity labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillSensitivity {
    Public,
    Internal,
    Confidential,
    SecretDetected,
}

impl SkillSensitivity {
    pub(crate) fn from_sensitivity(value: Sensitivity) -> Self {
        match value {
            Sensitivity::Public => Self::Public,
            Sensitivity::Internal => Self::Internal,
            Sensitivity::Confidential => Self::Confidential,
            Sensitivity::SecretDetected => Self::SecretDetected,
        }
    }

    pub(crate) fn parse(value: &str) -> AppResult<Self> {
        match value {
            "public" => Ok(Self::Public),
            "internal" => Ok(Self::Internal),
            "confidential" => Ok(Self::Confidential),
            "secret_detected" => Ok(Self::SecretDetected),
            other => Err(ErrorKind::Validation(format!("unsupported skill sensitivity `{other}`")).into()),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Confidential => "confidential",
            Self::SecretDetected => "secret_detected",
        }
    }
}

/// Prepared skill content ready for persistence and audit emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedSkillContent {
    pub(crate) content: String,
    pub(crate) sensitivity: &'static str,
    pub(crate) audit_payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SkillCreatedAudit {
    skill_id: SkillId,
    workspace_id: Option<WorkspaceId>,
    scope_kind: Option<String>,
    scope_id: Option<Uuid>,
    state: String,
    version: i32,
    sensitivity: String,
    classification: Value,
}

impl SkillCreatedAudit {
    pub(crate) fn new(identity: SkillAuditIdentity, sensitivity: impl Into<String>, classification: Value) -> Self {
        Self {
            skill_id: identity.skill_id,
            workspace_id: identity.workspace_id,
            scope_kind: identity.scope_kind,
            scope_id: identity.scope_id,
            state: identity.state,
            version: identity.version,
            sensitivity: sensitivity.into(),
            classification,
        }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.skill.created"
    }

    pub(crate) fn audit_payload(&self) -> Value {
        json!({
            "skill_id": self.skill_id,
            "workspace_id": self.workspace_id,
            "scope_kind": self.scope_kind,
            "scope_id": self.scope_id,
            "state": self.state,
            "version": self.version,
            "sensitivity": self.sensitivity,
            "classification": self.classification
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SkillUpdatedAudit {
    skill_id: SkillId,
    workspace_id: Option<WorkspaceId>,
    scope_kind: Option<String>,
    scope_id: Option<Uuid>,
    state: String,
    version: i32,
    content_changed: bool,
    from_version: i32,
    resulting_version: i32,
    skill_version_id: Uuid,
    classification: Option<Value>,
}

impl SkillUpdatedAudit {
    pub(crate) fn new(
        identity: SkillAuditIdentity,
        content_changed: bool,
        from_version: i32,
        resulting_version: i32,
        skill_version_id: Uuid,
        classification: Option<Value>,
    ) -> Self {
        Self {
            skill_id: identity.skill_id,
            workspace_id: identity.workspace_id,
            scope_kind: identity.scope_kind,
            scope_id: identity.scope_id,
            state: identity.state,
            version: identity.version,
            content_changed,
            from_version,
            resulting_version,
            skill_version_id,
            classification,
        }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.skill.updated"
    }

    pub(crate) fn audit_payload(&self) -> Value {
        json!({
            "skill_id": self.skill_id,
            "workspace_id": self.workspace_id,
            "scope_kind": self.scope_kind,
            "scope_id": self.scope_id,
            "state": self.state,
            "version": self.version,
            "content_changed": self.content_changed,
            "from_version": self.from_version,
            "resulting_version": self.resulting_version,
            "skill_version_id": self.skill_version_id,
            "classification": self.classification
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillRevokedAudit {
    skill_id: SkillId,
    workspace_id: Option<WorkspaceId>,
    scope_kind: Option<String>,
    scope_id: Option<Uuid>,
    state: String,
    version: i32,
    from_version: i32,
    resulting_version: i32,
    skill_version_id: Uuid,
}

impl SkillRevokedAudit {
    pub(crate) fn new(
        identity: SkillAuditIdentity,
        from_version: i32,
        resulting_version: i32,
        skill_version_id: Uuid,
    ) -> Self {
        Self {
            skill_id: identity.skill_id,
            workspace_id: identity.workspace_id,
            scope_kind: identity.scope_kind,
            scope_id: identity.scope_id,
            state: identity.state,
            version: identity.version,
            from_version,
            resulting_version,
            skill_version_id,
        }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.skill.revoked"
    }

    pub(crate) fn audit_payload(&self) -> Value {
        json!({
            "skill_id": self.skill_id,
            "workspace_id": self.workspace_id,
            "scope_kind": self.scope_kind,
            "scope_id": self.scope_id,
            "state": self.state,
            "version": self.version,
            "from_version": self.from_version,
            "resulting_version": self.resulting_version,
            "skill_version_id": self.skill_version_id
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillRestoredAudit {
    skill_id: SkillId,
    workspace_id: Option<WorkspaceId>,
    scope_kind: Option<String>,
    scope_id: Option<Uuid>,
    state: String,
    version: i32,
    target_version: i32,
    from_version: i32,
    resulting_version: i32,
    skill_version_id: Uuid,
}

impl SkillRestoredAudit {
    pub(crate) fn new(
        identity: SkillAuditIdentity,
        target_version: i32,
        from_version: i32,
        resulting_version: i32,
        skill_version_id: Uuid,
    ) -> Self {
        Self {
            skill_id: identity.skill_id,
            workspace_id: identity.workspace_id,
            scope_kind: identity.scope_kind,
            scope_id: identity.scope_id,
            state: identity.state,
            version: identity.version,
            target_version,
            from_version,
            resulting_version,
            skill_version_id,
        }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.skill.restored"
    }

    pub(crate) fn audit_payload(&self) -> Value {
        json!({
            "skill_id": self.skill_id,
            "workspace_id": self.workspace_id,
            "scope_kind": self.scope_kind,
            "scope_id": self.scope_id,
            "state": self.state,
            "version": self.version,
            "target_version": self.target_version,
            "from_version": self.from_version,
            "resulting_version": self.resulting_version,
            "skill_version_id": self.skill_version_id
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillAuditIdentity {
    skill_id: SkillId,
    workspace_id: Option<WorkspaceId>,
    scope_kind: Option<String>,
    scope_id: Option<Uuid>,
    state: String,
    version: i32,
}

impl SkillAuditIdentity {
    pub(crate) fn new(
        skill_id: SkillId,
        workspace_id: Option<WorkspaceId>,
        scope_kind: Option<String>,
        scope_id: Option<Uuid>,
        state: impl Into<String>,
        version: i32,
    ) -> Self {
        Self { skill_id, workspace_id, scope_kind, scope_id, state: state.into(), version }
    }
}

/// A skill content mutation that must be audited before returning an application error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillContentRejection {
    matched_patterns: Vec<SecretPattern>,
    redacted_preview: Option<String>,
}

impl SkillContentRejection {
    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.skill.mutation_rejected"
    }

    pub(crate) fn audit_payload(&self, operation: &'static str, skill_id: Option<Uuid>) -> Value {
        json!({
            "operation": operation,
            "skill_id": skill_id,
            "reason": "secret_detected",
            "matched_patterns": self.matched_patterns,
            "redacted_preview": self.redacted_preview
        })
    }

    pub(crate) fn into_app_error(self) -> AppError {
        ErrorKind::Unprocessable("secret detected in skill content; submit redacted content".into()).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SkillContentDecision {
    Prepared(PreparedSkillContent),
    Rejected(SkillContentRejection),
}

pub(crate) struct SkillContentPolicy;

impl SkillContentPolicy {
    pub(crate) fn prepare(content: &str) -> AppResult<SkillContentDecision> {
        let content = content.trim();
        if content.is_empty() {
            return Err(ErrorKind::Validation("skill content must not be empty".into()).into());
        }

        let classification = ContextGovernancePolicy::classify_sensitivity(content);
        if matches!(classification.sensitivity, Sensitivity::SecretDetected) {
            return Ok(SkillContentDecision::Rejected(SkillContentRejection {
                matched_patterns: classification.matched_patterns,
                redacted_preview: classification.redacted_preview,
            }));
        }

        let sensitivity = SkillSensitivity::from_sensitivity(classification.sensitivity).as_str();
        Ok(SkillContentDecision::Prepared(PreparedSkillContent {
            content: content.to_string(),
            sensitivity,
            audit_payload: json!({
                "sensitivity": sensitivity,
                "matched_patterns": classification.matched_patterns
            }),
        }))
    }
}

/// JSON-object policy for structured skill metadata fields.
pub(crate) struct SkillJsonObjectPolicy;

impl SkillJsonObjectPolicy {
    pub(crate) fn resolve(value: Option<Value>) -> Value {
        value.unwrap_or_else(|| Value::Object(serde_json::Map::new()))
    }

    pub(crate) fn validate(name: &str, value: &Value) -> AppResult<()> {
        if value.as_object().is_some() {
            Ok(())
        } else {
            Err(ErrorKind::Validation(format!("{name} must be a JSON object")).into())
        }
    }
}

pub(crate) struct SkillJsonArrayPolicy;

impl SkillJsonArrayPolicy {
    pub(crate) fn resolve(value: Option<Value>) -> Value {
        value.unwrap_or_else(|| Value::Array(Vec::new()))
    }
}

/// Restore-version request policy.
pub(crate) struct SkillRestoreVersionPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SkillRestoreVersionRequest<'a> {
    pub(crate) skill_id: Uuid,
    pub(crate) target_version: i32,
    pub(crate) current_scope_kind: Option<&'a str>,
    pub(crate) snapshot_scope_kind: Option<&'a str>,
    pub(crate) snapshot_sensitivity: &'a str,
    pub(crate) snapshot_content: &'a str,
    pub(crate) confirm_expansion: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SkillRestoreVersionPlan {
    Approved,
    Rejected(SkillRestoreVersionRejection),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SkillRestoreVersionRejection {
    ScopeExpansion {
        skill_id: Uuid,
        target_version: i32,
        rejection: ScopeExpansionRejection,
        confirm_expansion: bool,
    },
    SecretDetected {
        skill_id: Uuid,
        target_version: i32,
        matched_patterns: Vec<SecretPattern>,
        redacted_preview: Option<String>,
    },
}

impl SkillRestoreVersionRejection {
    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.skill.mutation_rejected"
    }

    pub(crate) fn audit_payload(&self) -> Value {
        match self {
            Self::ScopeExpansion { skill_id, target_version, rejection, confirm_expansion } => json!({
                "operation": "restore_version",
                "skill_id": skill_id,
                "target_version": target_version,
                "reason": rejection.reason.as_label(),
                "from_scope_kind": rejection.from_kind.as_label(),
                "to_scope_kind": rejection.to_kind.as_label(),
                "confirm_expansion": confirm_expansion
            }),
            Self::SecretDetected { skill_id, target_version, matched_patterns, redacted_preview } => json!({
                "operation": "restore_version",
                "skill_id": skill_id,
                "target_version": target_version,
                "reason": "secret_detected",
                "matched_patterns": matched_patterns,
                "redacted_preview": redacted_preview
            }),
        }
    }

    pub(crate) fn into_app_error(self) -> AppError {
        match self {
            Self::ScopeExpansion { rejection, .. } => rejection.into_app_error(),
            Self::SecretDetected { .. } => {
                ErrorKind::Unprocessable("secret detected in skill content; submit redacted content".into()).into()
            }
        }
    }
}

impl SkillRestoreVersionPolicy {
    pub(crate) fn validate(version: i32, expected_current_version: Option<i32>) -> AppResult<()> {
        if version < 1 {
            return Err(ErrorKind::Validation("version must be >= 1".into()).into());
        }
        if matches!(expected_current_version, Some(version) if version < 1) {
            return Err(ErrorKind::Validation("expected_current_version must be >= 1".into()).into());
        }
        Ok(())
    }

    pub(crate) fn ensure_current_restorable(skill_id: Uuid, state: &str) -> AppResult<()> {
        if state == "revoked" {
            return Err(ErrorKind::Unprocessable(format!("skill {skill_id} is revoked and cannot be restored")).into());
        }
        Ok(())
    }

    pub(crate) fn ensure_expected_current_version(
        skill_id: Uuid,
        current_version: i32,
        expected_current_version: Option<i32>,
    ) -> AppResult<()> {
        if let Some(expected) = expected_current_version
            && current_version != expected
        {
            return Err(ErrorKind::Conflict(format!(
                "skill {skill_id} current version is {current_version}; expected {expected}"
            ))
            .into());
        }
        Ok(())
    }

    pub(crate) fn ensure_snapshot_boundary(
        skill_id: Uuid,
        version: i32,
        current_org_id: Option<OrgId>,
        current_workspace_id: Option<WorkspaceId>,
        snapshot_org_id: Option<OrgId>,
        snapshot_workspace_id: Option<WorkspaceId>,
    ) -> AppResult<()> {
        if snapshot_org_id != current_org_id || snapshot_workspace_id != current_workspace_id {
            return Err(ErrorKind::Conflict(format!(
                "skill {skill_id} version {version} belongs to a different workspace boundary"
            ))
            .into());
        }
        Ok(())
    }

    pub(crate) fn ensure_snapshot_restorable(skill_id: Uuid, version: i32, snapshot_state: &str) -> AppResult<()> {
        if snapshot_state == "revoked" {
            return Err(ErrorKind::Unprocessable(format!(
                "skill {skill_id} version {version} is revoked and cannot be restored"
            ))
            .into());
        }
        Ok(())
    }

    pub(crate) fn resolve_current_scope_kind(scope_kind: Option<&str>) -> AppResult<ContextScopeKind> {
        scope_kind
            .and_then(ContextScopeKind::from_label)
            .ok_or_else(|| ErrorKind::Validation("current skill has unsupported scope_kind".into()).into())
    }

    pub(crate) fn resolve_snapshot_scope_kind(scope_kind: Option<&str>) -> AppResult<ContextScopeKind> {
        scope_kind
            .and_then(ContextScopeKind::from_label)
            .ok_or_else(|| ErrorKind::Validation("skill snapshot has unsupported scope_kind".into()).into())
    }

    pub(crate) fn plan_restore(request: SkillRestoreVersionRequest<'_>) -> AppResult<SkillRestoreVersionPlan> {
        let from_kind = Self::resolve_current_scope_kind(request.current_scope_kind)?;
        let to_kind = Self::resolve_snapshot_scope_kind(request.snapshot_scope_kind)?;
        if let Err(rejection) = ContextGovernancePolicy::gate_scope_expansion(ScopeExpansionRequest {
            from_kind,
            to_kind,
            confirm_expansion: request.confirm_expansion,
        }) {
            return Ok(SkillRestoreVersionPlan::Rejected(SkillRestoreVersionRejection::ScopeExpansion {
                skill_id: request.skill_id,
                target_version: request.target_version,
                rejection,
                confirm_expansion: request.confirm_expansion,
            }));
        }

        let classification = ContextGovernancePolicy::classify_sensitivity(request.snapshot_content);
        if request.snapshot_sensitivity == "secret_detected"
            || matches!(classification.sensitivity, Sensitivity::SecretDetected)
        {
            return Ok(SkillRestoreVersionPlan::Rejected(SkillRestoreVersionRejection::SecretDetected {
                skill_id: request.skill_id,
                target_version: request.target_version,
                matched_patterns: classification.matched_patterns,
                redacted_preview: classification.redacted_preview,
            }));
        }

        Ok(SkillRestoreVersionPlan::Approved)
    }
}

pub(crate) struct SkillMutationPolicy;

/// A skill write blocked because the skill exists outside the request boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillBoundaryMutationRejection {
    operation: &'static str,
    skill_id: Uuid,
    workspace_id: Option<WorkspaceId>,
}

impl SkillBoundaryMutationRejection {
    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.skill.mutation_rejected"
    }

    pub(crate) fn audit_payload(&self) -> Value {
        json!({
            "operation": self.operation,
            "attempted_skill_id": self.skill_id,
            "reason": "outside_request_boundary",
            "workspace_id": self.workspace_id
        })
    }

    pub(crate) fn into_app_error(self) -> AppError {
        ErrorKind::Forbidden.into()
    }
}

pub(crate) struct SkillBoundaryMutationPolicy;

impl SkillBoundaryMutationPolicy {
    pub(crate) fn plan(
        exists_outside_request_boundary: bool,
        operation: &'static str,
        skill_id: Uuid,
        workspace_id: Option<WorkspaceId>,
    ) -> Option<SkillBoundaryMutationRejection> {
        exists_outside_request_boundary.then_some(SkillBoundaryMutationRejection { operation, skill_id, workspace_id })
    }
}

pub(crate) struct SkillBoundaryAccessPolicy;

impl SkillBoundaryAccessPolicy {
    pub(crate) fn ensure_allowed(exists_outside_request_boundary: bool) -> AppResult<()> {
        if exists_outside_request_boundary {
            return Err(ErrorKind::Forbidden.into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillMutationManagerCheck {
    Org,
    Team(Uuid),
    Project(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillMutationAccess {
    Allowed,
    RequiresManager(SkillMutationManagerCheck),
    Forbidden,
}

pub(crate) struct SkillMutationAccessPolicy;

impl SkillMutationAccessPolicy {
    pub(crate) fn plan(
        owner_user_id: Option<Uuid>,
        actor_user_id: Uuid,
        scope_kind: Option<&str>,
        scope_id: Option<Uuid>,
    ) -> SkillMutationAccess {
        if owner_user_id == Some(actor_user_id) {
            return SkillMutationAccess::Allowed;
        }

        match scope_kind.and_then(SkillScopeKind::from_label) {
            Some(SkillScopeKind::Org) => SkillMutationAccess::RequiresManager(SkillMutationManagerCheck::Org),
            Some(SkillScopeKind::Team) => match scope_id {
                Some(scope_id) => SkillMutationAccess::RequiresManager(SkillMutationManagerCheck::Team(scope_id)),
                None => SkillMutationAccess::Forbidden,
            },
            Some(SkillScopeKind::Project) => match scope_id {
                Some(scope_id) => SkillMutationAccess::RequiresManager(SkillMutationManagerCheck::Project(scope_id)),
                None => SkillMutationAccess::Forbidden,
            },
            Some(SkillScopeKind::User) | None => SkillMutationAccess::Forbidden,
        }
    }

    pub(crate) fn ensure_manager_authorized(can_manage: bool) -> AppResult<()> {
        if can_manage { Ok(()) } else { Err(ErrorKind::Forbidden.into()) }
    }
}

impl SkillMutationPolicy {
    pub(crate) fn ensure_updateable(skill_id: Uuid, state: &str) -> AppResult<()> {
        if state == "revoked" {
            return Err(ErrorKind::Conflict(format!("skill {skill_id} is revoked")).into());
        }
        Ok(())
    }

    pub(crate) fn ensure_revokeable(skill_id: Uuid, state: &str) -> AppResult<()> {
        if state == "revoked" {
            return Err(ErrorKind::Conflict(format!("skill {skill_id} is already revoked")).into());
        }
        Ok(())
    }
}

/// Skill enabled/state update result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SkillStateChange {
    enabled: Option<bool>,
    state: Option<&'static str>,
}

impl SkillStateChange {
    pub(crate) fn enabled(self) -> Option<bool> {
        self.enabled
    }

    pub(crate) fn state(self) -> Option<&'static str> {
        self.state
    }
}

/// Skill enabled/state transition policy.
pub(crate) struct SkillStateTransitionPolicy;

impl SkillStateTransitionPolicy {
    pub(crate) fn next(current_state: &str, enabled: Option<bool>) -> AppResult<SkillStateChange> {
        let change = match enabled {
            Some(true) if current_state == "candidate" => {
                return Err(ErrorKind::Unprocessable("candidate skill promotion requires approval queue".into()).into());
            }
            Some(true) => SkillStateChange { enabled: Some(true), state: Some("active") },
            Some(false) => SkillStateChange { enabled: Some(false), state: Some("deprecated") },
            None => SkillStateChange { enabled: None, state: None },
        };
        Ok(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    #[test]
    fn skill_name_trims_and_checks_bounds() {
        assert_eq!(SkillName::parse(" my-skill ").unwrap().value(), "my-skill");
        assert!(SkillName::parse("").is_err());
        assert!(SkillName::parse("   ").is_err());
        assert!(SkillName::parse(&"x".repeat(256)).is_err());
    }

    #[test]
    fn skill_ttl_requires_future_expiry_when_present() {
        let now = Utc::now();

        assert!(SkillTtlPolicy::validate(None, now).is_ok());
        assert!(SkillTtlPolicy::validate(Some(now + Duration::seconds(1)), now).is_ok());
        assert!(SkillTtlPolicy::validate(Some(now), now).is_err());
        assert!(SkillTtlPolicy::validate(Some(now - Duration::seconds(1)), now).is_err());
    }

    #[test]
    fn skill_scope_kind_labels_are_stable() {
        assert_eq!(SkillScopeKind::from_label("org"), Some(SkillScopeKind::Org));
        assert_eq!(SkillScopeKind::from_label("user"), Some(SkillScopeKind::User));
        assert_eq!(SkillScopeKind::from_label("team"), Some(SkillScopeKind::Team));
        assert_eq!(SkillScopeKind::from_label("project"), Some(SkillScopeKind::Project));
        assert_eq!(SkillScopeKind::from_label("workspace"), None);
        assert_eq!(SkillScopeKind::Org.as_label(), "org");
        assert_eq!(SkillScopeKind::User.as_label(), "user");
        assert_eq!(SkillScopeKind::Team.as_label(), "team");
        assert_eq!(SkillScopeKind::Project.as_label(), "project");
    }

    #[test]
    fn skill_state_labels_are_stable() {
        assert_eq!(SkillState::Candidate.as_label(), "candidate");
        assert_eq!(SkillState::Active.as_label(), "active");
        assert_eq!(SkillState::Deprecated.as_label(), "deprecated");
    }

    #[test]
    fn skill_scope_target_policy_defaults_personal_scopes_and_requires_group_scope_id() {
        let org_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let team_id = Uuid::now_v7();

        assert_eq!(SkillScopeTargetPolicy::resolve(SkillScopeKind::Org, None, org_id, user_id).unwrap(), org_id);
        assert_eq!(SkillScopeTargetPolicy::resolve(SkillScopeKind::User, None, org_id, user_id).unwrap(), user_id);
        assert_eq!(
            SkillScopeTargetPolicy::resolve(SkillScopeKind::Team, Some(team_id), org_id, user_id).unwrap(),
            team_id
        );
        assert!(SkillScopeTargetPolicy::resolve(SkillScopeKind::Project, None, org_id, user_id).is_err());
    }

    #[test]
    fn skill_create_state_policy_defaults_to_active_and_requires_publish_rights() {
        assert_eq!(SkillCreateStatePolicy::resolve(None, true).unwrap(), SkillState::Active);
        assert_eq!(SkillCreateStatePolicy::resolve(Some(SkillState::Candidate), false).unwrap(), SkillState::Candidate);
        assert!(SkillCreateStatePolicy::resolve(Some(SkillState::Active), false).is_err());
    }

    #[test]
    fn skill_sensitivity_accepts_supported_labels_only() {
        assert_eq!(SkillSensitivity::parse("public").unwrap().as_str(), "public");
        assert_eq!(SkillSensitivity::parse("internal").unwrap().as_str(), "internal");
        assert_eq!(SkillSensitivity::parse("confidential").unwrap().as_str(), "confidential");
        assert_eq!(SkillSensitivity::parse("secret_detected").unwrap().as_str(), "secret_detected");
        assert_eq!(SkillSensitivity::from_sensitivity(Sensitivity::Internal).as_str(), "internal");
        assert!(SkillSensitivity::parse("private").is_err());
    }

    #[test]
    fn skill_created_audit_builds_stable_action_and_payload() {
        let skill_id = SkillId::from(Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap());
        let workspace_id = WorkspaceId::from(Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap());
        let scope_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
        let classification = json!({
            "sensitivity": "internal",
            "matched_patterns": []
        });

        let audit = SkillCreatedAudit::new(
            SkillAuditIdentity::new(skill_id, Some(workspace_id), Some("project".into()), Some(scope_id), "active", 1),
            "internal",
            classification.clone(),
        );

        assert_eq!(audit.audit_action(), "governance.context.skill.created");
        assert_eq!(skill_audit_resource_type(), "skill");
        let payload = audit.audit_payload();
        assert_eq!(payload["skill_id"], skill_id.as_uuid().to_string());
        assert_eq!(payload["workspace_id"], workspace_id.as_uuid().to_string());
        assert_eq!(payload["scope_kind"], "project");
        assert_eq!(payload["scope_id"], scope_id.to_string());
        assert_eq!(payload["state"], "active");
        assert_eq!(payload["version"], 1);
        assert_eq!(payload["sensitivity"], "internal");
        assert_eq!(payload["classification"], classification);

        let event = skill_audit_event(audit.audit_action(), Some(skill_id.as_uuid()), payload);
        assert_eq!(event.action, "governance.context.skill.created");
        assert_eq!(event.resource_type, "skill");
        assert_eq!(event.resource_id, Some(skill_id.as_uuid()));
        assert_eq!(event.payload["skill_id"], skill_id.as_uuid().to_string());
        assert_eq!(event.ip_address, None);
    }

    #[test]
    fn skill_updated_audit_builds_stable_action_and_payload() {
        let skill_id = SkillId::from(Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap());
        let workspace_id = WorkspaceId::from(Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap());
        let scope_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
        let skill_version_id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
        let classification = json!({
            "sensitivity": "internal",
            "matched_patterns": []
        });

        let audit = SkillUpdatedAudit::new(
            SkillAuditIdentity::new(
                skill_id,
                Some(workspace_id),
                Some("project".into()),
                Some(scope_id),
                "deprecated",
                4,
            ),
            true,
            3,
            4,
            skill_version_id,
            Some(classification.clone()),
        );

        assert_eq!(audit.audit_action(), "governance.context.skill.updated");
        let payload = audit.audit_payload();
        assert_eq!(payload["skill_id"], skill_id.as_uuid().to_string());
        assert_eq!(payload["workspace_id"], workspace_id.as_uuid().to_string());
        assert_eq!(payload["scope_kind"], "project");
        assert_eq!(payload["scope_id"], scope_id.to_string());
        assert_eq!(payload["state"], "deprecated");
        assert_eq!(payload["version"], 4);
        assert_eq!(payload["content_changed"], true);
        assert_eq!(payload["from_version"], 3);
        assert_eq!(payload["resulting_version"], 4);
        assert_eq!(payload["skill_version_id"], skill_version_id.to_string());
        assert_eq!(payload["classification"], classification);
    }

    #[test]
    fn skill_revoked_audit_builds_stable_action_and_payload() {
        let skill_id = SkillId::from(Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap());
        let workspace_id = WorkspaceId::from(Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap());
        let scope_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
        let skill_version_id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();

        let audit = SkillRevokedAudit::new(
            SkillAuditIdentity::new(skill_id, Some(workspace_id), Some("project".into()), Some(scope_id), "revoked", 5),
            4,
            5,
            skill_version_id,
        );

        assert_eq!(audit.audit_action(), "governance.context.skill.revoked");
        let payload = audit.audit_payload();
        assert_eq!(payload["skill_id"], skill_id.as_uuid().to_string());
        assert_eq!(payload["workspace_id"], workspace_id.as_uuid().to_string());
        assert_eq!(payload["scope_kind"], "project");
        assert_eq!(payload["scope_id"], scope_id.to_string());
        assert_eq!(payload["state"], "revoked");
        assert_eq!(payload["version"], 5);
        assert_eq!(payload["from_version"], 4);
        assert_eq!(payload["resulting_version"], 5);
        assert_eq!(payload["skill_version_id"], skill_version_id.to_string());
    }

    #[test]
    fn skill_restored_audit_builds_stable_action_and_payload() {
        let skill_id = SkillId::from(Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap());
        let workspace_id = WorkspaceId::from(Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap());
        let scope_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
        let skill_version_id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();

        let audit = SkillRestoredAudit::new(
            SkillAuditIdentity::new(skill_id, Some(workspace_id), Some("project".into()), Some(scope_id), "active", 6),
            2,
            5,
            6,
            skill_version_id,
        );

        assert_eq!(audit.audit_action(), "governance.context.skill.restored");
        let payload = audit.audit_payload();
        assert_eq!(payload["skill_id"], skill_id.as_uuid().to_string());
        assert_eq!(payload["workspace_id"], workspace_id.as_uuid().to_string());
        assert_eq!(payload["scope_kind"], "project");
        assert_eq!(payload["scope_id"], scope_id.to_string());
        assert_eq!(payload["state"], "active");
        assert_eq!(payload["version"], 6);
        assert_eq!(payload["target_version"], 2);
        assert_eq!(payload["from_version"], 5);
        assert_eq!(payload["resulting_version"], 6);
        assert_eq!(payload["skill_version_id"], skill_version_id.to_string());
    }

    #[test]
    fn skill_json_object_policy_rejects_non_objects() {
        assert_eq!(SkillJsonObjectPolicy::resolve(None), json!({}));
        assert_eq!(SkillJsonObjectPolicy::resolve(Some(json!({ "source": "manual" }))), json!({ "source": "manual" }));
        assert!(SkillJsonObjectPolicy::validate("provenance", &json!({})).is_ok());
        assert!(SkillJsonObjectPolicy::validate("provenance", &json!([])).is_err());
        assert!(SkillJsonObjectPolicy::validate("provenance", &json!("value")).is_err());
    }

    #[test]
    fn skill_json_array_policy_defaults_to_empty_array() {
        assert_eq!(SkillJsonArrayPolicy::resolve(None), json!([]));
        assert_eq!(SkillJsonArrayPolicy::resolve(Some(json!(["input"]))), json!(["input"]));
    }

    #[test]
    fn skill_restore_version_policy_rejects_non_positive_versions() {
        assert!(SkillRestoreVersionPolicy::validate(1, None).is_ok());
        assert!(SkillRestoreVersionPolicy::validate(1, Some(1)).is_ok());
        assert!(SkillRestoreVersionPolicy::validate(0, None).is_err());
        assert!(SkillRestoreVersionPolicy::validate(1, Some(0)).is_err());
    }

    #[test]
    fn skill_mutation_policy_rejects_revoked_updates_and_revoke_replays() {
        let skill_id = Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap();
        assert!(SkillMutationPolicy::ensure_updateable(skill_id, "active").is_ok());
        assert!(SkillMutationPolicy::ensure_updateable(skill_id, "revoked").is_err());
        assert!(SkillMutationPolicy::ensure_revokeable(skill_id, "deprecated").is_ok());
        assert!(SkillMutationPolicy::ensure_revokeable(skill_id, "revoked").is_err());
    }

    #[test]
    fn skill_mutation_access_policy_prefers_owner_then_manager_scope() {
        let owner = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let actor = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
        let scope_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();

        assert_eq!(
            SkillMutationAccessPolicy::plan(Some(owner), owner, Some("user"), Some(owner)),
            SkillMutationAccess::Allowed
        );
        assert_eq!(
            SkillMutationAccessPolicy::plan(Some(owner), actor, Some("org"), None),
            SkillMutationAccess::RequiresManager(SkillMutationManagerCheck::Org)
        );
        assert_eq!(
            SkillMutationAccessPolicy::plan(Some(owner), actor, Some("team"), Some(scope_id)),
            SkillMutationAccess::RequiresManager(SkillMutationManagerCheck::Team(scope_id))
        );
        assert_eq!(
            SkillMutationAccessPolicy::plan(Some(owner), actor, Some("project"), Some(scope_id)),
            SkillMutationAccess::RequiresManager(SkillMutationManagerCheck::Project(scope_id))
        );
        assert_eq!(
            SkillMutationAccessPolicy::plan(Some(owner), actor, Some("team"), None),
            SkillMutationAccess::Forbidden
        );
        assert_eq!(
            SkillMutationAccessPolicy::plan(Some(owner), actor, Some("user"), Some(owner)),
            SkillMutationAccess::Forbidden
        );
        assert_eq!(
            SkillMutationAccessPolicy::plan(None, actor, Some("workspace"), Some(scope_id)),
            SkillMutationAccess::Forbidden
        );
    }

    #[test]
    fn skill_mutation_access_policy_maps_manager_checks_to_forbidden() {
        assert!(SkillMutationAccessPolicy::ensure_manager_authorized(true).is_ok());
        assert!(matches!(
            SkillMutationAccessPolicy::ensure_manager_authorized(false).unwrap_err().kind,
            ErrorKind::Forbidden
        ));
    }

    #[test]
    fn skill_boundary_mutation_policy_plans_auditable_forbidden_rejection() {
        let skill_id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
        let workspace_id = WorkspaceId::from(Uuid::parse_str("99999999-9999-4999-8999-999999999999").unwrap());

        assert!(SkillBoundaryMutationPolicy::plan(false, "update", skill_id, Some(workspace_id)).is_none());

        let rejection = SkillBoundaryMutationPolicy::plan(true, "restore_version", skill_id, Some(workspace_id))
            .expect("outside-boundary writes should be rejected");

        assert_eq!(rejection.audit_action(), "governance.context.skill.mutation_rejected");
        let payload = rejection.audit_payload();
        assert_eq!(payload["operation"], "restore_version");
        assert_eq!(payload["attempted_skill_id"], skill_id.to_string());
        assert_eq!(payload["reason"], "outside_request_boundary");
        assert_eq!(payload["workspace_id"], workspace_id.as_uuid().to_string());
        assert!(matches!(rejection.into_app_error().kind, ErrorKind::Forbidden));
    }

    #[test]
    fn skill_boundary_access_policy_rejects_outside_boundary_reads() {
        assert!(SkillBoundaryAccessPolicy::ensure_allowed(false).is_ok());
        assert!(matches!(SkillBoundaryAccessPolicy::ensure_allowed(true).unwrap_err().kind, ErrorKind::Forbidden));
    }

    #[test]
    fn skill_restore_policy_validates_current_snapshot_and_scope_rules() {
        let skill_id = Uuid::parse_str("66666666-6666-4666-8666-666666666666").unwrap();
        let org_id = OrgId::from(Uuid::parse_str("77777777-7777-4777-8777-777777777777").unwrap());
        let other_org_id = OrgId::from(Uuid::parse_str("88888888-8888-4888-8888-888888888888").unwrap());
        let workspace_id = WorkspaceId::from(Uuid::parse_str("99999999-9999-4999-8999-999999999999").unwrap());

        assert!(SkillRestoreVersionPolicy::ensure_current_restorable(skill_id, "active").is_ok());
        assert!(SkillRestoreVersionPolicy::ensure_current_restorable(skill_id, "revoked").is_err());
        assert!(SkillRestoreVersionPolicy::ensure_expected_current_version(skill_id, 3, Some(3)).is_ok());
        assert!(SkillRestoreVersionPolicy::ensure_expected_current_version(skill_id, 3, Some(2)).is_err());
        assert!(
            SkillRestoreVersionPolicy::ensure_snapshot_boundary(
                skill_id,
                2,
                Some(org_id),
                Some(workspace_id),
                Some(org_id),
                Some(workspace_id),
            )
            .is_ok()
        );
        assert!(
            SkillRestoreVersionPolicy::ensure_snapshot_boundary(
                skill_id,
                2,
                Some(org_id),
                Some(workspace_id),
                Some(other_org_id),
                Some(workspace_id),
            )
            .is_err()
        );
        assert!(SkillRestoreVersionPolicy::ensure_snapshot_restorable(skill_id, 2, "active").is_ok());
        assert!(SkillRestoreVersionPolicy::ensure_snapshot_restorable(skill_id, 2, "revoked").is_err());
        assert_eq!(
            SkillRestoreVersionPolicy::resolve_current_scope_kind(Some("project")).unwrap(),
            ContextScopeKind::Project
        );
        assert!(SkillRestoreVersionPolicy::resolve_snapshot_scope_kind(None).is_err());
    }

    #[test]
    fn skill_restore_policy_plans_scope_and_secret_rejections() {
        let skill_id = Uuid::parse_str("66666666-6666-4666-8666-666666666666").unwrap();

        assert!(matches!(
            SkillRestoreVersionPolicy::plan_restore(SkillRestoreVersionRequest {
                skill_id,
                target_version: 2,
                current_scope_kind: Some("user"),
                snapshot_scope_kind: Some("team"),
                snapshot_sensitivity: "internal",
                snapshot_content: "restore clean guidance",
                confirm_expansion: true,
            })
            .expect("scope labels should be valid"),
            SkillRestoreVersionPlan::Approved
        ));

        let plan = SkillRestoreVersionPolicy::plan_restore(SkillRestoreVersionRequest {
            skill_id,
            target_version: 2,
            current_scope_kind: Some("user"),
            snapshot_scope_kind: Some("project"),
            snapshot_sensitivity: "internal",
            snapshot_content: "restore clean guidance",
            confirm_expansion: false,
        })
        .expect("scope labels should be valid");
        let SkillRestoreVersionPlan::Rejected(rejection) = plan else {
            panic!("unconfirmed restore expansion should be rejected");
        };
        assert_eq!(rejection.audit_action(), "governance.context.skill.mutation_rejected");
        let payload = rejection.audit_payload();
        assert_eq!(payload["operation"], "restore_version");
        assert_eq!(payload["skill_id"], skill_id.to_string());
        assert_eq!(payload["target_version"], 2);
        assert_eq!(payload["reason"], "confirmation_required");

        let secret = synthetic_assigned_secret();
        let secret_fragment = synthetic_secret_fragment();
        let plan = SkillRestoreVersionPolicy::plan_restore(SkillRestoreVersionRequest {
            skill_id,
            target_version: 3,
            current_scope_kind: Some("team"),
            snapshot_scope_kind: Some("team"),
            snapshot_sensitivity: "internal",
            snapshot_content: &secret,
            confirm_expansion: false,
        })
        .expect("scope labels should be valid");
        let SkillRestoreVersionPlan::Rejected(rejection) = plan else {
            panic!("secret snapshot should be rejected");
        };
        let payload = rejection.audit_payload();
        assert_eq!(payload["reason"], "secret_detected");
        assert!(payload["matched_patterns"].as_array().is_some_and(|items| !items.is_empty()));
        assert!(!payload["redacted_preview"].as_str().unwrap_or_default().contains(&secret_fragment));
        assert!(matches!(rejection.into_app_error().kind, ErrorKind::Unprocessable(_)));
    }

    #[test]
    fn skill_state_transition_maps_enabled_updates() {
        let active = SkillStateTransitionPolicy::next("deprecated", Some(true)).unwrap();
        assert_eq!(active.enabled(), Some(true));
        assert_eq!(active.state(), Some("active"));

        let deprecated = SkillStateTransitionPolicy::next("active", Some(false)).unwrap();
        assert_eq!(deprecated.enabled(), Some(false));
        assert_eq!(deprecated.state(), Some("deprecated"));

        let unchanged = SkillStateTransitionPolicy::next("active", None).unwrap();
        assert_eq!(unchanged.enabled(), None);
        assert_eq!(unchanged.state(), None);
    }

    #[test]
    fn skill_state_transition_rejects_direct_candidate_promotion() {
        assert!(SkillStateTransitionPolicy::next("candidate", Some(true)).is_err());
    }

    #[test]
    fn skill_response_helpers_keep_legacy_data_shape() {
        assert_eq!(skill_data_response(vec!["review"])["data"], json!(["review"]));
        assert_eq!(skill_delete_response()["ok"], true);
    }

    fn synthetic_assigned_secret() -> String {
        let key = ["api", "_", "key"].concat();
        let value = ["12345678", "90abcdef", "12345678", "90abcdef"].concat();
        format!("{key}={value}")
    }

    fn synthetic_secret_fragment() -> String {
        ["12345678", "90abcdef"].concat()
    }

    #[test]
    fn skill_content_policy_rejects_empty_content() {
        assert!(SkillContentPolicy::prepare("  ").is_err());
    }

    #[test]
    fn skill_content_policy_requests_auditable_secret_rejection() {
        let secret = synthetic_assigned_secret();
        let secret_fragment = synthetic_secret_fragment();
        let skill_id = Uuid::now_v7();

        let decision = SkillContentPolicy::prepare(&secret).expect("classification should succeed");

        let SkillContentDecision::Rejected(rejection) = decision else {
            panic!("secret should require auditable rejection");
        };
        assert_eq!(rejection.audit_action(), "governance.context.skill.mutation_rejected");
        let payload = rejection.audit_payload("create", Some(skill_id));
        assert_eq!(payload["operation"], "create");
        assert_eq!(payload["skill_id"], skill_id.to_string());
        assert_eq!(payload["reason"], "secret_detected");
        assert!(payload["matched_patterns"].as_array().is_some_and(|items| !items.is_empty()));
        assert!(!payload["redacted_preview"].as_str().unwrap_or_default().contains(&secret_fragment));
    }

    #[test]
    fn skill_content_policy_prepares_clean_content() {
        let decision = SkillContentPolicy::prepare("  use cargo test  ").expect("classification should succeed");

        let SkillContentDecision::Prepared(prepared) = decision else {
            panic!("clean content should prepare");
        };
        assert_eq!(prepared.content, "use cargo test");
        assert_eq!(prepared.sensitivity, "internal");
        assert_eq!(prepared.audit_payload["sensitivity"], "internal");
    }
}
