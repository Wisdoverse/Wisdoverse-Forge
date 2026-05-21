//! User-facing Inbox endpoints.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::inbox::{InboxService, inbox_data_response, inbox_ok_response};

#[derive(Deserialize)]
struct ListInboxQuery {
    limit: Option<i64>,
}

fn make_service(state: &AppState) -> InboxService {
    InboxService::from_pool(state.pool.clone())
}

async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListInboxQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let notifications = make_service(&state).list(&auth.scope, query.limit).await?;
    Ok(Json(inbox_data_response(notifications)))
}

async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    make_service(&state).mark_read(&auth.scope, &id).await?;
    Ok(Json(inbox_ok_response()))
}

async fn mark_all_read(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    make_service(&state).mark_all_read(&auth.scope).await?;
    Ok(Json(inbox_ok_response()))
}

pub fn inbox_routes() -> Router<AppState> {
    Router::new()
        .route("/inbox/notifications", get(list_notifications))
        .route("/inbox/notifications/{id}/read", post(mark_read))
        .route("/inbox/notifications/read-all", post(mark_all_read))
}
