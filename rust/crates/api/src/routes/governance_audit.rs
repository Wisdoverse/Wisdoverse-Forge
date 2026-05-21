//! Governed context audit projection endpoints.
//!
//! - `GET /api/v1/governance/audit` — list scope-aware governance audit rows.
//! - `POST /api/v1/governance/audit/export` — return a redacted export bundle
//!   and record that export in the audit trail.

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::context_feature::ContextFeatureService;
use crate::services::governance_audit::{
    GovernanceAuditService, QueryParams as GovernanceAuditQueryParams, governance_audit_response,
};

pub fn governance_audit_routes() -> Router<AppState> {
    Router::new()
        .route("/governance/audit", get(list_governance_audit))
        .route("/governance/audit/export", post(export_governance_audit))
}

fn make_service(state: &AppState) -> AppResult<GovernanceAuditService> {
    GovernanceAuditService::from_pool_and_app_config(state.pool.clone(), &state.config, state.encryption_key)
}

fn make_feature_service(state: &AppState) -> ContextFeatureService {
    ContextFeatureService::new(state.pool.clone(), state.context_features)
}

async fn list_governance_audit(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<GovernanceAuditQueryParams>,
) -> AppResult<Json<serde_json::Value>> {
    make_feature_service(&state).ensure_governance_enabled(&auth.scope).await?;
    let data = make_service(&state)?.list(&auth.scope, &auth.role, query).await?;
    Ok(Json(governance_audit_response(data)))
}

async fn export_governance_audit(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(query): Json<GovernanceAuditQueryParams>,
) -> AppResult<Json<serde_json::Value>> {
    make_feature_service(&state).ensure_governance_enabled(&auth.scope).await?;
    let data = make_service(&state)?.export(&auth.scope, &auth.role, query).await?;
    Ok(Json(governance_audit_response(data)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::context_governance::ContextGovernancePolicy;

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
        assert!(ContextGovernancePolicy::validate_governance_audit_query_params(&query).is_err());
    }

    #[test]
    fn redacts_secret_bearing_details() {
        let (value, redacted) = ContextGovernancePolicy::redact_audit_details(serde_json::json!({
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
