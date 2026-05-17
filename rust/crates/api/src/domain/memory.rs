//! Memory domain rules.
//!
//! This module owns pure memory item input, pagination, and retention policies
//! that are independent of repositories, HTTP route DTOs, and audit emission.

use agentforge_core::{AppError, AppResult, ErrorKind, ScopeKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::context_governance::{ContextGovernancePolicy, ContextScopeKind, SecretPattern, Sensitivity};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

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

impl MemoryReclassificationPolicy {
    pub(crate) fn resolve_current_scope_kind(scope_kind: &str) -> AppResult<ContextScopeKind> {
        ContextScopeKind::from_label(scope_kind)
            .ok_or_else(|| ErrorKind::Validation(format!("unsupported memory scope kind `{scope_kind}`")).into())
    }

    pub(crate) fn ensure_sensitive_scope_change_allowed(
        sensitivity: &str,
        content_redacted: bool,
        confirm_sensitive: bool,
    ) -> AppResult<()> {
        if sensitivity == "secret_detected" && !content_redacted && !confirm_sensitive {
            return Err(ErrorKind::Unprocessable(
                "secret-detected memory requires explicit redaction before scope change".into(),
            )
            .into());
        }
        Ok(())
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
    fn memory_reclassification_policy_validates_scope_and_sensitive_confirmation() {
        assert_eq!(MemoryReclassificationPolicy::resolve_current_scope_kind("team").unwrap(), ContextScopeKind::Team);
        assert!(MemoryReclassificationPolicy::resolve_current_scope_kind("workspace").is_err());
        assert!(
            MemoryReclassificationPolicy::ensure_sensitive_scope_change_allowed("secret_detected", false, false,)
                .is_err()
        );
        assert!(
            MemoryReclassificationPolicy::ensure_sensitive_scope_change_allowed("secret_detected", false, true).is_ok()
        );
        assert!(
            MemoryReclassificationPolicy::ensure_sensitive_scope_change_allowed("secret_detected", true, false).is_ok()
        );
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
