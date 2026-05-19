//! Team and project member endpoints.

use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, ProjectId, TeamId};

use crate::health::AppState;
use crate::repositories::resource::member::ResourceMemberRepository;
use crate::repositories::resource::permission::ResourcePermissionRepository;
use crate::services::resource_member::ResourceMemberService;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddMemberRequest {
    #[serde(alias = "user_id")]
    user_id: Uuid,
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InviteMemberRequest {
    email: String,
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateMemberRequest {
    role: String,
}

fn make_service(state: &AppState) -> ResourceMemberService {
    ResourceMemberService::new(
        ResourceMemberRepository::new(state.pool.clone()),
        ResourcePermissionRepository::new(state.pool.clone()),
    )
}

async fn list_team_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((org_id, team_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    let members = make_service(&state).list_team_members(&auth.scope, org_id, TeamId::from(team_id)).await?;
    Ok(Json(serde_json::json!({ "ok": true, "members": members })))
}

async fn add_team_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((org_id, team_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<AddMemberRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let member = make_service(&state)
        .add_team_member(&auth.scope, org_id, TeamId::from(team_id), req.user_id, req.role.as_deref())
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "member": member })))
}

async fn invite_team_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((org_id, team_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<InviteMemberRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let member = make_service(&state)
        .add_team_member_by_email(&auth.scope, org_id, TeamId::from(team_id), &req.email, req.role.as_deref())
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "member": member })))
}

async fn update_team_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((org_id, team_id, user_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(req): Json<UpdateMemberRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let member =
        make_service(&state).update_team_member(&auth.scope, org_id, TeamId::from(team_id), user_id, &req.role).await?;
    Ok(Json(serde_json::json!({ "ok": true, "member": member })))
}

async fn remove_team_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((org_id, team_id, user_id)): Path<(Uuid, Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    make_service(&state).remove_team_member(&auth.scope, org_id, TeamId::from(team_id), user_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn list_project_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let members = make_service(&state).list_project_members(&auth.scope, ProjectId::from(project_id)).await?;
    Ok(Json(serde_json::json!({ "ok": true, "members": members })))
}

async fn add_project_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let member = make_service(&state)
        .add_project_member(&auth.scope, ProjectId::from(project_id), req.user_id, req.role.as_deref())
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "member": member })))
}

async fn invite_project_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(req): Json<InviteMemberRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let member = make_service(&state)
        .add_project_member_by_email(&auth.scope, ProjectId::from(project_id), &req.email, req.role.as_deref())
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "member": member })))
}

async fn update_project_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((project_id, user_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateMemberRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let member = make_service(&state)
        .update_project_member(&auth.scope, ProjectId::from(project_id), user_id, &req.role)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true, "member": member })))
}

async fn remove_project_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((project_id, user_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    make_service(&state).remove_project_member(&auth.scope, ProjectId::from(project_id), user_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub fn resource_member_routes() -> Router<AppState> {
    Router::new()
        .route("/orgs/{org_id}/teams/{team_id}/members", get(list_team_members).post(add_team_member))
        .route("/orgs/{org_id}/teams/{team_id}/invites", post(invite_team_member))
        .route("/orgs/{org_id}/teams/{team_id}/members/{user_id}", patch(update_team_member).delete(remove_team_member))
        .route("/projects/{project_id}/members", get(list_project_members).post(add_project_member))
        .route("/projects/{project_id}/invites", post(invite_project_member))
        .route("/projects/{project_id}/members/{user_id}", patch(update_project_member).delete(remove_project_member))
}
