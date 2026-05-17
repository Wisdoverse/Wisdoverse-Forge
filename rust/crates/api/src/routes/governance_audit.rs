//! Governed context audit projection endpoints.
//!
//! - `GET /api/v1/governance/audit` — list scope-aware governance audit rows.
//! - `POST /api/v1/governance/audit/export` — return a redacted export bundle
//!   and record that export in the audit trail.

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, ErrorKind};

use crate::domain::context_governance::{ContextGovernancePolicy, GovernanceAuditQueryPolicy};
use crate::health::{AppState, ContextFeature, ensure_context_feature_enabled};
use crate::repositories::audit::AuditRepository;
use crate::repositories::governance_audit::{
    GOVERNANCE_CONTEXT_AUDIT_PREFIX, GovernanceAuditFilter, GovernanceAuditRepository, GovernanceAuditRow,
    normalize_limit,
};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceAuditQueryParams {
    pub event_type: Option<String>,
    pub event_prefix: Option<String>,
    pub item_kind: Option<String>,
    pub scope_kind: Option<String>,
    pub scope_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub redact_secrets: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceAuditResponse {
    pub entries: Vec<GovernanceAuditEntry>,
    pub query: GovernanceAuditQuery,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceAuditQuery {
    pub event_prefix: String,
    pub limit: i64,
    pub offset: i64,
    pub redacted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceAuditEntry {
    pub id: Uuid,
    pub event_type: String,
    pub actor_user_id: Option<Uuid>,
    pub item_kind: Option<String>,
    pub scope_kind: Option<String>,
    pub scope_id: Option<Uuid>,
    pub raw_item_id: Option<Uuid>,
    pub audit_subject_hash: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub details: Value,
    pub details_redacted: bool,
    pub tamper_status: AuditTamperStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditTamperStatus {
    NotConfigured,
    Valid,
    Invalid,
}

pub fn governance_audit_routes() -> Router<AppState> {
    Router::new()
        .route("/governance/audit", get(list_governance_audit))
        .route("/governance/audit/export", post(export_governance_audit))
}

async fn list_governance_audit(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<GovernanceAuditQueryParams>,
) -> AppResult<Json<Value>> {
    ensure_context_feature_enabled(&state, &auth.scope, ContextFeature::Governance).await?;
    let data = load_projection(&state, &auth, query).await?;
    Ok(Json(json!({ "ok": true, "data": data })))
}

async fn export_governance_audit(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(mut query): Json<GovernanceAuditQueryParams>,
) -> AppResult<Json<Value>> {
    ensure_context_feature_enabled(&state, &auth.scope, ContextFeature::Governance).await?;
    query.redact_secrets = Some(query.redact_secrets.unwrap_or(true));
    query.limit = Some(query.limit.unwrap_or(500).clamp(1, 500));

    let data = load_projection(&state, &auth, query.clone()).await?;
    let audit = AuditRepository::new(state.pool.clone());
    audit
        .create(
            auth.scope.org_id(),
            Some(auth.scope.user_id()),
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

    Ok(Json(json!({ "ok": true, "data": data })))
}

async fn load_projection(
    state: &AppState,
    auth: &AuthUser,
    query: GovernanceAuditQueryParams,
) -> AppResult<GovernanceAuditResponse> {
    validate_query(&query)?;
    let event_prefix = query.event_prefix.as_deref().unwrap_or(GOVERNANCE_CONTEXT_AUDIT_PREFIX);
    let redact = query.redact_secrets.unwrap_or(true);
    let key = audit_hmac_key(state)?;
    let include_org_wide = is_admin_role(&auth.role);
    let repo = GovernanceAuditRepository::new(state.pool.clone());
    let rows = repo
        .list(
            &auth.scope,
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

    let entries = rows.into_iter().map(|row| project_row(row, &key, redact)).collect();
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

fn validate_query(query: &GovernanceAuditQueryParams) -> AppResult<()> {
    ContextGovernancePolicy::validate_audit_query(GovernanceAuditQueryPolicy {
        event_prefix: query.event_prefix.as_deref(),
        event_type: query.event_type.as_deref(),
        item_kind: query.item_kind.as_deref(),
        scope_kind: query.scope_kind.as_deref(),
        from: query.from,
        to: query.to,
    })
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

fn audit_hmac_key(state: &AppState) -> AppResult<Vec<u8>> {
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

    if let Some(key) = state.encryption_key {
        return Ok(key.to_vec());
    }

    if state.config.is_production() {
        return Err(ErrorKind::Validation("CONTEXT_AUDIT_HMAC_KEY or LLM_ENCRYPTION_KEY is required".into()).into());
    }

    Ok(b"agentforge-dev-governance-audit-key".to_vec())
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
    use super::*;

    #[test]
    fn rejects_non_governance_prefix() {
        let query = GovernanceAuditQueryParams {
            event_prefix: Some("system.".to_string()),
            event_type: None,
            item_kind: None,
            scope_kind: None,
            scope_id: None,
            user_id: None,
            from: None,
            to: None,
            redact_secrets: None,
            limit: None,
            offset: None,
        };
        assert!(validate_query(&query).is_err());
    }

    #[test]
    fn redacts_secret_bearing_details() {
        let (value, redacted) = ContextGovernancePolicy::redact_audit_details(json!({
            "classification": {
                "token": "github-token-placeholder"
            },
            "safe": "internal"
        }));
        assert!(redacted);
        assert_eq!(value["classification"]["token"], "[REDACTED]");
        assert_eq!(value["safe"], "internal");
    }
}
