use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::auth;
use crate::state::AppState;

use super::errors::TeamError;
use super::model::{AddMemberRequest, CreateTeamRequest, Team, TeamMember, TeamRole, UpdateTeamRequest};
use super::store::Store;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/", axum::routing::get(list).post(create))
        .route("/{id}", axum::routing::get(get).patch(update).delete(delete))
        .route("/{id}/members", axum::routing::post(add_member))
        .route("/{id}/members/{pid}", axum::routing::delete(remove_member))
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"ok": false, "error": message}))).into_response()
}

#[allow(clippy::result_large_err)]
fn require_store(state: &AppState) -> Result<Arc<dyn Store>, Response> {
    state.team_store.clone().ok_or_else(|| error(StatusCode::SERVICE_UNAVAILABLE, "database not configured"))
}

fn map_error(err: TeamError, not_found_message: &str) -> Response {
    match err {
        TeamError::NotFound => error(StatusCode::NOT_FOUND, not_found_message),
        TeamError::InvalidInput(message) => error(StatusCode::BAD_REQUEST, &message),
        TeamError::Internal(message) => error(StatusCode::INTERNAL_SERVER_ERROR, &message),
    }
}

async fn list(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    match store.list(&identity.org_id).await {
        Ok(teams) => (StatusCode::OK, Json(json!({"ok": true, "teams": teams}))).into_response(),
        Err(err) => map_error(err, "team not found"),
    }
}

async fn create(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<CreateTeamRequest>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    let Some(name) = req.name.filter(|name| !name.trim().is_empty()) else {
        return error(StatusCode::BAD_REQUEST, "name is required");
    };

    let mut team = Team {
        id: String::new(),
        name,
        org_id: identity.org_id,
        created_by: identity.user_id,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    match store.create(&mut team).await {
        Ok(()) => (StatusCode::CREATED, Json(json!({"ok": true, "team": team}))).into_response(),
        Err(err) => map_error(err, "team not found"),
    }
}

async fn get(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    match store.get_by_id(&id, &identity.org_id).await {
        Ok(team) => (StatusCode::OK, Json(json!({"ok": true, "team": team}))).into_response(),
        Err(err) => map_error(err, "team not found"),
    }
}

async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateTeamRequest>,
) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    if req.name.as_ref().is_some_and(|name| name.trim().is_empty()) {
        return error(StatusCode::BAD_REQUEST, "name is required");
    }

    match store.update(&id, &identity.org_id, req).await {
        Ok(()) => match store.get_by_id(&id, &identity.org_id).await {
            Ok(team) => (StatusCode::OK, Json(json!({"ok": true, "team": team}))).into_response(),
            Err(err) => map_error(err, "team not found"),
        },
        Err(err) => map_error(err, "team not found"),
    }
}

async fn delete(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    match store.delete(&id, &identity.org_id).await {
        Ok(()) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(err) => map_error(err, "team not found"),
    }
}

async fn add_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<AddMemberRequest>,
) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    if let Err(err) = store.get_by_id(&id, &identity.org_id).await {
        return map_error(err, "team not found");
    }
    let Some(participant_id) = req.participant_id.filter(|participant_id| !participant_id.trim().is_empty()) else {
        return error(StatusCode::BAD_REQUEST, "participantId is required");
    };

    let mut member = TeamMember {
        team_id: String::new(),
        participant_id,
        role: req.role.unwrap_or(TeamRole::Member),
        joined_at: chrono::Utc::now(),
    };
    match store.add_member(&id, &mut member).await {
        Ok(()) => (StatusCode::CREATED, Json(json!({"ok": true, "member": member}))).into_response(),
        Err(err) => map_error(err, "team not found"),
    }
}

async fn remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((id, pid)): Path<(String, String)>,
) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let store = match require_store(&state) {
        Ok(store) => store,
        Err(response) => return response,
    };

    if let Err(err) = store.get_by_id(&id, &identity.org_id).await {
        return map_error(err, "team not found");
    }
    match store.remove_member(&id, &pid).await {
        Ok(()) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(err) => map_error(err, "member not found"),
    }
}
