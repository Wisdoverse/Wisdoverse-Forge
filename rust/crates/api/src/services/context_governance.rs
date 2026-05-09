//! Governed context sensitivity classification and same-transaction audit helpers.

use std::collections::HashSet;
use std::sync::OnceLock;

use agentforge_core::{AppError, AppResult, ErrorKind, ScopeKind, TenantScope};
use agentforge_db::entities::AuditLogEntry;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::repositories::audit::AuditRepository;

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

pub struct ContextGovernanceService;

impl ContextGovernanceService {
    pub fn classify_sensitivity(content: &str) -> SensitivityClassification {
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

    pub async fn emit_audit(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        event: ContextAuditEvent<'_>,
    ) -> AppResult<AuditLogEntry> {
        validate_audit_event(&event)?;
        AuditRepository::create_in_tx(
            tx,
            scope.org_id(),
            Some(scope.user_id()),
            event.action,
            event.resource_type,
            event.resource_id,
            &event.payload,
            event.ip_address,
        )
        .await
    }

    pub fn gate_scope_expansion(
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

fn validate_audit_event(event: &ContextAuditEvent<'_>) -> AppResult<()> {
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
            if matches!(ContextGovernanceService::classify_sensitivity(raw).sensitivity, Sensitivity::SecretDetected) {
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
