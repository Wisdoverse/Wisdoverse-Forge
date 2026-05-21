//! Workspace CRUD endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/workspaces`      — list workspaces (paginated)
//! - `POST   /api/v1/workspaces`      — create workspace
//! - `GET    /api/v1/workspaces/{id}` — get workspace by ID
//! - `PATCH  /api/v1/workspaces/{id}` — update workspace
//! - `DELETE /api/v1/workspaces/{id}` — soft delete workspace

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, WorkspaceId};

use crate::health::AppState;
use crate::services::workspace::{
    CreateWorkspaceInput, UpdateWorkspaceInput, WorkspaceService, resource_data_response, resource_delete_response,
};

/// Query parameters for the list endpoint.
#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

/// Request body for creating a workspace.
#[derive(Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
}

/// Request body for updating a workspace.
#[derive(Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: String,
}

/// Build a service instance from shared state.
fn make_service(state: &AppState) -> WorkspaceService {
    WorkspaceService::from_pool(state.pool.clone())
}

/// `GET /api/workspaces` — list workspaces for the authenticated tenant.
async fn list_workspaces(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let workspaces = service.list(&auth.scope, query.limit, query.offset).await?;
    Ok(Json(resource_data_response(workspaces)))
}

/// `GET /api/workspaces/{id}` — get a single workspace.
async fn get_workspace(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let workspace = service.get(&auth.scope, WorkspaceId::from(id)).await?;
    Ok(Json(resource_data_response(workspace)))
}

/// `POST /api/workspaces` — create a new workspace.
async fn create_workspace(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateWorkspaceRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let workspace = service.create(&auth.scope, CreateWorkspaceInput { name: req.name }).await?;
    Ok(Json(resource_data_response(workspace)))
}

/// `PATCH /api/workspaces/{id}` — update a workspace.
async fn update_workspace(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let workspace = service.update(&auth.scope, WorkspaceId::from(id), UpdateWorkspaceInput { name: req.name }).await?;
    Ok(Json(resource_data_response(workspace)))
}

/// `DELETE /api/workspaces/{id}` — soft delete a workspace.
async fn delete_workspace(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, WorkspaceId::from(id)).await?;
    Ok(Json(resource_delete_response()))
}

/// Build workspace routes sub-router.
pub fn workspace_routes() -> Router<AppState> {
    Router::new()
        .route("/workspaces", get(list_workspaces).post(create_workspace))
        .route("/workspaces/{id}", get(get_workspace).patch(update_workspace).delete(delete_workspace))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_query_defaults() {
        let query: ListQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.limit, 20);
        assert_eq!(query.offset, 0);
    }

    #[test]
    fn list_query_custom_values() {
        let query: ListQuery = serde_json::from_str(r#"{"limit": 50, "offset": 10}"#).unwrap();
        assert_eq!(query.limit, 50);
        assert_eq!(query.offset, 10);
    }

    #[test]
    fn create_request_deserialization() {
        let req: CreateWorkspaceRequest = serde_json::from_str(r#"{"name": "Dev Workspace"}"#).unwrap();
        assert_eq!(req.name, "Dev Workspace");
    }

    #[test]
    fn create_request_missing_name_fails() {
        let result = serde_json::from_str::<CreateWorkspaceRequest>(r#"{}"#);
        assert!(result.is_err());
    }

    #[test]
    fn update_request_deserialization() {
        let req: UpdateWorkspaceRequest = serde_json::from_str(r#"{"name": "Updated Workspace"}"#).unwrap();
        assert_eq!(req.name, "Updated Workspace");
    }
}
