//! Skill domain rules.
//!
//! This module owns pure skill input, lifecycle, and version policies that are
//! independent of repositories, authorization, and audit emission.

use agentforge_core::{AppError, AppResult, ErrorKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::context_governance::{ContextGovernancePolicy, SecretPattern, Sensitivity};

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

/// A skill content mutation that must be audited before returning an application error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillContentRejection {
    matched_patterns: Vec<SecretPattern>,
    redacted_preview: Option<String>,
}

impl SkillContentRejection {
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
    pub(crate) fn validate(name: &str, value: &Value) -> AppResult<()> {
        if value.as_object().is_some() {
            Ok(())
        } else {
            Err(ErrorKind::Validation(format!("{name} must be a JSON object")).into())
        }
    }
}

/// Restore-version request policy.
pub(crate) struct SkillRestoreVersionPolicy;

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
    fn skill_json_object_policy_rejects_non_objects() {
        assert!(SkillJsonObjectPolicy::validate("provenance", &json!({})).is_ok());
        assert!(SkillJsonObjectPolicy::validate("provenance", &json!([])).is_err());
        assert!(SkillJsonObjectPolicy::validate("provenance", &json!("value")).is_err());
    }

    #[test]
    fn skill_restore_version_policy_rejects_non_positive_versions() {
        assert!(SkillRestoreVersionPolicy::validate(1, None).is_ok());
        assert!(SkillRestoreVersionPolicy::validate(1, Some(1)).is_ok());
        assert!(SkillRestoreVersionPolicy::validate(0, None).is_err());
        assert!(SkillRestoreVersionPolicy::validate(1, Some(0)).is_err());
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
