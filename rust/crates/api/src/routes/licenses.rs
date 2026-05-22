//! License endpoints (nested under `/api/v1`).
//!
//! - `GET  /licenses`          — list org licenses
//! - `POST /licenses/validate` — validate a license key
//! - `POST /licenses/activate` — activate license
//! - `GET  /licenses/{id}`     — get license details

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::license::{LicenseService, license_data_response};

/// Request body for validating/activating a license.
#[derive(Deserialize)]
pub struct LicenseKeyRequest {
    pub license_key: String,
}

/// Build a LicenseService from shared state.
fn make_service(state: &AppState) -> LicenseService {
    state.license_service()
}

/// `GET /licenses` — list licenses.
async fn list_licenses(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let licenses = service.list(&auth.scope).await?;
    Ok(Json(license_data_response(licenses)))
}

/// `POST /licenses/validate` — validate a license key.
async fn validate_license(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(req): Json<LicenseKeyRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let result = service.validate(&req.license_key).await?;
    Ok(Json(license_data_response(result)))
}

/// `POST /licenses/activate` — activate a license.
async fn activate_license(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<LicenseKeyRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let license = service.activate(&auth.scope, &req.license_key).await?;
    tracing::info!(org_id = %auth.scope.org_id(), license = %license.license_key, "License activated");
    Ok(Json(license_data_response(license)))
}

/// `GET /licenses/{id}` — get license details.
async fn get_license(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let license = service.get(&auth.scope, id).await?;
    Ok(Json(license_data_response(license)))
}

/// Build license routes sub-router.
pub fn license_routes() -> Router<AppState> {
    Router::new()
        .route("/licenses", get(list_licenses))
        .route("/licenses/validate", post(validate_license))
        .route("/licenses/activate", post(activate_license))
        .route("/licenses/{id}", get(get_license))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn license_key_request_deserialization() {
        let req: LicenseKeyRequest = serde_json::from_str(r#"{"license_key": "LIC-ABC-123"}"#).unwrap();
        assert_eq!(req.license_key, "LIC-ABC-123");
    }

    #[test]
    fn license_key_request_empty_key() {
        let req: LicenseKeyRequest = serde_json::from_str(r#"{"license_key": ""}"#).unwrap();
        assert!(req.license_key.is_empty());
    }
}
