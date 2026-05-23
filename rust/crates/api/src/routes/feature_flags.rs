//! Feature flag endpoints (nested under `/api/v1`).
//!
//! - `GET /api/v1/feature-flags`        — list flags for org
//! - `GET /api/v1/feature-flags/{name}` — get specific flag
//! - `PUT /api/v1/feature-flags/{name}` — upsert flag

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::feature_flag::{FeatureFlagService, UpsertFeatureFlagInput, configuration_data_response};

/// Request body for upserting a feature flag.
#[derive(Deserialize)]
pub struct UpsertFeatureFlagRequest {
    pub enabled: bool,
    pub metadata: Option<serde_json::Value>,
}

/// Build a FeatureFlagService from shared state.
fn make_service(state: &AppState) -> FeatureFlagService {
    state.feature_flag_service()
}

/// `GET /api/feature-flags` — list flags.
async fn list_flags(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let flags = service.list(&auth.scope).await?;
    Ok(Json(configuration_data_response(flags)))
}

/// `GET /api/feature-flags/{name}` — get a specific flag.
async fn get_flag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(name): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let flag = service.get_by_name(&auth.scope, &name).await?;
    Ok(Json(configuration_data_response(flag)))
}

/// `PUT /api/feature-flags/{name}` — upsert a flag.
async fn upsert_flag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(name): Path<String>,
    Json(req): Json<UpsertFeatureFlagRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let flag = service
        .upsert(&auth.scope, &name, UpsertFeatureFlagInput { enabled: req.enabled, metadata: req.metadata })
        .await?;
    tracing::info!(org_id = %auth.scope.org_id(), user_id = %auth.scope.user_id(), flag = %name, enabled = req.enabled, "Feature flag updated");
    Ok(Json(configuration_data_response(flag)))
}

/// Build feature flag routes sub-router.
pub fn feature_flag_routes() -> Router<AppState> {
    Router::new()
        .route("/feature-flags", get(list_flags))
        .route("/feature-flags/{name}", get(get_flag).put(upsert_flag))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_request_deserialization() {
        let req: UpsertFeatureFlagRequest =
            serde_json::from_str(r#"{"enabled": true, "metadata": {"rollout": 0.5}}"#).unwrap();
        assert!(req.enabled);
        assert_eq!(req.metadata.unwrap()["rollout"], 0.5);
    }

    #[test]
    fn upsert_request_without_metadata() {
        let req: UpsertFeatureFlagRequest = serde_json::from_str(r#"{"enabled": false}"#).unwrap();
        assert!(!req.enabled);
        assert!(req.metadata.is_none());
    }

    #[test]
    fn feature_flag_toggle() {
        let enable: UpsertFeatureFlagRequest = serde_json::from_str(r#"{"enabled": true}"#).unwrap();
        let disable: UpsertFeatureFlagRequest = serde_json::from_str(r#"{"enabled": false}"#).unwrap();
        assert!(enable.enabled);
        assert!(!disable.enabled);
    }
}
