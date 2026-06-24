//! Group CRUD endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/groups`                   — list groups (paginated)
//! - `POST   /api/v1/groups`                   — create group
//! - `GET    /api/v1/groups/{id}`              — get group by ID
//! - `PATCH  /api/v1/groups/{id}`              — update group
//! - `DELETE /api/v1/groups/{id}`              — soft delete group
//! - `GET    /api/v1/groups/{id}/members`      — list members
//! - `POST   /api/v1/groups/{id}/members`      — add member
//! - `DELETE /api/v1/groups/{id}/members/{user_id}` — remove member

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, GroupId, ProjectId};

use crate::health::AppState;
use crate::services::group::{
    GroupService, resource_data_response, resource_delete_response, resource_group_created_response,
    resource_project_groups_response,
};

/// Query parameters for the list endpoint.
#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default, alias = "projectId")]
    pub project_id: Option<Uuid>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

/// Request body for creating a group.
#[derive(Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default, alias = "projectId")]
    pub project_id: Option<Uuid>,
}

/// Request body for updating a group.
#[derive(Deserialize)]
pub struct UpdateGroupRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Request body for adding a member.
#[derive(Deserialize)]
pub struct AddMemberRequest {
    pub user_id: Uuid,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "member".to_string()
}

/// Build a service instance from shared state.
fn make_service(state: &AppState) -> GroupService {
    state.group_service()
}

/// `GET /api/v1/groups` — list groups for the authenticated tenant.
///
/// When `project_id` is provided, scopes to that project via `public.groups`.
/// Post-P4 there is no legacy fallback; migration 027 dropped `legacy.*`.
async fn list_groups(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    if let Some(project_id) = query.project_id {
        let groups = service.list_project_group_summaries(&auth.scope, ProjectId::from(project_id)).await?;
        return Ok(Json(resource_project_groups_response(groups)));
    }

    let groups = service.list(&auth.scope, query.limit, query.offset).await?;
    Ok(Json(resource_data_response(groups)))
}

/// `GET /api/v1/groups/{id}` — get a single group.
async fn get_group(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let group = service.get(&auth.scope, GroupId::from(id)).await?;
    Ok(Json(resource_data_response(group)))
}

/// `POST /api/v1/groups` — create a new group.
async fn create_group(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateGroupRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let project_id = req.project_id.map(ProjectId::from);
    let group = service.create(&auth.scope, &req.name, req.description.as_deref(), project_id).await?;
    let summary = service.project_group_summary(&group, req.project_id);
    Ok(Json(resource_group_created_response(group, summary)))
}

/// `PATCH /api/v1/groups/{id}` — update a group.
async fn update_group(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateGroupRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let group = service.update(&auth.scope, GroupId::from(id), req.name.as_deref(), req.description.as_deref()).await?;
    Ok(Json(resource_data_response(group)))
}

/// `DELETE /api/v1/groups/{id}` — soft delete a group.
async fn delete_group(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, GroupId::from(id)).await?;
    Ok(Json(resource_delete_response()))
}

/// `GET /api/v1/groups/{id}/members` — list members of a group.
async fn list_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let members = service.list_members(&auth.scope, GroupId::from(id)).await?;
    Ok(Json(resource_data_response(members)))
}

/// `POST /api/v1/groups/{id}/members` — add a member to a group.
async fn add_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let member = service.add_member(&auth.scope, GroupId::from(id), req.user_id, &req.role).await?;
    Ok(Json(resource_data_response(member)))
}

/// `DELETE /api/v1/groups/{id}/members/{user_id}` — remove a member from a group.
async fn remove_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.remove_member(&auth.scope, GroupId::from(id), user_id).await?;
    Ok(Json(resource_delete_response()))
}

