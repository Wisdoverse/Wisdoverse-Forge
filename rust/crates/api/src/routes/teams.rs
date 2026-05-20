//! Team CRUD endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/teams`      — list teams (paginated)
//! - `POST   /api/v1/teams`      — create team
//! - `GET    /api/v1/teams/{id}` — get team by ID
//! - `PATCH  /api/v1/teams/{id}` — update team
//! - `DELETE /api/v1/teams/{id}` — soft delete team

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, TeamId};

use crate::health::AppState;
use crate::repositories::identity::team::TeamRepository;
use crate::repositories::resource::permission::ResourcePermissionRepository;
use crate::services::team::{
    CreateTeamInput, TeamService, UpdateTeamInput, resource_data_response, resource_delete_response,
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

/// Request body for creating a team.
#[derive(Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
}

/// Request body for updating a team.
#[derive(Deserialize)]
pub struct UpdateTeamRequest {
    pub name: String,
}

/// Build a service instance from shared state.
fn make_service(state: &AppState) -> TeamService {
    TeamService::new(TeamRepository::new(state.pool.clone()), ResourcePermissionRepository::new(state.pool.clone()))
}

/// `GET /api/teams` — list teams for the authenticated tenant.
async fn list_teams(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let teams = service.list(&auth.scope, query.limit, query.offset).await?;
    Ok(Json(resource_data_response(teams)))
}

/// `GET /api/teams/{id}` — get a single team.
async fn get_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let team = service.get(&auth.scope, TeamId::from(id)).await?;
    Ok(Json(resource_data_response(team)))
}

/// `POST /api/teams` — create a new team.
async fn create_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateTeamRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let team = service.create(&auth.scope, CreateTeamInput { name: req.name }).await?;
    Ok(Json(resource_data_response(team)))
}

/// `PATCH /api/teams/{id}` — update a team.
async fn update_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTeamRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let team_id = TeamId::from(id);
    let service = make_service(&state);
    let team = service.update(&auth.scope, team_id, UpdateTeamInput { name: req.name }).await?;
    Ok(Json(resource_data_response(team)))
}

/// `DELETE /api/teams/{id}` — soft delete a team.
async fn delete_team(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let team_id = TeamId::from(id);
    let service = make_service(&state);
    service.delete(&auth.scope, team_id).await?;
    Ok(Json(resource_delete_response()))
}

/// Build team routes sub-router.
pub fn team_routes() -> Router<AppState> {
    Router::new()
        .route("/teams", get(list_teams).post(create_team))
        .route("/teams/{id}", get(get_team).patch(update_team).delete(delete_team))
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
        let req: CreateTeamRequest = serde_json::from_str(r#"{"name": "Engineering"}"#).unwrap();
        assert_eq!(req.name, "Engineering");
    }

    #[test]
    fn create_request_missing_name_fails() {
        let result = serde_json::from_str::<CreateTeamRequest>(r#"{}"#);
        assert!(result.is_err());
    }

    #[test]
    fn update_request_deserialization() {
        let req: UpdateTeamRequest = serde_json::from_str(r#"{"name": "Platform"}"#).unwrap();
        assert_eq!(req.name, "Platform");
    }

    #[test]
    fn update_request_missing_name_fails() {
        let result = serde_json::from_str::<UpdateTeamRequest>(r#"{}"#);
        assert!(result.is_err());
    }
}
