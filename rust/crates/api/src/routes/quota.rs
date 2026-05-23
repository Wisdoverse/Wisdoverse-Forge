//! Quota endpoints (nested under `/api/v1`).
//!
//! - `GET /api/v1/quota`                 — get org quota usage summary
//! - `GET /api/v1/quota/{resource_type}` — get specific resource usage

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::quota::{QuotaService, configuration_data_response};

/// Build a QuotaService from shared state.
fn make_service(state: &AppState) -> QuotaService {
    state.quota_service()
}

/// `GET /api/v1/quota` — get org quota usage summary.
async fn list_quota(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let quotas = service.list(&auth.scope).await?;
    Ok(Json(configuration_data_response(quotas)))
}

/// `GET /api/v1/quota/{resource_type}` — get specific resource usage.
async fn get_quota_by_type(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(resource_type): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let quota = service.get_by_type(&auth.scope, &resource_type).await?;
    Ok(Json(configuration_data_response(quota)))
}

/// Build quota routes sub-router.
pub fn quota_routes() -> Router<AppState> {
    Router::new().route("/quota", get(list_quota)).route("/quota/{resource_type}", get(get_quota_by_type))
}

#[cfg(test)]
mod tests {
    #[test]
    fn quota_response_format() {
        let response = serde_json::json!({
            "ok": true,
            "data": {
                "resource_type": "agents",
                "current_usage": 5,
                "max_allowed": 10
            }
        });
        assert_eq!(response["ok"], true);
        assert_eq!(response["data"]["resource_type"], "agents");
    }
}
