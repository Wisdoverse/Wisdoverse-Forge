//! Governed context audit projection endpoints.
//!
//! - `GET /api/v1/governance/audit` — list scope-aware governance audit rows.
//! - `POST /api/v1/governance/audit/export` — return a redacted export bundle
//!   and record that export in the audit trail.

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, ErrorKind};

use crate::domain::context_governance::{ContextGovernancePolicy, GovernanceAuditQueryPolicy};
use crate::domain::governance_audit::{GovernanceAuditProjection, GovernanceAuditQuery, GovernanceAuditResponse};
use crate::health::{AppState, ContextFeature, ensure_context_feature_enabled};
use crate::repositories::audit::AuditRepository;
use crate::repositories::governance_audit::{
    GOVERNANCE_CONTEXT_AUDIT_PREFIX, GovernanceAuditFilter, GovernanceAuditRepository, normalize_limit,
};

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

    let entries = rows.into_iter().map(|row| GovernanceAuditProjection::project_row(row, &key, redact)).collect();
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
