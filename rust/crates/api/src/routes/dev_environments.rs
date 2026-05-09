//! Dev environment endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/devenv`            — list environments
//! - `POST   /api/v1/devenv`            — create environment
//! - `GET    /api/v1/devenv/{id}`       — get environment
//! - `POST   /api/v1/devenv/{id}/start` — create and start the environment container
//! - `POST   /api/v1/devenv/{id}/stop`  — stop and remove the environment container
//! - `DELETE /api/v1/devenv/{id}`       — delete environment

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::repositories::dev_environment::DevEnvironmentRepository;
use crate::services::dev_environment::{DevEnvironmentRuntime, DevEnvironmentService, DockerDevEnvironmentRuntime};

/// Request body for creating a dev environment.
#[derive(Deserialize)]
pub struct CreateDevEnvRequest {
    pub name: String,
    pub project_id: Option<Uuid>,
    #[serde(default = "default_config")]
    pub config: serde_json::Value,
}

fn default_config() -> serde_json::Value {
    serde_json::json!({})
}

/// Build a DevEnvironmentService from shared state.
fn make_service(state: &AppState) -> DevEnvironmentService {
    let runtime = state
        .docker
        .as_ref()
        .map(|docker| Arc::new(DockerDevEnvironmentRuntime::new(docker.clone())) as Arc<dyn DevEnvironmentRuntime>);
    DevEnvironmentService::with_runtime(DevEnvironmentRepository::new(state.pool.clone()), runtime)
}

/// `GET /api/v1/devenv` — list dev environments.
async fn list_devenvs(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let envs = service.list(&auth.scope).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": envs })))
}

/// `POST /api/v1/devenv` — create a dev environment.
async fn create_devenv(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateDevEnvRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let env = service.create(&auth.scope, &req.name, req.project_id, &req.config).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": env })))
}

/// `GET /api/v1/devenv/{id}` — get a dev environment.
async fn get_devenv(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let env = service.get(&auth.scope, id).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": env })))
}

/// `POST /api/v1/devenv/{id}/start` — create and start the dev environment container.
async fn start_devenv(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let env = service.start(&auth.scope, id).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": env, "message": "environment started" })))
}

/// `POST /api/v1/devenv/{id}/stop` — stop and remove the dev environment container.
async fn stop_devenv(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let env = service.stop(&auth.scope, id).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": env, "message": "environment stopped" })))
}

/// `DELETE /api/v1/devenv/{id}` — delete a dev environment.
async fn delete_devenv(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Build dev environment routes sub-router.
pub fn dev_environment_routes() -> Router<AppState> {
    Router::new()
        .route("/devenv", get(list_devenvs).post(create_devenv))
        // Static routes BEFORE parameterized (per CLAUDE.md)
        .route("/devenv/{id}/start", post(start_devenv))
        .route("/devenv/{id}/stop", post(stop_devenv))
        .route("/devenv/{id}", get(get_devenv).delete(delete_devenv))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_request_deserialization() {
        let req: CreateDevEnvRequest = serde_json::from_str(r#"{"name": "dev-env-1"}"#).unwrap();
        assert_eq!(req.name, "dev-env-1");
        assert!(req.project_id.is_none());
        assert!(req.config.is_object());
    }

    #[test]
    fn create_request_with_project() {
        let req: CreateDevEnvRequest = serde_json::from_str(
            r#"{
                "name": "my-env",
                "project_id": "550e8400-e29b-41d4-a716-446655440000",
                "config": {"image": "ubuntu:22.04", "ports": [8080]}
            }"#,
        )
        .unwrap();
        assert!(req.project_id.is_some());
        assert_eq!(req.config["image"], "ubuntu:22.04");
    }
}
