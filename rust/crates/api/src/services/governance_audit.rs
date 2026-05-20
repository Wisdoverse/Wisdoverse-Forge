//! Governance audit projection service.

use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

use agentforge_core::{AppResult, ErrorKind, TenantScope};

use crate::domain::context_governance::{
    AuditTamperStatus, ContextGovernancePolicy, GovernanceAuditEntry, GovernanceAuditQuery, GovernanceAuditQueryParams,
    GovernanceAuditResponse,
};
pub(crate) use crate::domain::context_governance::{
    GovernanceAuditQueryParams as QueryParams, governance_audit_response,
};
use crate::repositories::audit::AuditRepository;
use crate::repositories::governance_audit::{
    GOVERNANCE_CONTEXT_AUDIT_PREFIX, GovernanceAuditFilter, GovernanceAuditRepository, GovernanceAuditRow,
    normalize_limit,
};

type HmacSha256 = Hmac<Sha256>;

pub struct GovernanceAuditService {
    repo: GovernanceAuditRepository,
    audit: AuditRepository,
    hmac_key: Vec<u8>,
}

impl GovernanceAuditService {
    pub fn new(repo: GovernanceAuditRepository, audit: AuditRepository, hmac_key: Vec<u8>) -> Self {
        Self { repo, audit, hmac_key }
    }

    pub(crate) async fn list(
        &self,
        scope: &TenantScope,
        role: &str,
        query: GovernanceAuditQueryParams,
    ) -> AppResult<GovernanceAuditResponse> {
        self.load_projection(scope, role, query).await
    }

    pub(crate) async fn export(
        &self,
        scope: &TenantScope,
        role: &str,
        mut query: GovernanceAuditQueryParams,
    ) -> AppResult<GovernanceAuditResponse> {
        query.apply_export_defaults();

        let data = self.load_projection(scope, role, query).await?;
        self.audit
            .create(
                scope.org_id(),
                Some(scope.user_id()),
                "governance.context.audit.exported",
                "governance_audit_export",
                None,
                &json!({
                    "entry_count": data.entries.len(),
                    "redact_secrets": data.query.redacted,
                    "event_prefix": data.query.event_prefix,
                    "limit": data.query.limit,
                    "offset": data.query.offset
                }),
                None,
            )
            .await?;

        Ok(data)
    }

    async fn load_projection(
        &self,
        scope: &TenantScope,
        role: &str,
        query: GovernanceAuditQueryParams,
    ) -> AppResult<GovernanceAuditResponse> {
        ContextGovernancePolicy::validate_governance_audit_query_params(&query)?;
        let event_prefix = query.event_prefix.as_deref().unwrap_or(GOVERNANCE_CONTEXT_AUDIT_PREFIX);
        let redact = query.redact_secrets.unwrap_or(true);
        let include_org_wide = is_admin_role(role);
        let rows = self
            .repo
            .list(
                scope,
                GovernanceAuditFilter {
                    event_type: query.event_type.as_deref(),
                    event_prefix: Some(event_prefix),
                    item_kind: query.item_kind.as_deref(),
                    scope_kind: query.scope_kind.as_deref(),
                    scope_id: query.scope_id,
                    user_id: query.user_id,
                    from: query.from,
                    to: query.to,
                    limit: query.limit,
                    offset: query.offset,
                },
                include_org_wide,
            )
            .await?;

        let entries = rows.into_iter().map(|row| project_row(row, &self.hmac_key, redact)).collect();
        Ok(GovernanceAuditResponse {
            entries,
            query: GovernanceAuditQuery {
                event_prefix: event_prefix.to_string(),
                limit: normalize_limit(query.limit),
                offset: query.offset.unwrap_or(0).max(0),
                redacted: redact,
            },
        })
    }
}

pub(crate) fn governance_audit_hmac_key(is_production: bool, encryption_key: Option<[u8; 32]>) -> AppResult<Vec<u8>> {
    if let Ok(raw) = std::env::var("CONTEXT_AUDIT_HMAC_KEY") {
        if raw.trim().is_empty() {
            return Err(ErrorKind::Validation("CONTEXT_AUDIT_HMAC_KEY is empty".into()).into());
        }
        if raw.len() == 64 && raw.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return hex::decode(raw)
                .map_err(|err| ErrorKind::Validation(format!("invalid CONTEXT_AUDIT_HMAC_KEY: {err}")).into());
        }
        return Ok(raw.into_bytes());
    }

    if let Some(key) = encryption_key {
        return Ok(key.to_vec());
    }

    if is_production {
        return Err(ErrorKind::Validation("CONTEXT_AUDIT_HMAC_KEY or LLM_ENCRYPTION_KEY is required".into()).into());
    }

    Ok(b"agentforge-dev-governance-audit-key".to_vec())
}

fn project_row(row: GovernanceAuditRow, key: &[u8], redact: bool) -> GovernanceAuditEntry {
    let hash_subject = row.subject_item_id.unwrap_or(row.id);
    let scope_kind = row.subject_scope_kind.clone().unwrap_or_else(|| "unknown".to_string());
    let scope_id = row.subject_scope_id.map(|id| id.to_string()).unwrap_or_else(|| "unknown".to_string());
    let audit_subject_hash = hmac_hex(key, &format!("{hash_subject}|{scope_kind}|{scope_id}"));
    let raw_item_id = row.subject_item_id.filter(|_| row.visible_by_scope);
    let tamper_status = tamper_status(&row, key);
    let (details, details_redacted) =
        if redact { ContextGovernancePolicy::redact_audit_details(row.details) } else { (row.details, false) };

    GovernanceAuditEntry {
        id: row.id,
        event_type: row.event_type,
        actor_user_id: row.actor_user_id,
        item_kind: row.item_kind,
        scope_kind: row.subject_scope_kind,
        scope_id: row.subject_scope_id,
        raw_item_id,
        audit_subject_hash,
        resource_type: row.resource_type,
        resource_id: row.resource_id,
        details,
        details_redacted,
        tamper_status,
        created_at: row.created_at,
    }
}

fn tamper_status(row: &GovernanceAuditRow, key: &[u8]) -> AuditTamperStatus {
    let Some(signature) = row.details.get("hmac_signature").and_then(Value::as_str) else {
        return AuditTamperStatus::NotConfigured;
    };
    let subject = row.subject_item_id.unwrap_or(row.id);
    let expected = hmac_hex(key, &format!("{}|{}|{}", row.event_type, subject, row.created_at.to_rfc3339()));
    if constant_time_eq(signature.as_bytes(), expected.as_bytes()) {
        AuditTamperStatus::Valid
    } else {
        AuditTamperStatus::Invalid
    }
}

fn hmac_hex(key: &[u8], message: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any size");
    mac.update(message.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter().zip(right.iter()).fold(0_u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

fn is_admin_role(role: &str) -> bool {
    matches!(role, "owner" | "admin")
}
