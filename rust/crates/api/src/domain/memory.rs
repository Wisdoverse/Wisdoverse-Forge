//! Memory domain rules.
//!
//! This module owns pure memory item input, pagination, and retention policies
//! that are independent of repositories, HTTP route DTOs, and audit emission.

use agentforge_core::{AppError, AppResult, ErrorKind, ScopeKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::context_governance::{
    ContextAuditEvent, ContextGovernancePolicy, ContextScopeKind, ScopeExpansionRejection, ScopeExpansionRequest,
    SecretPattern, Sensitivity,
};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

pub(crate) fn memory_audit_resource_type() -> &'static str {
    "memory_item"
}

pub(crate) fn memory_audit_event(action: &'static str, payload: Value) -> ContextAuditEvent<'static> {
    ContextAuditEvent {
        action,
        resource_type: memory_audit_resource_type(),
        // The current audit route is org-wide. Avoid writing raw memory item IDs
        // until scope-aware audit projection lands.
        resource_id: None,
        payload,
        ip_address: None,
    }
}

/// Supported memory item scope kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScopeKind {
    User,
    Team,
    Project,
}

impl MemoryScopeKind {
    pub fn from_label(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "team" => Some(Self::Team),
            "project" => Some(Self::Project),
            _ => None,
        }
    }

    pub fn as_scope_kind(self) -> ScopeKind {
        match self {
            Self::User => ScopeKind::User,
            Self::Team => ScopeKind::Team,
            Self::Project => ScopeKind::Project,
        }
    }
}

pub(crate) struct MemoryScopeTargetPolicy;

impl MemoryScopeTargetPolicy {
    pub(crate) fn resolve(
        scope_kind: MemoryScopeKind,
        scope_id: Option<Uuid>,
        user_id: Uuid,
    ) -> AppResult<(ScopeKind, Uuid)> {
        let scope_kind = scope_kind.as_scope_kind();
        let scope_id = match scope_kind {
            ScopeKind::User => scope_id.unwrap_or(user_id),
            ScopeKind::Team | ScopeKind::Project => scope_id.ok_or_else(|| {
                ErrorKind::Validation(format!("scope_id is required for {} memory", scope_kind.as_label()))
            })?,
        };
        Ok((scope_kind, scope_id))
    }
}

