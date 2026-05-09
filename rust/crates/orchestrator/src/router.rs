use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::audit;
use crate::auth;
use crate::knowledge;
use crate::mcp;
use crate::metrics;
use crate::realtime;
use crate::review;
use crate::state::AppState;
use crate::task;
use crate::team;
use crate::workflow;

pub fn create_router(state: AppState) -> Router {
    let mut api = Router::new()
        .route("/health", get(api_health))
        .route("/participants", get(list_participants).post(create_participant))
        .nest("/tasks", task::routes())
        .nest("/reviews", review::routes())
        .nest("/metrics", metrics::routes())
        .nest("/audit", audit::routes())
        .nest("/teams", team::routes())
        .nest("/workflows", workflow::routes())
        .nest("/knowledge", knowledge::routes());

    if state.sessions.is_some() {
        api = api.nest("/auth", auth::routes());
    }

    let mut router = Router::new().route("/health", get(health)).nest("/api/v1", api).nest("/ws", realtime::routes());
    if state.mcp_server.is_some() {
        router = router.route("/mcp", post(mcp::handle_request));
    }

    router.with_state(state)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateParticipantRequest {
    user_id: Option<String>,
    display_name: Option<String>,
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({"status": "healthy"}))
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"ok": false, "error": message}))).into_response()
}

#[allow(clippy::result_large_err)]
fn require_provisioner(state: &AppState) -> Result<Arc<auth::Provisioner>, Response> {
    state.provisioner.clone().ok_or_else(|| error(StatusCode::SERVICE_UNAVAILABLE, "participants not configured"))
}

async fn api_health(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(response) = auth::require_api_auth(&state, &headers) {
        return response;
    }

    let _ = state;
    Json(json!({
        "ok": true,
        "status": "healthy",
        "service": "orchestrator"
    }))
    .into_response()
}

async fn list_participants(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let org_id = match auth::require_org_context(&state, &headers) {
        Ok(org_id) => org_id,
        Err(response) => return response,
    };
    let provisioner = match require_provisioner(&state) {
        Ok(provisioner) => provisioner,
        Err(response) => return response,
    };

    match provisioner.list_participants(&org_id).await {
        Ok(participants) => (StatusCode::OK, Json(json!({"ok": true, "participants": participants}))).into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "failed to list participants"),
    }
}

async fn create_participant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateParticipantRequest>,
) -> Response {
    let org_id = match auth::require_org_context(&state, &headers) {
        Ok(org_id) => org_id,
        Err(response) => return response,
    };
    let provisioner = match require_provisioner(&state) {
        Ok(provisioner) => provisioner,
        Err(response) => return response,
    };

    let Some(user_id) = req.user_id.as_deref().map(str::trim).filter(|value| !value.is_empty()) else {
        return error(StatusCode::BAD_REQUEST, "userId is required");
    };

    match provisioner.create_or_update_internal_participant(&org_id, user_id, req.display_name.as_deref()).await {
        Ok(participant) => (StatusCode::CREATED, Json(json!({"ok": true, "participant": participant}))).into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "failed to create participant"),
    }
}