/// Build the group routes sub-router.
///
/// P4 collapsed the former compat-split. `GET /groups` (with optional
/// `projectId` query) reads canonical `public.groups`; there is no separate
/// `group_compat_routes` any more.
pub fn group_routes() -> Router<AppState> {
    Router::new()
        .route("/groups", get(list_groups).post(create_group))
        .route("/groups/{id}", get(get_group).patch(update_group).delete(delete_group))
        .route("/groups/{id}/members", get(list_members).post(add_member))
        .route("/groups/{id}/members/{user_id}", axum::routing::delete(remove_member))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_query_defaults() {
        let query: ListQuery = serde_json::from_str("{}").unwrap();
        assert!(query.project_id.is_none());
        assert_eq!(query.limit, 20);
        assert_eq!(query.offset, 0);
    }

    #[test]
    fn list_query_custom_values() {
        let query: ListQuery =
            serde_json::from_str(r#"{"projectId":"550e8400-e29b-41d4-a716-446655440000","limit": 50, "offset": 10}"#)
                .unwrap();
        assert!(query.project_id.is_some());
        assert_eq!(query.limit, 50);
        assert_eq!(query.offset, 10);
    }

    #[test]
    fn create_request_deserialization() {
        let req: CreateGroupRequest =
            serde_json::from_str(
                r#"{"name": "Backend Team", "description": "Backend devs", "projectId": "550e8400-e29b-41d4-a716-446655440000"}"#,
            )
            .unwrap();
        assert_eq!(req.name, "Backend Team");
        assert_eq!(req.description.as_deref(), Some("Backend devs"));
        assert_eq!(req.project_id, Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()));
    }

    #[test]
    fn create_request_name_only() {
        let req: CreateGroupRequest = serde_json::from_str(r#"{"name": "Frontend"}"#).unwrap();
        assert_eq!(req.name, "Frontend");
        assert!(req.description.is_none());
        assert!(req.project_id.is_none());
    }

    #[test]
    fn create_request_missing_name_fails() {
        let result = serde_json::from_str::<CreateGroupRequest>(r#"{}"#);
        assert!(result.is_err());
    }

    #[test]
    fn update_request_all_fields() {
        let req: UpdateGroupRequest =
            serde_json::from_str(r#"{"name": "New Name", "description": "New desc"}"#).unwrap();
        assert_eq!(req.name.as_deref(), Some("New Name"));
        assert_eq!(req.description.as_deref(), Some("New desc"));
    }

    #[test]
    fn update_request_empty() {
        let req: UpdateGroupRequest = serde_json::from_str(r#"{}"#).unwrap();
        assert!(req.name.is_none());
        assert!(req.description.is_none());
    }

    #[test]
    fn add_member_request_defaults() {
        let req: AddMemberRequest =
            serde_json::from_str(r#"{"user_id": "00000000-0000-0000-0000-000000000001"}"#).unwrap();
        assert_eq!(req.role, "member");
    }

    #[test]
    fn add_member_request_with_role() {
        let req: AddMemberRequest =
            serde_json::from_str(r#"{"user_id": "00000000-0000-0000-0000-000000000001", "role": "admin"}"#).unwrap();
        assert_eq!(req.role, "admin");
    }

    #[test]
    fn add_member_missing_user_id_fails() {
        let result = serde_json::from_str::<AddMemberRequest>(r#"{"role": "member"}"#);
        assert!(result.is_err());
    }
}

/// Test-only entry point for integration tests in
/// `tests/nav_regression_e2e_test.rs`. Delegates to
/// `GroupRepository::list_canonical_for_project_for_test` so the contract test
/// exercises the same SQL the route serves.
///
/// Marked `#[doc(hidden)]` to keep it out of rustdoc and signal "not for
/// downstream use."
#[doc(hidden)]
#[cfg(any(test, feature = "test-support"))]
pub mod test_only {
    use agentforge_core::AppResult;
    use sqlx::PgPool;
    use uuid::Uuid;

    use crate::domain::resource::ProjectGroupSummary;
    use crate::repositories::identity::group::GroupRepository;

    /// Exposes the canonical project-group SQL to integration tests without
    /// needing a live Axum stack, while keeping the SQL centralized in the
    /// repository test-support path.
    pub async fn list_groups_canonical_for_test(
        pool: &PgPool,
        user_id: Uuid,
        project_id: Uuid,
    ) -> AppResult<Vec<serde_json::Value>> {
        let rows = GroupRepository::new(pool.clone()).list_canonical_for_project_for_test(user_id, project_id).await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                serde_json::to_value(ProjectGroupSummary::new(row.id, row.name, row.project_id))
                    .expect("ProjectGroupSummary serializes")
            })
            .collect())
    }
}
