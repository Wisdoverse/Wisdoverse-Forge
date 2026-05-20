//! Governed context sensitivity and audit-detail policies.

use std::collections::HashSet;
use std::sync::OnceLock;

use agentforge_core::{AppError, AppResult, ErrorKind, ScopeKind};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const GOVERNANCE_CONTEXT_ACTION_PREFIX: &str = "governance.context.";
pub const MAX_CLASSIFICATION_INPUT_BYTES: usize = 256 * 1024;
pub const HIGH_ENTROPY_MIN_BYTES: usize = 96;
pub const HIGH_ENTROPY_THRESHOLD_BITS: f64 = 4.8;

const REDACTED_MARKER: &str = "[REDACTED]";
const REDACTED_OVERSIZE_PREVIEW: &str = "[REDACTED: input exceeded classifier limit]";
const PREVIEW_MAX_CHARS: usize = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Public,
    Internal,
    Confidential,
    SecretDetected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretPattern {
    AwsAccessKeyId,
    AzureSecret,
    GcpServiceAccount,
    GithubToken,
    Jwt,
    StripeSecretKey,
    GenericHexToken,
    GenericAssignedSecret,
    HighEntropy,
    InputTooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensitivityClassification {
    pub sensitivity: Sensitivity,
    pub matched_patterns: Vec<SecretPattern>,
    pub redacted_preview: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextScopeKind {
    User,
    Team,
    Project,
    Org,
}

impl ContextScopeKind {
    pub fn from_label(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "team" => Some(Self::Team),
            "project" => Some(Self::Project),
            "org" => Some(Self::Org),
            _ => None,
        }
    }

    pub fn from_scope_kind(value: ScopeKind) -> Self {
        match value {
            ScopeKind::User => Self::User,
            ScopeKind::Team => Self::Team,
            ScopeKind::Project => Self::Project,
        }
    }

    pub fn as_label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Team => "team",
            Self::Project => "project",
            Self::Org => "org",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::User => 0,
            Self::Team => 1,
            Self::Project => 2,
            Self::Org => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeExpansionRequest {
    pub from_kind: ContextScopeKind,
    pub to_kind: ContextScopeKind,
    pub confirm_expansion: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeExpansionDecision {
    pub expanded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeExpansionRejectionReason {
    ConfirmationRequired,
    OrgWideUnsupported,
}

impl ScopeExpansionRejectionReason {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::ConfirmationRequired => "confirmation_required",
            Self::OrgWideUnsupported => "org_wide_unsupported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeExpansionRejection {
    pub from_kind: ContextScopeKind,
    pub to_kind: ContextScopeKind,
    pub reason: ScopeExpansionRejectionReason,
}

impl ScopeExpansionRejection {
    pub fn into_app_error(self) -> AppError {
        match self.reason {
            ScopeExpansionRejectionReason::ConfirmationRequired => ErrorKind::Unprocessable(format!(
                "scope expansion from {} to {} requires confirm_expansion=true",
                self.from_kind.as_label(),
                self.to_kind.as_label()
            ))
            .into(),
            ScopeExpansionRejectionReason::OrgWideUnsupported => {
                ErrorKind::Unprocessable("org-wide context scope is out of scope for MVP".into()).into()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContextAuditEvent<'a> {
    pub action: &'a str,
    pub resource_type: &'a str,
    pub resource_id: Option<Uuid>,
    pub payload: Value,
    pub ip_address: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GovernanceAuditQueryPolicy<'a> {
    pub event_prefix: Option<&'a str>,
    pub event_type: Option<&'a str>,
    pub item_kind: Option<&'a str>,
    pub scope_kind: Option<&'a str>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GovernanceAuditQueryParams {
    pub(crate) event_type: Option<String>,
    pub(crate) event_prefix: Option<String>,
    pub(crate) item_kind: Option<String>,
    pub(crate) scope_kind: Option<String>,
    pub(crate) scope_id: Option<Uuid>,
    pub(crate) user_id: Option<Uuid>,
    pub(crate) from: Option<DateTime<Utc>>,
    pub(crate) to: Option<DateTime<Utc>>,
    pub(crate) redact_secrets: Option<bool>,
    pub(crate) limit: Option<i64>,
    pub(crate) offset: Option<i64>,
}

impl GovernanceAuditQueryParams {
    pub(crate) fn apply_export_defaults(&mut self) {
        self.redact_secrets = Some(self.redact_secrets.unwrap_or(true));
        self.limit = Some(self.limit.unwrap_or(500).clamp(1, 500));
    }

    fn policy(&self) -> GovernanceAuditQueryPolicy<'_> {
        GovernanceAuditQueryPolicy {
            event_prefix: self.event_prefix.as_deref(),
            event_type: self.event_type.as_deref(),
            item_kind: self.item_kind.as_deref(),
            scope_kind: self.scope_kind.as_deref(),
            from: self.from,
            to: self.to,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GovernanceAuditResponse {
    pub(crate) entries: Vec<GovernanceAuditEntry>,
    pub(crate) query: GovernanceAuditQuery,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GovernanceAuditQuery {
    pub(crate) event_prefix: String,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
    pub(crate) redacted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GovernanceAuditEntry {
    pub(crate) id: Uuid,
    pub(crate) event_type: String,
    pub(crate) actor_user_id: Option<Uuid>,
    pub(crate) item_kind: Option<String>,
    pub(crate) scope_kind: Option<String>,
    pub(crate) scope_id: Option<Uuid>,
    pub(crate) raw_item_id: Option<Uuid>,
    pub(crate) audit_subject_hash: String,
    pub(crate) resource_type: String,
    pub(crate) resource_id: Option<Uuid>,
    pub(crate) details: Value,
    pub(crate) details_redacted: bool,
    pub(crate) tamper_status: AuditTamperStatus,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuditTamperStatus {
    NotConfigured,
    Valid,
    Invalid,
}

pub(crate) fn governance_audit_response(data: GovernanceAuditResponse) -> Value {
    serde_json::json!({ "ok": true, "data": data })
}

pub(crate) struct ContextGovernancePolicy;

impl ContextGovernancePolicy {
    pub(crate) fn classify_sensitivity(content: &str) -> SensitivityClassification {
        if content.len() > MAX_CLASSIFICATION_INPUT_BYTES {
            return SensitivityClassification {
                sensitivity: Sensitivity::SecretDetected,
                matched_patterns: vec![SecretPattern::InputTooLarge],
                redacted_preview: Some(REDACTED_OVERSIZE_PREVIEW.to_string()),
            };
        }

        let mut matched = Vec::new();
        let mut seen = HashSet::new();
        let mut redacted = content.to_string();

        for (pattern, regex) in secret_patterns() {
            if regex.is_match(content) {
                if seen.insert(*pattern) {
                    matched.push(*pattern);
                }
                redacted = regex.replace_all(&redacted, REDACTED_MARKER).to_string();
            }
        }

        if matched.is_empty()
            && content.len() >= HIGH_ENTROPY_MIN_BYTES
            && shannon_entropy(content.as_bytes()) >= HIGH_ENTROPY_THRESHOLD_BITS
        {
            matched.push(SecretPattern::HighEntropy);
            redacted = redact_high_entropy_tokens(content);
        }

        if matched.is_empty() {
            SensitivityClassification {
                sensitivity: Sensitivity::Internal,
                matched_patterns: matched,
                redacted_preview: None,
            }
        } else {
            SensitivityClassification {
                sensitivity: Sensitivity::SecretDetected,
                matched_patterns: matched,
                redacted_preview: Some(preview(&redacted)),
            }
        }
    }

    pub(crate) fn gate_scope_expansion(
        request: ScopeExpansionRequest,
    ) -> Result<ScopeExpansionDecision, ScopeExpansionRejection> {
        if request.to_kind == ContextScopeKind::Org && request.from_kind != ContextScopeKind::Org {
            return Err(ScopeExpansionRejection {
                from_kind: request.from_kind,
                to_kind: request.to_kind,
                reason: ScopeExpansionRejectionReason::OrgWideUnsupported,
            });
        }

        let expanded = request.to_kind.rank() > request.from_kind.rank();
        if expanded && !request.confirm_expansion {
            return Err(ScopeExpansionRejection {
                from_kind: request.from_kind,
                to_kind: request.to_kind,
                reason: ScopeExpansionRejectionReason::ConfirmationRequired,
            });
        }

        Ok(ScopeExpansionDecision { expanded })
    }

    pub(crate) fn validate_audit_event(event: &ContextAuditEvent<'_>) -> AppResult<()> {
        if !event.action.starts_with(GOVERNANCE_CONTEXT_ACTION_PREFIX) {
            return Err(ErrorKind::Validation(format!(
                "governance audit action must start with {GOVERNANCE_CONTEXT_ACTION_PREFIX}"
            ))
            .into());
        }
        if event.resource_type.trim().is_empty() {
            return Err(ErrorKind::Validation("governance audit resource_type must not be empty".into()).into());
        }
        validate_audit_details(&event.payload)
    }

    pub(crate) fn validate_audit_query(query: GovernanceAuditQueryPolicy<'_>) -> AppResult<()> {
        if matches!(query.event_prefix, Some(prefix) if !prefix.starts_with(GOVERNANCE_CONTEXT_ACTION_PREFIX)) {
            return Err(ErrorKind::Validation("event_prefix must stay under governance.context.".into()).into());
        }
        if matches!(query.event_type, Some(event_type) if !event_type.starts_with(GOVERNANCE_CONTEXT_ACTION_PREFIX)) {
            return Err(ErrorKind::Validation("event_type must start with governance.context.".into()).into());
        }
        if matches!(query.item_kind, Some(item_kind) if !matches!(item_kind, "memory" | "skill")) {
            return Err(ErrorKind::Validation("item_kind must be memory or skill".into()).into());
        }
        if matches!(query.scope_kind, Some(scope_kind) if !matches!(scope_kind, "org" | "user" | "workspace" | "team" | "project"))
        {
            return Err(ErrorKind::Validation("unsupported scope_kind".into()).into());
        }
        if matches!((query.from, query.to), (Some(from), Some(to)) if from >= to) {
            return Err(ErrorKind::Validation("from must be earlier than to".into()).into());
        }
        Ok(())
    }

    pub(crate) fn validate_governance_audit_query_params(query: &GovernanceAuditQueryParams) -> AppResult<()> {
        Self::validate_audit_query(query.policy())
    }

    pub(crate) fn redact_audit_details(value: Value) -> (Value, bool) {
        match value {
            Value::Object(map) => {
                let mut redacted = false;
                let mut out = serde_json::Map::with_capacity(map.len());
                for (key, value) in map {
                    if is_export_secret_key(&key) {
                        out.insert(key, Value::String(REDACTED_MARKER.to_string()));
                        redacted = true;
                        continue;
                    }
                    let (value, nested) = Self::redact_audit_details(value);
                    redacted |= nested;
                    out.insert(key, value);
                }
                (Value::Object(out), redacted)
            }
            Value::Array(items) => {
                let mut redacted = false;
                let items = items
                    .into_iter()
                    .map(|item| {
                        let (item, nested) = Self::redact_audit_details(item);
                        redacted |= nested;
                        item
                    })
                    .collect();
                (Value::Array(items), redacted)
            }
            Value::String(raw) => {
                if matches!(Self::classify_sensitivity(&raw).sensitivity, Sensitivity::SecretDetected) {
                    (Value::String(REDACTED_MARKER.to_string()), true)
                } else {
                    (Value::String(raw), false)
                }
            }
            other => (other, false),
        }
    }
}

fn secret_patterns() -> &'static [(SecretPattern, Regex)] {
    static PATTERNS: OnceLock<Vec<(SecretPattern, Regex)>> = OnceLock::new();
    PATTERNS
        .get_or_init(|| {
            vec![
                (SecretPattern::AwsAccessKeyId, compile_regex(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b")),
                (
                    SecretPattern::GithubToken,
                    compile_regex(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{20,}\b|\bgithub_pat_[A-Za-z0-9_]{20,}\b"),
                ),
                (SecretPattern::StripeSecretKey, compile_regex(r"\bsk_(?:live|test)_[A-Za-z0-9]{16,}\b")),
                (
                    SecretPattern::GenericHexToken,
                    compile_regex(r"(?i)\b[a-f0-9]{32,}\b"),
                ),
                (
                    SecretPattern::Jwt,
                    compile_regex(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{16,}\b"),
                ),
                (
                    SecretPattern::GcpServiceAccount,
                    compile_regex(r#"(?is)"type"\s*:\s*"service_account"[\s\S]{0,4000}"private_key"\s*:\s*"[^"]+""#),
                ),
                (
                    SecretPattern::GcpServiceAccount,
                    compile_regex(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.iam\.gserviceaccount\.com\b"),
                ),
                (
                    SecretPattern::AzureSecret,
                    compile_regex(r"(?i)\b(?:AccountKey|SharedAccessKey|azure[_-]?secret)\s*=\s*[A-Za-z0-9+/=]{20,}"),
                ),
                (
                    SecretPattern::GenericAssignedSecret,
                    compile_regex(
                        r#"(?ix)\b(?:api[_-]?key|access[_-]?token|auth[_-]?token|token|secret|password)\b\s*[:=]\s*["']?[A-Za-z0-9_./+=-]{16,}["']?"#,
                    ),
                ),
            ]
        })
        .as_slice()
}

fn compile_regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("context governance regex must compile")
}

fn preview(value: &str) -> String {
    let mut out: String = value.chars().take(PREVIEW_MAX_CHARS).collect();
    if value.chars().count() > PREVIEW_MAX_CHARS {
        out.push_str("...");
    }
    out
}

fn redact_high_entropy_tokens(content: &str) -> String {
    let redacted = content
        .split_whitespace()
        .map(|token| {
            if token.len() >= HIGH_ENTROPY_MIN_BYTES / 2
                && shannon_entropy(token.as_bytes()) >= HIGH_ENTROPY_THRESHOLD_BITS
            {
                REDACTED_MARKER
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    preview(&redacted)
}

fn shannon_entropy(bytes: &[u8]) -> f64 {
    if bytes.is_empty() {
        return 0.0;
    }

    let mut counts = [0_usize; 256];
    for byte in bytes {
        counts[*byte as usize] += 1;
    }

    let len = bytes.len() as f64;
    counts
        .iter()
        .filter(|count| **count > 0)
        .map(|count| {
            let probability = *count as f64 / len;
            -probability * probability.log2()
        })
        .sum()
}

fn validate_audit_details(value: &Value) -> AppResult<()> {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                if is_secret_bearing_key(key) {
                    return Err(
                        ErrorKind::Validation(format!("secret-bearing audit detail `{key}` is not allowed")).into()
                    );
                }
                validate_audit_details(nested)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                validate_audit_details(item)?;
            }
        }
        Value::String(raw) => {
            if matches!(ContextGovernancePolicy::classify_sensitivity(raw).sensitivity, Sensitivity::SecretDetected) {
                return Err(ErrorKind::Validation("secret-bearing audit detail value is not allowed".into()).into());
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

fn is_secret_bearing_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace(['-', ' '], "_");
    matches!(
        normalized.as_str(),
        "content"
            | "raw_content"
            | "api_key"
            | "apikey"
            | "access_token"
            | "auth_token"
            | "token"
            | "secret"
            | "password"
            | "private_key"
            | "webhook_url"
            | "provider_token"
    )
}

fn is_export_secret_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace(['-', ' '], "_");
    matches!(
        normalized.as_str(),
        "secret"
            | "secrets"
            | "token"
            | "access_token"
            | "refresh_token"
            | "api_key"
            | "apikey"
            | "password"
            | "private_key"
            | "credential"
            | "credentials"
            | "hmac_key"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audit_event(payload: Value) -> ContextAuditEvent<'static> {
        ContextAuditEvent {
            action: "governance.context.approved",
            resource_type: "context_candidate",
            resource_id: None,
            payload,
            ip_address: None,
        }
    }

    #[test]
    fn sensitivity_detects_and_redacts_known_tokens() {
        let classification =
            ContextGovernancePolicy::classify_sensitivity("token=ghp_abcdefghijklmnopqrstuvwxyz123456");

        assert_eq!(classification.sensitivity, Sensitivity::SecretDetected);
        assert!(classification.matched_patterns.contains(&SecretPattern::GithubToken));
        let preview = classification.redacted_preview.as_deref().expect("redacted preview");
        assert!(preview.contains("[REDACTED]"));
        assert!(!preview.contains("abcdefghijklmnopqrstuvwxyz123456"));
    }

    #[test]
    fn sensitivity_marks_plain_text_internal() {
        let classification = ContextGovernancePolicy::classify_sensitivity("normal project note");

        assert_eq!(classification.sensitivity, Sensitivity::Internal);
        assert!(classification.matched_patterns.is_empty());
        assert!(classification.redacted_preview.is_none());
    }

    #[test]
    fn sensitivity_rejects_oversize_input() {
        let content = "x".repeat(MAX_CLASSIFICATION_INPUT_BYTES + 1);
        let classification = ContextGovernancePolicy::classify_sensitivity(&content);

        assert_eq!(classification.sensitivity, Sensitivity::SecretDetected);
        assert_eq!(classification.matched_patterns, vec![SecretPattern::InputTooLarge]);
        assert_eq!(classification.redacted_preview.as_deref(), Some(REDACTED_OVERSIZE_PREVIEW));
    }

    #[test]
    fn scope_expansion_requires_confirmation() {
        let rejection = ContextGovernancePolicy::gate_scope_expansion(ScopeExpansionRequest {
            from_kind: ContextScopeKind::User,
            to_kind: ContextScopeKind::Project,
            confirm_expansion: false,
        })
        .unwrap_err();

        assert_eq!(rejection.reason, ScopeExpansionRejectionReason::ConfirmationRequired);
        let decision = ContextGovernancePolicy::gate_scope_expansion(ScopeExpansionRequest {
            from_kind: ContextScopeKind::User,
            to_kind: ContextScopeKind::Project,
            confirm_expansion: true,
        })
        .unwrap();
        assert!(decision.expanded);
    }

    #[test]
    fn scope_expansion_rejects_org_wide_target() {
        let rejection = ContextGovernancePolicy::gate_scope_expansion(ScopeExpansionRequest {
            from_kind: ContextScopeKind::Project,
            to_kind: ContextScopeKind::Org,
            confirm_expansion: true,
        })
        .unwrap_err();

        assert_eq!(rejection.reason, ScopeExpansionRejectionReason::OrgWideUnsupported);
    }

    #[test]
    fn audit_event_rejects_invalid_action_and_resource_type() {
        let mut event = audit_event(serde_json::json!({ "kind": "safe" }));
        event.action = "context.approved";
        assert!(ContextGovernancePolicy::validate_audit_event(&event).is_err());

        event.action = "governance.context.approved";
        event.resource_type = " ";
        assert!(ContextGovernancePolicy::validate_audit_event(&event).is_err());
    }

    #[test]
    fn audit_event_rejects_secret_keys_and_values() {
        assert!(
            ContextGovernancePolicy::validate_audit_event(&audit_event(serde_json::json!({
                "token": "redacted"
            })))
            .is_err()
        );
        assert!(
            ContextGovernancePolicy::validate_audit_event(&audit_event(serde_json::json!({
                "details": ["ghp_abcdefghijklmnopqrstuvwxyz123456"]
            })))
            .is_err()
        );
    }

    #[test]
    fn audit_event_accepts_safe_nested_payload() {
        let event = audit_event(serde_json::json!({
            "candidate_id": "c1",
            "scope": { "kind": "project", "label": "docs" },
            "changes": [1, 2, 3]
        }));

        assert!(ContextGovernancePolicy::validate_audit_event(&event).is_ok());
    }

    #[test]
    fn audit_query_rejects_non_governance_prefix() {
        let query = GovernanceAuditQueryPolicy {
            event_prefix: Some("system."),
            event_type: None,
            item_kind: None,
            scope_kind: None,
            from: None,
            to: None,
        };

        assert!(ContextGovernancePolicy::validate_audit_query(query).is_err());
    }

    #[test]
    fn audit_export_redacts_secret_bearing_details() {
        let (value, redacted) = ContextGovernancePolicy::redact_audit_details(serde_json::json!({
            "classification": {
                "token": "github-token-placeholder"
            },
            "safe": "internal"
        }));

        assert!(redacted);
        assert_eq!(value["classification"]["token"], REDACTED_MARKER);
        assert_eq!(value["safe"], "internal");
    }
}
