//! Project CRUD endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/projects`            — list projects (paginated, optional workspace filter)
//! - `POST   /api/v1/projects`            — create project (optional `repository_url` to clone)
//! - `GET    /api/v1/projects/{id}`       — get project by ID (with clone summary)
//! - `PATCH  /api/v1/projects/{id}`       — update project (repo URL immutable once cloned)
//! - `DELETE /api/v1/projects/{id}`       — soft delete project
//! - `POST   /api/v1/projects/{id}/clone/retry` — retry a FAILED clone
//!
//! Every handler authenticates via the `AuthUser` extractor (JWT) and operates
//! through the tenant `scope` it derives, so a foreign-org / unauthenticated
//! caller is rejected before any service call.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, ProjectId, TeamId, WorkspaceId};

use crate::health::AppState;
use crate::services::project::{
    CreateProjectInput, ProjectService, UpdateProjectInput, resource_data_response, resource_delete_response,
};

/// Query parameters for the list endpoint.
#[derive(Deserialize)]
pub struct ListQuery {
    pub workspace_id: Option<Uuid>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

/// Request body for creating a project.
///
/// `team_id` is optional because legacy callers predating P3 MR-B never sent
/// it; when absent the repository defaults to the org's oldest surviving
/// team, matching migration 026's backfill rule. New callers should send
/// `team_id` explicitly — leaving it off an empty org will 400.
#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub workspace_id: Uuid,
    pub team_id: Option<Uuid>,
    pub name: String,
    pub repository_url: Option<String>,
}

/// Deserialize a field that distinguishes between absent, null, and present.
/// - Absent -> `None` (field not in JSON)
/// - `null` -> `Some(None)` (explicitly set to null)
/// - `"value"` -> `Some(Some("value"))` (explicitly set to a value)
fn deserialize_optional_field<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

/// Request body for updating a project.
///
/// `repository_url` is still accepted on the wire so a client that sends one gets
/// a clear, actionable `400` ("the repository URL is set when the project is
/// created and cannot be changed afterward") from the service rather than a
/// confusing deserialize error or a silent drop. The service REJECTS any present
/// value (§9 one-shot bind); an update never writes the column.
#[derive(Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_field")]
    pub repository_url: Option<Option<String>>,
}

/// Build a service instance from shared state.
fn make_service(state: &AppState) -> ProjectService {
    state.project_service()
}

/// `GET /api/projects` — list projects for the authenticated tenant.
async fn list_projects(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let workspace_id = query.workspace_id.map(WorkspaceId::from);
    let projects = service.list(&auth.scope, workspace_id, query.limit, query.offset).await?;
    Ok(Json(resource_data_response(projects)))
}

/// `GET /api/projects/{id}` — get a single project.
async fn get_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let project = service.get(&auth.scope, ProjectId::from(id)).await?;
    Ok(Json(resource_data_response(project)))
}

/// `POST /api/projects` — create a new project.
async fn create_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateProjectRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let project = service
        .create(
            &auth.scope,
            CreateProjectInput {
                workspace_id: WorkspaceId::from(req.workspace_id),
                team_id: req.team_id.map(TeamId::from),
                name: req.name,
                repository_url: req.repository_url,
            },
        )
        .await?;
    Ok(Json(resource_data_response(project)))
}

/// `PATCH /api/projects/{id}` — update a project.
async fn update_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProjectRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let project_id = ProjectId::from(id);
    let service = make_service(&state);
    let project = service
        .update(&auth.scope, project_id, UpdateProjectInput { name: req.name, repository_url: req.repository_url })
        .await?;
    Ok(Json(resource_data_response(project)))
}

/// `DELETE /api/projects/{id}` — soft delete a project.
async fn delete_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let project_id = ProjectId::from(id);
    let service = make_service(&state);
    service.delete(&auth.scope, project_id).await?;
    Ok(Json(resource_delete_response()))
}

/// `POST /api/projects/{id}/clone/retry` — retry a failed clone.
///
/// Owner/manager only (enforced in the service via `require_project_manager`).
/// Allowed ONLY when the latest attempt is `failed`; otherwise the service
/// returns a `409 Conflict` (or `400` when the project has no repository URL).
/// On success a new `queued` attempt is created and its summary is returned.
async fn retry_clone(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let project_id = ProjectId::from(id);
    let service = make_service(&state);
    let summary = service.retry_clone(&auth.scope, project_id).await?;
    Ok(Json(resource_data_response(summary)))
}

/// Build project routes sub-router.
pub fn project_routes() -> Router<AppState> {
    Router::new()
        .route("/projects", get(list_projects).post(create_project))
        .route("/projects/{id}", get(get_project).patch(update_project).delete(delete_project))
        .route("/projects/{id}/clone/retry", post(retry_clone))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_query_defaults() {
        let query: ListQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.limit, 20);
        assert_eq!(query.offset, 0);
        assert!(query.workspace_id.is_none());
    }

    #[test]
    fn list_query_with_workspace_filter() {
        let query: ListQuery =
            serde_json::from_str(r#"{"workspace_id": "550e8400-e29b-41d4-a716-446655440000", "limit": 50}"#).unwrap();
        assert!(query.workspace_id.is_some());
        assert_eq!(query.limit, 50);
    }

    #[test]
    fn create_request_full() {
        let req: CreateProjectRequest = serde_json::from_str(
            r#"{
                "workspace_id": "550e8400-e29b-41d4-a716-446655440000",
                "name": "Wisdoverse Forge",
                "repository_url": "https://github.com/Wisdoverse/Wisdoverse-Forge"
            }"#,
        )
        .unwrap();
        assert_eq!(req.name, "Wisdoverse Forge");
        assert_eq!(req.repository_url.as_deref(), Some("https://github.com/Wisdoverse/Wisdoverse-Forge"));
    }

    #[test]
    fn create_request_minimal() {
        let req: CreateProjectRequest =
            serde_json::from_str(r#"{"workspace_id": "550e8400-e29b-41d4-a716-446655440000", "name": "MyProject"}"#)
                .unwrap();
        assert_eq!(req.name, "MyProject");
        assert!(req.repository_url.is_none());
    }

    #[test]
    fn create_request_missing_name_fails() {
        let result =
            serde_json::from_str::<CreateProjectRequest>(r#"{"workspace_id": "550e8400-e29b-41d4-a716-446655440000"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn update_request_partial() {
        let req: UpdateProjectRequest = serde_json::from_str(r#"{"name": "New Name"}"#).unwrap();
        assert_eq!(req.name.as_deref(), Some("New Name"));
        assert!(req.repository_url.is_none());
    }

    #[test]
    fn update_request_empty() {
        let req: UpdateProjectRequest = serde_json::from_str(r#"{}"#).unwrap();
        assert!(req.name.is_none());
        assert!(req.repository_url.is_none());
    }

    #[test]
    fn update_request_clear_url() {
        let req: UpdateProjectRequest = serde_json::from_str(r#"{"repository_url": null}"#).unwrap();
        assert_eq!(req.repository_url, Some(None));
    }
}
