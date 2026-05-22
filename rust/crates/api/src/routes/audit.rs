//! Audit log endpoints (nested under `/api/v1`).
//!
//! - `GET /api/v1/audit` — list audit log (query: action?, resource_type?, limit, offset)

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::audit::{AuditService, audit_data_response};

/// Query parameters for the audit log list endpoint.
#[derive(Deserialize)]
pub struct AuditListQuery {
    pub action: Option<String>,
    pub resource_type: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

/// Build an AuditService from shared state.
fn make_service(state: &AppState) -> AuditService {
    state.audit_service()
}

/// `GET /api/audit` — list audit log entries.
async fn list_audit(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<AuditListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let entries = service
        .list(&auth.scope, query.action.as_deref(), query.resource_type.as_deref(), query.limit, query.offset)
        .await?;
    Ok(Json(audit_data_response(entries)))
}

/// Build audit log routes sub-router.
pub fn audit_routes() -> Router<AppState> {
    Router::new().route("/audit", get(list_audit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_query_defaults() {
        let query: AuditListQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.limit, 20);
        assert_eq!(query.offset, 0);
        assert!(query.action.is_none());
        assert!(query.resource_type.is_none());
    }

    #[test]
    fn audit_query_with_filters() {
        let query: AuditListQuery =
            serde_json::from_str(r#"{"action": "create", "resource_type": "agent", "limit": 50, "offset": 10}"#)
                .unwrap();
        assert_eq!(query.action.as_deref(), Some("create"));
        assert_eq!(query.resource_type.as_deref(), Some("agent"));
        assert_eq!(query.limit, 50);
        assert_eq!(query.offset, 10);
    }

    #[test]
    fn audit_query_pagination() {
        let q1: AuditListQuery = serde_json::from_str(r#"{"limit": 10, "offset": 0}"#).unwrap();
        let q2: AuditListQuery = serde_json::from_str(r#"{"limit": 10, "offset": 10}"#).unwrap();
        assert_eq!(q1.limit, q2.limit);
        assert_ne!(q1.offset, q2.offset);
    }
}
