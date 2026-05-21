//! Resource profile endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/resource-profiles`      — list (includes system defaults)
//! - `POST   /api/v1/resource-profiles`      — create custom profile
//! - `GET    /api/v1/resource-profiles/{id}` — get
//! - `PATCH  /api/v1/resource-profiles/{id}` — update
//! - `DELETE /api/v1/resource-profiles/{id}` — delete

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::resource_profile::{
    CreateResourceProfileInput, ResourceProfileService, UpdateResourceProfileInput, resource_data_response,
    resource_delete_response,
};

/// Request body for creating a resource profile.
#[derive(Deserialize)]
pub struct CreateResourceProfileRequest {
    pub name: String,
    #[serde(default = "default_cpu")]
    pub cpu_millicores: i32,
    #[serde(default = "default_memory")]
    pub memory_mb: i32,
    #[serde(default = "default_storage")]
    pub storage_mb: i32,
    #[serde(default = "default_pids")]
    pub max_pids: i32,
}

fn default_cpu() -> i32 {
    1000
}
fn default_memory() -> i32 {
    512
}
fn default_storage() -> i32 {
    1024
}
fn default_pids() -> i32 {
    256
}

/// Request body for updating a resource profile.
#[derive(Deserialize)]
pub struct UpdateResourceProfileRequest {
    pub name: Option<String>,
    pub cpu_millicores: Option<i32>,
    pub memory_mb: Option<i32>,
    pub storage_mb: Option<i32>,
    pub max_pids: Option<i32>,
}

/// Build a ResourceProfileService from shared state.
fn make_service(state: &AppState) -> ResourceProfileService {
    ResourceProfileService::from_pool(state.pool.clone())
}

/// `GET /api/v1/resource-profiles` — list resource profiles.
async fn list_profiles(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let profiles = service.list(&auth.scope).await?;
    Ok(Json(resource_data_response(profiles)))
}

/// `POST /api/v1/resource-profiles` — create a custom profile.
async fn create_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateResourceProfileRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let profile = service
        .create(
            &auth.scope,
            CreateResourceProfileInput {
                name: req.name,
                cpu_millicores: req.cpu_millicores,
                memory_mb: req.memory_mb,
                storage_mb: req.storage_mb,
                max_pids: req.max_pids,
            },
        )
        .await?;
    Ok(Json(resource_data_response(profile)))
}

/// `GET /api/v1/resource-profiles/{id}` — get a profile.
async fn get_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let profile = service.get(&auth.scope, id).await?;
    Ok(Json(resource_data_response(profile)))
}

/// `PATCH /api/v1/resource-profiles/{id}` — update a profile.
async fn update_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateResourceProfileRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let profile = service
        .update(
            &auth.scope,
            id,
            UpdateResourceProfileInput {
                name: req.name,
                cpu_millicores: req.cpu_millicores,
                memory_mb: req.memory_mb,
                storage_mb: req.storage_mb,
                max_pids: req.max_pids,
            },
        )
        .await?;
    Ok(Json(resource_data_response(profile)))
}

/// `DELETE /api/v1/resource-profiles/{id}` — delete a profile.
async fn delete_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, id).await?;
    Ok(Json(resource_delete_response()))
}

/// Build resource profile routes sub-router.
pub fn resource_profile_routes() -> Router<AppState> {
    Router::new()
        .route("/resource-profiles", get(list_profiles).post(create_profile))
        .route("/resource-profiles/{id}", get(get_profile).patch(update_profile).delete(delete_profile))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_request_deserialization() {
        let req: CreateResourceProfileRequest = serde_json::from_str(r#"{"name": "large"}"#).unwrap();
        assert_eq!(req.name, "large");
        assert_eq!(req.cpu_millicores, 1000);
        assert_eq!(req.memory_mb, 512);
        assert_eq!(req.storage_mb, 1024);
        assert_eq!(req.max_pids, 256);
    }

    #[test]
    fn create_request_custom_values() {
        let req: CreateResourceProfileRequest = serde_json::from_str(
            r#"{"name": "xl", "cpu_millicores": 8000, "memory_mb": 16384, "storage_mb": 51200, "max_pids": 1024}"#,
        )
        .unwrap();
        assert_eq!(req.cpu_millicores, 8000);
        assert_eq!(req.memory_mb, 16384);
    }

    #[test]
    fn update_request_partial() {
        let req: UpdateResourceProfileRequest = serde_json::from_str(r#"{"memory_mb": 2048}"#).unwrap();
        assert!(req.name.is_none());
        assert_eq!(req.memory_mb, Some(2048));
        assert!(req.cpu_millicores.is_none());
    }
}
