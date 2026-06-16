//! Governance audit projection service.

use hmac::{Hmac, KeyInit, Mac};
use serde_json::Value;
use sha2::Sha256;

use agentforge_core::{AppConfig, AppResult, TenantScope};
use sqlx::PgPool;

use crate::domain::context_governance::{
    AuditTamperStatus, ContextGovernancePolicy, GovernanceAuditEntry, GovernanceAuditExportedAudit,
    GovernanceAuditHmacKeyPolicy, GovernanceAuditQuery, GovernanceAuditQueryParams, GovernanceAuditResponse,
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

    pub fn from_pool_and_app_config(
        pool: PgPool,
        config: &AppConfig,
        encryption_key: Option<[u8; 32]>,
    ) -> AppResult<Self> {
        Self::with_runtime_config(
            GovernanceAuditRepository::new(pool.clone()),
            AuditRepository::new(pool),
            config.is_production(),
            encryption_key,
        )
    }

    pub fn with_runtime_config(
        repo: GovernanceAuditRepository,
        audit: AuditRepository,
        is_production: bool,
        encryption_key: Option<[u8; 32]>,
    ) -> AppResult<Self> {
        Ok(Self::new(repo, audit, governance_audit_hmac_key(is_production, encryption_key)?))
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
                &GovernanceAuditExportedAudit::from_response(&data).audit_payload(),
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
    GovernanceAuditHmacKeyPolicy::resolve(std::env::var("CONTEXT_AUDIT_HMAC_KEY").ok(), is_production, encryption_key)
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

#[cfg(test)]
mod tests {
    use super::{constant_time_eq, hmac_hex, tamper_status};
    use crate::domain::context_governance::AuditTamperStatus;
    use crate::repositories::governance_audit::GovernanceAuditRow;
    use chrono::{DateTime, Utc};
    use serde_json::json;
    use uuid::Uuid;

    fn fixed_created_at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-02-03T04:05:06+00:00").expect("valid rfc3339").with_timezone(&Utc)
    }

    fn audit_row(
        event_type: &str,
        subject: Uuid,
        created_at: DateTime<Utc>,
        details: serde_json::Value,
    ) -> GovernanceAuditRow {
        GovernanceAuditRow {
            id: subject,
            actor_user_id: None,
            event_type: event_type.to_string(),
            resource_type: "context_item".to_string(),
            resource_id: None,
            details,
            ip_address: None,
            created_at,
            item_kind: None,
            // subject_item_id is set so `tamper_status` resolves the subject
            // deterministically (it falls back to `id` only when this is None).
            subject_item_id: Some(subject),
            subject_scope_kind: None,
            subject_scope_id: None,
            visible_by_scope: true,
        }
    }

    /// The signature the service expects for a row, computed exactly the way
    /// `tamper_status` recomputes it: `hmac_hex(key, "event|subject|rfc3339")`.
    fn expected_signature(key: &[u8], event_type: &str, subject: Uuid, created_at: DateTime<Utc>) -> String {
        hmac_hex(key, &format!("{}|{}|{}", event_type, subject, created_at.to_rfc3339()))
    }

    /// Golden HMAC-SHA256 vector pinned locally to the tamper-detection path's
    /// own hashing function, so a future digest/library change that altered the
    /// persisted-signature bytes fails here (next to the code that depends on
    /// it) and not only in the central `crypto_vectors` test.
    #[test]
    fn hmac_hex_matches_known_vector() {
        assert_eq!(
            hmac_hex(b"key", "The quick brown fox jumps over the lazy dog"),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8",
        );
    }

    #[test]
    fn tamper_status_valid_for_matching_signature() {
        let key = b"audit-hmac-key";
        let event = "context.item.exported";
        let subject = Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
        let created_at = fixed_created_at();
        let signature = expected_signature(key, event, subject, created_at);
        let row = audit_row(event, subject, created_at, json!({ "hmac_signature": signature }));
        assert!(matches!(tamper_status(&row, key), AuditTamperStatus::Valid));
    }

    #[test]
    fn tamper_status_invalid_for_tampered_signature() {
        let key = b"audit-hmac-key";
        let event = "context.item.exported";
        let subject = Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
        let created_at = fixed_created_at();
        let mut signature = expected_signature(key, event, subject, created_at);
        // Flip the first hex nibble so length is preserved but a byte differs.
        let first = if signature.starts_with('a') { 'b' } else { 'a' };
        signature.replace_range(0..1, &first.to_string());
        let row = audit_row(event, subject, created_at, json!({ "hmac_signature": signature }));
        assert!(matches!(tamper_status(&row, key), AuditTamperStatus::Invalid));
    }

    #[test]
    fn tamper_status_invalid_under_a_different_key() {
        let event = "context.item.exported";
        let subject = Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
        let created_at = fixed_created_at();
        let signature = expected_signature(b"signing-key", event, subject, created_at);
        let row = audit_row(event, subject, created_at, json!({ "hmac_signature": signature }));
        assert!(matches!(tamper_status(&row, b"verifying-key"), AuditTamperStatus::Invalid));
    }

    #[test]
    fn tamper_status_not_configured_without_signature() {
        let subject = Uuid::from_u128(1);
        let row = audit_row("context.item.exported", subject, fixed_created_at(), json!({}));
        assert!(matches!(tamper_status(&row, b"audit-hmac-key"), AuditTamperStatus::NotConfigured));
    }

    #[test]
    fn constant_time_eq_rejects_length_and_content_mismatch() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
