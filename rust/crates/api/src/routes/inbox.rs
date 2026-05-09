//! User-facing Inbox endpoints.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;
use agentforge_db::inbox_notifications::{
    InboxNotificationRow, list_user_inbox_notifications, mark_all_inbox_notifications_read,
    mark_inbox_notification_read,
};

use crate::health::AppState;

#[derive(Deserialize)]
struct ListInboxQuery {
    #[serde(default = "default_limit")]
    limit: i64,
}

#[derive(Serialize)]
struct InboxNotificationResponse {
    id: String,
    #[serde(rename = "type")]
    notification_type: String,
    #[serde(rename = "taskId")]
    task_id: String,
    #[serde(rename = "taskTitle")]
    task_title: String,
    message: String,
    #[serde(rename = "taskHref", skip_serializing_if = "Option::is_none")]
    task_href: Option<String>,
    #[serde(rename = "ownerUserId")]
    owner_user_id: Uuid,
    read: bool,
    timestamp: i64,
}

fn default_limit() -> i64 {
    50
}

async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListInboxQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let rows = list_user_inbox_notifications(
        &state.pool,
        auth.scope.org_id().as_uuid(),
        auth.scope.user_id().as_uuid(),
        query.limit,
    )
    .await?;
    let notifications: Vec<InboxNotificationResponse> = rows.into_iter().map(InboxNotificationResponse::from).collect();
    Ok(Json(serde_json::json!({ "ok": true, "data": notifications })))
}

async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    mark_inbox_notification_read(&state.pool, auth.scope.org_id().as_uuid(), auth.scope.user_id().as_uuid(), &id)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn mark_all_read(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    mark_all_inbox_notifications_read(&state.pool, auth.scope.org_id().as_uuid(), auth.scope.user_id().as_uuid())
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

impl From<InboxNotificationRow> for InboxNotificationResponse {
    fn from(row: InboxNotificationRow) -> Self {
        Self {
            id: row.id,
            notification_type: row.notification_type,
            task_id: row.task_id.map(|id| id.to_string()).unwrap_or_default(),
            task_title: row.task_title,
            message: row.message,
            task_href: row.task_href,
            owner_user_id: row.user_id,
            read: row.read,
            timestamp: row.updated_at.timestamp_millis(),
        }
    }
}

pub fn inbox_routes() -> Router<AppState> {
    Router::new()
        .route("/inbox/notifications", get(list_notifications))
        .route("/inbox/notifications/{id}/read", post(mark_read))
        .route("/inbox/notifications/read-all", post(mark_all_read))
}