pub(crate) struct MemoryReclassificationPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MemoryReclassificationRequest<'a> {
    pub(crate) current_scope_kind: &'a str,
    pub(crate) target_scope_kind: ScopeKind,
    pub(crate) sensitivity: &'a str,
    pub(crate) content_redacted: bool,
    pub(crate) confirm_sensitive: bool,
    pub(crate) confirm_expansion: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MemoryReclassificationDecision {
    from_kind: ContextScopeKind,
    to_kind: ContextScopeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryReclassifiedAudit {
    from_kind: ContextScopeKind,
    to_kind: ContextScopeKind,
    sensitivity: String,
    content_redacted: bool,
}

impl MemoryReclassifiedAudit {
    pub(crate) fn from_decision(
        decision: MemoryReclassificationDecision,
        sensitivity: impl Into<String>,
        content_redacted: bool,
    ) -> Self {
        Self {
            from_kind: decision.from_kind,
            to_kind: decision.to_kind,
            sensitivity: sensitivity.into(),
            content_redacted,
        }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.memory.reclassified"
    }

    pub(crate) fn audit_payload(&self) -> Value {
        json!({
            "from_scope_kind": self.from_kind.as_label(),
            "to_scope_kind": self.to_kind.as_label(),
            "sensitivity": self.sensitivity,
            "content_redacted": self.content_redacted
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryReclassificationRejection {
    ScopeExpansion { rejection: ScopeExpansionRejection, confirm_expansion: bool },
    SensitiveScopeChange { confirm_sensitive: bool },
}

impl MemoryReclassificationRejection {
    pub(crate) fn audit_action(self) -> &'static str {
        match self {
            Self::ScopeExpansion { .. } => "governance.context.memory.scope_expansion_rejected",
            Self::SensitiveScopeChange { .. } => "governance.context.memory.sensitive_scope_change_rejected",
        }
    }

    pub(crate) fn audit_payload(self) -> Value {
        match self {
            Self::ScopeExpansion { rejection, confirm_expansion } => json!({
                "from_scope_kind": rejection.from_kind.as_label(),
                "to_scope_kind": rejection.to_kind.as_label(),
                "reason": rejection.reason.as_label(),
                "confirm_expansion": confirm_expansion
            }),
            Self::SensitiveScopeChange { confirm_sensitive } => json!({
                "reason": "secret_detected",
                "sensitivity": "secret_detected",
                "content_redacted": false,
                "confirm_sensitive": confirm_sensitive
            }),
        }
    }

    pub(crate) fn into_app_error(self) -> AppError {
        match self {
            Self::ScopeExpansion { rejection, .. } => rejection.into_app_error(),
            Self::SensitiveScopeChange { .. } => ErrorKind::Unprocessable(
                "secret-detected memory requires explicit redaction before scope change".into(),
            )
            .into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryReclassificationPlan {
    Approved(MemoryReclassificationDecision),
    Rejected(MemoryReclassificationRejection),
}

impl MemoryReclassificationPolicy {
    pub(crate) fn resolve_current_scope_kind(scope_kind: &str) -> AppResult<ContextScopeKind> {
        ContextScopeKind::from_label(scope_kind)
            .ok_or_else(|| ErrorKind::Validation(format!("unsupported memory scope kind `{scope_kind}`")).into())
    }

    pub(crate) fn plan(request: MemoryReclassificationRequest<'_>) -> AppResult<MemoryReclassificationPlan> {
        let from_kind = Self::resolve_current_scope_kind(request.current_scope_kind)?;
        let to_kind = ContextScopeKind::from_scope_kind(request.target_scope_kind);

        if let Err(rejection) = ContextGovernancePolicy::gate_scope_expansion(ScopeExpansionRequest {
            from_kind,
            to_kind,
            confirm_expansion: request.confirm_expansion,
        }) {
            return Ok(MemoryReclassificationPlan::Rejected(MemoryReclassificationRejection::ScopeExpansion {
                rejection,
                confirm_expansion: request.confirm_expansion,
            }));
        }

        if request.sensitivity == "secret_detected" && !request.content_redacted && !request.confirm_sensitive {
            return Ok(MemoryReclassificationPlan::Rejected(MemoryReclassificationRejection::SensitiveScopeChange {
                confirm_sensitive: request.confirm_sensitive,
            }));
        }

        Ok(MemoryReclassificationPlan::Approved(MemoryReclassificationDecision { from_kind, to_kind }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryMutationManagerCheck {
    Team(Uuid),
    Project(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryMutationAccess {
    Allowed,
    RequiresManager(MemoryMutationManagerCheck),
    Forbidden,
}

pub(crate) struct MemoryMutationAccessPolicy;

impl MemoryMutationAccessPolicy {
    pub(crate) fn plan(
        owner_user_id: Uuid,
        actor_user_id: Uuid,
        scope_kind: &str,
        scope_id: Uuid,
    ) -> MemoryMutationAccess {
        if owner_user_id == actor_user_id {
            return MemoryMutationAccess::Allowed;
        }

        match MemoryScopeKind::from_label(scope_kind) {
            Some(MemoryScopeKind::Team) => {
                MemoryMutationAccess::RequiresManager(MemoryMutationManagerCheck::Team(scope_id))
            }
            Some(MemoryScopeKind::Project) => {
                MemoryMutationAccess::RequiresManager(MemoryMutationManagerCheck::Project(scope_id))
            }
            Some(MemoryScopeKind::User) | None => MemoryMutationAccess::Forbidden,
        }
    }

    pub(crate) fn ensure_manager_authorized(can_manage: bool) -> AppResult<()> {
        if can_manage { Ok(()) } else { Err(ErrorKind::Forbidden.into()) }
    }
}

/// Memory list pagination policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MemoryListPage {
    limit: i64,
    offset: i64,
}

impl MemoryListPage {
    pub(crate) fn new(limit: Option<i64>, offset: Option<i64>) -> Self {
        Self { limit: limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT), offset: offset.unwrap_or(0).max(0) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn offset(self) -> i64 {
        self.offset
    }
}

/// Validated memory title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MemoryTitle<'a> {
    value: &'a str,
}

impl<'a> MemoryTitle<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        let value = value.trim();
        if value.is_empty() || value.len() > 255 {
            return Err(ErrorKind::Validation("memory title must be 1-255 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Memory visibility policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryVisibility {
    Private,
    Shared,
}

impl MemoryVisibility {
    pub(crate) fn parse(value: Option<&str>) -> AppResult<Self> {
        match value.unwrap_or("shared") {
            "private" => Ok(Self::Private),
            "shared" => Ok(Self::Shared),
            other => Err(ErrorKind::Validation(format!("unsupported memory visibility `{other}`")).into()),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Shared => "shared",
        }
    }
}

/// Memory TTL policy.
pub(crate) struct MemoryTtlPolicy;

impl MemoryTtlPolicy {
    pub(crate) fn validate(ttl_expires_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> AppResult<()> {
        if let Some(ttl) = ttl_expires_at
            && ttl <= now
        {
            return Err(ErrorKind::Validation("ttl_expires_at must be in the future".into()).into());
        }
        Ok(())
    }
}

/// Memory confidence score policy.
pub(crate) struct MemoryConfidencePolicy;

impl MemoryConfidencePolicy {
    pub(crate) fn validate(confidence: Option<f64>) -> AppResult<()> {
        if let Some(value) = confidence
            && !(0.0..=1.0).contains(&value)
        {
            return Err(ErrorKind::Validation("confidence must be between 0 and 1".into()).into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryContentReadAudit {
    scope_kind: String,
    sensitivity: String,
    content_redacted: bool,
}

impl MemoryContentReadAudit {
    pub(crate) fn new(scope_kind: impl Into<String>, sensitivity: impl Into<String>, content_redacted: bool) -> Self {
        Self { scope_kind: scope_kind.into(), sensitivity: sensitivity.into(), content_redacted }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.memory.content_read"
    }

    pub(crate) fn audit_payload(&self) -> Value {
        json!({
            "scope_kind": self.scope_kind,
            "sensitivity": self.sensitivity,
            "content_redacted": self.content_redacted
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryCreatedAudit {
    scope_kind: String,
    visibility: String,
    sensitivity: String,
    content_redacted: bool,
    classification: Value,
}

impl MemoryCreatedAudit {
    pub(crate) fn new(
        scope_kind: impl Into<String>,
        visibility: impl Into<String>,
        sensitivity: impl Into<String>,
        content_redacted: bool,
        classification: Value,
    ) -> Self {
        Self {
            scope_kind: scope_kind.into(),
            visibility: visibility.into(),
            sensitivity: sensitivity.into(),
            content_redacted,
            classification,
        }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.memory.created"
    }

    pub(crate) fn audit_payload(&self) -> Value {
        json!({
            "scope_kind": self.scope_kind,
            "visibility": self.visibility,
            "sensitivity": self.sensitivity,
            "content_redacted": self.content_redacted,
            "classification": self.classification
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryUpdatedAudit {
    scope_kind: String,
    visibility: String,
    sensitivity: String,
    content_changed: bool,
    content_redacted: bool,
}

impl MemoryUpdatedAudit {
    pub(crate) fn new(
        scope_kind: impl Into<String>,
        visibility: impl Into<String>,
        sensitivity: impl Into<String>,
        content_changed: bool,
        content_redacted: bool,
    ) -> Self {
        Self {
            scope_kind: scope_kind.into(),
            visibility: visibility.into(),
            sensitivity: sensitivity.into(),
            content_changed,
            content_redacted,
        }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.memory.updated"
    }

    pub(crate) fn audit_payload(&self) -> Value {
        json!({
            "scope_kind": self.scope_kind,
            "visibility": self.visibility,
            "sensitivity": self.sensitivity,
            "content_changed": self.content_changed,
            "content_redacted": self.content_redacted
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryRevokedAudit {
    scope_kind: String,
    sensitivity: String,
}

impl MemoryRevokedAudit {
    pub(crate) fn new(scope_kind: impl Into<String>, sensitivity: impl Into<String>) -> Self {
        Self { scope_kind: scope_kind.into(), sensitivity: sensitivity.into() }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.memory.revoked"
    }

    pub(crate) fn audit_payload(&self) -> Value {
        json!({
            "scope_kind": self.scope_kind,
            "sensitivity": self.sensitivity
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryTtlExtendedAudit {
    scope_kind: String,
    ttl_expires_at: Option<DateTime<Utc>>,
}

impl MemoryTtlExtendedAudit {
    pub(crate) fn new(scope_kind: impl Into<String>, ttl_expires_at: Option<DateTime<Utc>>) -> Self {
        Self { scope_kind: scope_kind.into(), ttl_expires_at }
    }

    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.memory.ttl_extended"
    }

    pub(crate) fn audit_payload(&self) -> Value {
        json!({
            "scope_kind": self.scope_kind,
            "ttl_expires_at": self.ttl_expires_at
        })
    }
}

/// Prepared memory content ready for persistence and audit emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedMemoryContent {
    pub(crate) content: String,
    pub(crate) content_redacted: bool,
    pub(crate) sensitivity: &'static str,
    pub(crate) audit_payload: Value,
}

/// A content mutation that must be audited before returning an application error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryContentRejection {
    matched_patterns: Vec<SecretPattern>,
    redacted_preview: Option<String>,
}

impl MemoryContentRejection {
    pub(crate) fn audit_action(&self) -> &'static str {
        "governance.context.memory.rejected"
    }

    pub(crate) fn audit_payload(&self, operation: &str) -> Value {
        json!({
            "operation": operation,
            "reason": "secret_detected",
            "matched_patterns": self.matched_patterns,
            "redacted_preview": self.redacted_preview
        })
    }

    pub(crate) fn into_app_error(self) -> AppError {
        ErrorKind::Unprocessable("secret detected in memory content; submit redacted content".into()).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MemoryContentDecision {
    Prepared(PreparedMemoryContent),
    Rejected(MemoryContentRejection),
}

pub(crate) struct MemoryContentPolicy;

impl MemoryContentPolicy {
    pub(crate) fn prepare(content: &str, redacted: bool) -> AppResult<MemoryContentDecision> {
        let content = content.trim();
        if content.is_empty() {
            return Err(ErrorKind::Validation("memory content must not be empty".into()).into());
        }

        let classification = ContextGovernancePolicy::classify_sensitivity(content);
        if matches!(classification.sensitivity, Sensitivity::SecretDetected) && !redacted {
            return Ok(MemoryContentDecision::Rejected(MemoryContentRejection {
                matched_patterns: classification.matched_patterns,
                redacted_preview: classification.redacted_preview,
            }));
        }

        let content_redacted = matches!(classification.sensitivity, Sensitivity::SecretDetected);
        let stored_content = if content_redacted {
            classification.redacted_preview.clone().unwrap_or_else(|| "[REDACTED]".to_string())
        } else {
            content.to_string()
        };
        let sensitivity = sensitivity_label(classification.sensitivity);

        Ok(MemoryContentDecision::Prepared(PreparedMemoryContent {
            content: stored_content,
            content_redacted,
            sensitivity,
            audit_payload: json!({
                "sensitivity": sensitivity,
                "matched_patterns": classification.matched_patterns,
                "redacted": content_redacted
            }),
        }))
    }
}

fn sensitivity_label(sensitivity: Sensitivity) -> &'static str {
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
    fn memory_list_page_clamps_limit_and_offset() {
        assert_eq!(MemoryListPage::new(None, None).limit(), 50);
        assert_eq!(MemoryListPage::new(Some(0), Some(-10)).limit(), 1);
        assert_eq!(MemoryListPage::new(Some(500), Some(10)).limit(), 200);
        assert_eq!(MemoryListPage::new(Some(20), Some(-10)).offset(), 0);
        assert_eq!(MemoryListPage::new(Some(20), Some(10)).offset(), 10);
    }

    #[test]
    fn memory_scope_kind_maps_protocol_labels_to_core_scope_kind() {
        assert_eq!(MemoryScopeKind::from_label("user").unwrap().as_scope_kind(), ScopeKind::User);
        assert_eq!(MemoryScopeKind::from_label("team").unwrap().as_scope_kind(), ScopeKind::Team);
        assert_eq!(MemoryScopeKind::from_label("project").unwrap().as_scope_kind(), ScopeKind::Project);
        assert_eq!(MemoryScopeKind::from_label("org"), None);
    }

    #[test]
    fn memory_scope_target_policy_defaults_user_and_requires_group_scope_id() {
        let user_id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
        assert_eq!(
            MemoryScopeTargetPolicy::resolve(MemoryScopeKind::User, None, user_id).unwrap(),
            (ScopeKind::User, user_id)
        );
        assert!(MemoryScopeTargetPolicy::resolve(MemoryScopeKind::Project, None, user_id).is_err());
    }

    #[test]
    fn memory_reclassification_policy_plans_reclassification_and_rejections() {
        assert_eq!(MemoryReclassificationPolicy::resolve_current_scope_kind("team").unwrap(), ContextScopeKind::Team);
        assert!(MemoryReclassificationPolicy::resolve_current_scope_kind("workspace").is_err());

        let plan = MemoryReclassificationPolicy::plan(MemoryReclassificationRequest {
            current_scope_kind: "user",
            target_scope_kind: ScopeKind::Team,
            sensitivity: "internal",
            content_redacted: false,
            confirm_sensitive: false,
            confirm_expansion: true,
        })
        .expect("scope labels should be valid");
        let MemoryReclassificationPlan::Approved(decision) = plan else {
            panic!("confirmed expansion should be approved");
        };
        let audit = MemoryReclassifiedAudit::from_decision(decision, "internal", false);
        assert_eq!(audit.audit_action(), "governance.context.memory.reclassified");
        let payload = audit.audit_payload();
        assert_eq!(payload["from_scope_kind"], "user");
        assert_eq!(payload["to_scope_kind"], "team");
        assert_eq!(payload["sensitivity"], "internal");
        assert_eq!(payload["content_redacted"], false);

        let plan = MemoryReclassificationPolicy::plan(MemoryReclassificationRequest {
            current_scope_kind: "user",
            target_scope_kind: ScopeKind::Project,
            sensitivity: "internal",
            content_redacted: false,
            confirm_sensitive: false,
            confirm_expansion: false,
        })
        .expect("scope labels should be valid");
        let MemoryReclassificationPlan::Rejected(rejection) = plan else {
            panic!("unconfirmed expansion should be rejected");
        };
        assert_eq!(rejection.audit_action(), "governance.context.memory.scope_expansion_rejected");
        assert_eq!(rejection.audit_payload()["reason"], "confirmation_required");

        let plan = MemoryReclassificationPolicy::plan(MemoryReclassificationRequest {
            current_scope_kind: "user",
            target_scope_kind: ScopeKind::Team,
            sensitivity: "secret_detected",
            content_redacted: false,
            confirm_sensitive: false,
            confirm_expansion: true,
        })
        .expect("scope labels should be valid");
        let MemoryReclassificationPlan::Rejected(rejection) = plan else {
            panic!("unredacted sensitive scope change should be rejected");
        };
        assert_eq!(rejection.audit_action(), "governance.context.memory.sensitive_scope_change_rejected");
        assert!(matches!(rejection.into_app_error().kind, ErrorKind::Unprocessable(_)));

        assert!(matches!(
            MemoryReclassificationPolicy::plan(MemoryReclassificationRequest {
                current_scope_kind: "user",
                target_scope_kind: ScopeKind::Team,
                sensitivity: "secret_detected",
                content_redacted: false,
                confirm_sensitive: true,
                confirm_expansion: true,
            })
            .expect("scope labels should be valid"),
            MemoryReclassificationPlan::Approved(_)
        ));
        assert!(matches!(
            MemoryReclassificationPolicy::plan(MemoryReclassificationRequest {
                current_scope_kind: "user",
                target_scope_kind: ScopeKind::Team,
                sensitivity: "secret_detected",
                content_redacted: true,
                confirm_sensitive: false,
                confirm_expansion: true,
            })
            .expect("scope labels should be valid"),
            MemoryReclassificationPlan::Approved(_)
        ));
    }

    #[test]
    fn memory_mutation_access_policy_prefers_owner_then_manager_scope() {
        let owner = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let actor = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
        let scope_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();

        assert_eq!(MemoryMutationAccessPolicy::plan(owner, owner, "user", owner), MemoryMutationAccess::Allowed);
        assert_eq!(
            MemoryMutationAccessPolicy::plan(owner, actor, "team", scope_id),
            MemoryMutationAccess::RequiresManager(MemoryMutationManagerCheck::Team(scope_id))
        );
        assert_eq!(
            MemoryMutationAccessPolicy::plan(owner, actor, "project", scope_id),
            MemoryMutationAccess::RequiresManager(MemoryMutationManagerCheck::Project(scope_id))
        );
        assert_eq!(MemoryMutationAccessPolicy::plan(owner, actor, "user", owner), MemoryMutationAccess::Forbidden);
        assert_eq!(
            MemoryMutationAccessPolicy::plan(owner, actor, "workspace", scope_id),
            MemoryMutationAccess::Forbidden
        );
    }

    #[test]
    fn memory_mutation_access_policy_maps_manager_checks_to_forbidden() {
        assert!(MemoryMutationAccessPolicy::ensure_manager_authorized(true).is_ok());
        assert!(matches!(
            MemoryMutationAccessPolicy::ensure_manager_authorized(false).unwrap_err().kind,
            ErrorKind::Forbidden
        ));
    }

    #[test]
    fn memory_title_trims_and_checks_bounds() {
        assert_eq!(MemoryTitle::parse(" title ").unwrap().value(), "title");
        assert!(MemoryTitle::parse("").is_err());
        assert!(MemoryTitle::parse("   ").is_err());
        assert!(MemoryTitle::parse(&"x".repeat(256)).is_err());
    }

    #[test]
    fn memory_visibility_defaults_and_rejects_unsupported_values() {
        assert_eq!(MemoryVisibility::parse(None).unwrap().as_str(), "shared");
        assert_eq!(MemoryVisibility::parse(Some("private")).unwrap().as_str(), "private");
        assert_eq!(MemoryVisibility::parse(Some("shared")).unwrap().as_str(), "shared");
        assert!(MemoryVisibility::parse(Some("team")).is_err());
    }

    #[test]
    fn memory_ttl_requires_future_expiry_when_present() {
        let now = Utc::now();

        assert!(MemoryTtlPolicy::validate(None, now).is_ok());
        assert!(MemoryTtlPolicy::validate(Some(now + Duration::seconds(1)), now).is_ok());
        assert!(MemoryTtlPolicy::validate(Some(now), now).is_err());
        assert!(MemoryTtlPolicy::validate(Some(now - Duration::seconds(1)), now).is_err());
    }

    #[test]
    fn memory_confidence_accepts_unit_interval_only() {
        assert!(MemoryConfidencePolicy::validate(None).is_ok());
        assert!(MemoryConfidencePolicy::validate(Some(0.0)).is_ok());
        assert!(MemoryConfidencePolicy::validate(Some(1.0)).is_ok());
        assert!(MemoryConfidencePolicy::validate(Some(-0.1)).is_err());
        assert!(MemoryConfidencePolicy::validate(Some(1.1)).is_err());
    }

    #[test]
    fn memory_content_read_audit_owns_action_and_payload() {
        let audit = MemoryContentReadAudit::new("team", "confidential", true);
        let payload = audit.audit_payload();

        assert_eq!(memory_audit_resource_type(), "memory_item");
        assert_eq!(audit.audit_action(), "governance.context.memory.content_read");
        assert_eq!(payload["scope_kind"], "team");
        assert_eq!(payload["sensitivity"], "confidential");
        assert_eq!(payload["content_redacted"], true);

        let event = memory_audit_event(audit.audit_action(), payload);
        assert_eq!(event.action, "governance.context.memory.content_read");
        assert_eq!(event.resource_type, "memory_item");
        assert_eq!(event.resource_id, None);
        assert_eq!(event.payload["scope_kind"], "team");
        assert_eq!(event.ip_address, None);
    }

    #[test]
    fn memory_created_audit_owns_action_and_payload() {
        let audit = MemoryCreatedAudit::new(
            "project",
            "shared",
            "secret_detected",
            true,
            json!({
                "sensitivity": "secret_detected",
                "redacted": true
            }),
        );
        let payload = audit.audit_payload();

        assert_eq!(audit.audit_action(), "governance.context.memory.created");
        assert_eq!(payload["scope_kind"], "project");
        assert_eq!(payload["visibility"], "shared");
        assert_eq!(payload["sensitivity"], "secret_detected");
        assert_eq!(payload["content_redacted"], true);
        assert_eq!(payload["classification"]["sensitivity"], "secret_detected");
        assert_eq!(payload["classification"]["redacted"], true);
    }

    #[test]
    fn memory_updated_audit_owns_action_and_payload() {
        let audit = MemoryUpdatedAudit::new("team", "private", "internal", true, false);
        let payload = audit.audit_payload();

        assert_eq!(audit.audit_action(), "governance.context.memory.updated");
        assert_eq!(payload["scope_kind"], "team");
        assert_eq!(payload["visibility"], "private");
        assert_eq!(payload["sensitivity"], "internal");
        assert_eq!(payload["content_changed"], true);
        assert_eq!(payload["content_redacted"], false);
    }

    #[test]
    fn memory_revoked_audit_owns_action_and_payload() {
        let audit = MemoryRevokedAudit::new("user", "confidential");
        let payload = audit.audit_payload();

        assert_eq!(audit.audit_action(), "governance.context.memory.revoked");
        assert_eq!(payload["scope_kind"], "user");
        assert_eq!(payload["sensitivity"], "confidential");
    }

    #[test]
    fn memory_ttl_extended_audit_owns_action_and_payload() {
        let ttl = Utc::now() + Duration::days(7);
        let audit = MemoryTtlExtendedAudit::new("project", Some(ttl));
        let payload = audit.audit_payload();

        assert_eq!(audit.audit_action(), "governance.context.memory.ttl_extended");
        assert_eq!(payload["scope_kind"], "project");
        assert_eq!(payload["ttl_expires_at"], serde_json::to_value(ttl).unwrap());
    }

    #[test]
    fn memory_ttl_extended_audit_allows_clearing_ttl() {
        let audit = MemoryTtlExtendedAudit::new("team", None);
        let payload = audit.audit_payload();

        assert_eq!(payload["scope_kind"], "team");
        assert!(payload["ttl_expires_at"].is_null());
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
    fn memory_content_policy_rejects_empty_content() {
        assert!(MemoryContentPolicy::prepare("   ", false).is_err());
    }

    #[test]
    fn memory_content_policy_requests_auditable_secret_rejection() {
        let secret = synthetic_assigned_secret();
        let secret_fragment = synthetic_secret_fragment();

        let decision = MemoryContentPolicy::prepare(&secret, false).expect("classification should succeed");

        let MemoryContentDecision::Rejected(rejection) = decision else {
            panic!("secret should require auditable rejection");
        };
        assert_eq!(rejection.audit_action(), "governance.context.memory.rejected");
        let payload = rejection.audit_payload("create");
        assert_eq!(payload["operation"], "create");
        assert_eq!(payload["reason"], "secret_detected");
        assert!(payload["matched_patterns"].as_array().is_some_and(|items| !items.is_empty()));
        assert!(!payload["redacted_preview"].as_str().unwrap_or_default().contains(&secret_fragment));
    }

    #[test]
    fn memory_content_policy_redacts_confirmed_secret_for_storage() {
        let secret = synthetic_assigned_secret();
        let secret_fragment = synthetic_secret_fragment();

        let decision = MemoryContentPolicy::prepare(&secret, true).expect("classification should succeed");

        let MemoryContentDecision::Prepared(prepared) = decision else {
            panic!("confirmed redaction should prepare content");
        };
        assert!(prepared.content_redacted);
        assert_eq!(prepared.sensitivity, "secret_detected");
        assert!(!prepared.content.contains(&secret_fragment));
        assert_eq!(prepared.audit_payload["redacted"], true);
    }

    #[test]
    fn memory_content_policy_keeps_clean_content_visible() {
        let decision =
            MemoryContentPolicy::prepare("  deployment note  ", true).expect("classification should succeed");

        let MemoryContentDecision::Prepared(prepared) = decision else {
            panic!("clean content should prepare");
        };
        assert_eq!(prepared.content, "deployment note");
        assert!(!prepared.content_redacted);
        assert_eq!(prepared.sensitivity, "internal");
        assert_eq!(prepared.audit_payload["redacted"], false);
    }
}
