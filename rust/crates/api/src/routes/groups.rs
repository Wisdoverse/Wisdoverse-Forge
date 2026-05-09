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
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, GroupId, ProjectId};

use crate::health::AppState;
use crate::repositories::group::GroupRepository;
use crate::services::group::GroupService;

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

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct LegacyGroupSummary {
    id: Uuid,
    name: String,
    project_id: Uuid,
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
    GroupService::new(GroupRepository::new(state.pool.clone()))
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
    if let Some(project_id) = query.project_id {
        let groups = list_canonical_groups_for_project(&state, &auth, project_id).await?;
        return Ok(Json(serde_json::json!({ "ok": true, "data": groups.clone(), "groups": groups })));
    }

    let service = make_service(&state);
    let groups = service.list(&auth.scope, query.limit, query.offset).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": groups })))
}

/// Canonical (`public.groups`) read for `GET /groups?projectId=...`.
///
/// Does NOT fall back to "groups with NULL project_id in the same org" —
/// `groups.project_id` stays nullable per ADR 0001, but the frontend only
/// renders project-scoped groups when filtering by `projectId`. Pre-project
/// groups exist for admin tooling and are returned by the unfiltered
/// `GET /groups` list, not by the project-scoped filter.
async fn list_canonical_groups_for_project(
    state: &AppState,
    auth: &AuthUser,
    project_id: Uuid,
) -> AppResult<Vec<LegacyGroupSummary>> {
    sqlx::query_as::<_, LegacyGroupSummary>(
        r#"SELECT
               g.id,
               g.name,
               g.project_id
           FROM public.groups g
           JOIN public.projects p
             ON p.id = g.project_id
           JOIN organization_members om
             ON om.organization_id = p.organization_id
          WHERE g.project_id = $1
            AND om.user_id = $2
            AND g.deleted_at IS NULL
            AND p.deleted_at IS NULL
          ORDER BY g.created_at ASC"#,
    )
    .bind(project_id)
    .bind(auth.scope.user_id().as_uuid())
    .fetch_all(&state.pool)
    .await
    .map_err(Into::into)
}

/// `GET /api/v1/groups/{id}` — get a single group.
async fn get_group(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let group = service.get(&auth.scope, GroupId::from(id)).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": group })))
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
    let summary = req.project_id.map(|project_id| LegacyGroupSummary {
        id: group.id.as_uuid(),
        name: group.name.clone(),
        project_id,
    });
    Ok(Json(serde_json::json!({ "ok": true, "data": group, "group": summary })))
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
    Ok(Json(serde_json::json!({ "ok": true, "data": group })))
}

/// `DELETE /api/v1/groups/{id}` — soft delete a group.
async fn delete_group(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, GroupId::from(id)).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `GET /api/v1/groups/{id}/members` — list members of a group.
async fn list_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let members = service.list_members(&auth.scope, GroupId::from(id)).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": members })))
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
    Ok(Json(serde_json::json!({ "ok": true, "data": member })))
}

/// `DELETE /api/v1/groups/{id}/members/{user_id}` — remove a member from a group.
async fn remove_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.remove_member(&auth.scope, GroupId::from(id), user_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
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

/// Test-only constructor for [`LegacyGroupSummary`]. The struct keeps private
/// fields so callers outside this module can't synthesize a legacy response by
/// accident. The `testing` module uses this helper to build shape fixtures for
/// contract tests.
///
/// Marked `#[doc(hidden)]` to keep it out of rustdoc and signal "not for
/// downstream use."
#[doc(hidden)]
pub mod test_only {
    use super::{AppResult, LegacyGroupSummary};
    use sqlx::PgPool;
    use uuid::Uuid;

    /// Build a sample [`LegacyGroupSummary`].
    pub fn sample_group_summary() -> serde_json::Value {
        serde_json::to_value(LegacyGroupSummary {
            id: Uuid::nil(),
            name: "Backend Team".to_string(),
            project_id: Uuid::nil(),
        })
        .expect("LegacyGroupSummary serializes")
    }

    /// Exposes `list_canonical_groups_for_project`'s SQL to integration
    /// tests without needing a live Axum stack. Query body is duplicated
    /// here (not delegated) so drift between the two is caught by the
    /// regression tests in `tests/nav_regression_e2e_test.rs`.
    pub async fn list_groups_canonical_for_test(
        pool: &PgPool,
        user_id: Uuid,
        project_id: Uuid,
    ) -> AppResult<Vec<serde_json::Value>> {
        let rows = sqlx::query_as::<_, LegacyGroupSummary>(
            r#"SELECT
                   g.id,
                   g.name,
                   g.project_id
               FROM public.groups g
               JOIN public.projects p
                 ON p.id = g.project_id
               JOIN organization_members om
                 ON om.organization_id = p.organization_id
              WHERE g.project_id = $1
                AND om.user_id = $2
                AND g.deleted_at IS NULL
                AND p.deleted_at IS NULL
              ORDER BY g.created_at ASC"#,
        )
        .bind(project_id)
        .bind(user_id)
        .fetch_all(pool)
        .await
        .map_err(Into::into);
        rows.map(|r| r.iter().map(|g| serde_json::to_value(g).expect("LegacyGroupSummary serializes")).collect())
    }
}
