//! Recurring task endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/recurring-tasks`      — list schedules
//! - `POST   /api/v1/recurring-tasks`      — create a schedule
//! - `PATCH  /api/v1/recurring-tasks/{id}` — enable/disable
//! - `DELETE /api/v1/recurring-tasks/{id}` — remove a schedule

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::domain::resource::{resource_data_response, resource_delete_response};
use crate::health::AppState;
use crate::services::recurring_task::{CreateRecurringTaskInput, RecurringTaskService, UpdateRecurringTaskInput};

fn make_service(state: &AppState) -> RecurringTaskService {
    state.recurring_task_service()
}

/// `GET /recurring-tasks` — list schedules for the team space.
async fn list_recurring_tasks(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let rows = make_service(&state).list(&auth.scope).await?;
    Ok(Json(resource_data_response(rows)))
}

/// `POST /recurring-tasks` — create a schedule.
async fn create_recurring_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateRecurringTaskInput>,
) -> AppResult<Json<serde_json::Value>> {
    let row = make_service(&state).create(&auth.scope, &req).await?;
    Ok(Json(resource_data_response(row)))
}

/// `PATCH /recurring-tasks/{id}` — enable or disable a schedule.
async fn update_recurring_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRecurringTaskInput>,
) -> AppResult<Json<serde_json::Value>> {
    let row = make_service(&state).set_enabled(&auth.scope, id, req.enabled).await?;
    Ok(Json(resource_data_response(row)))
}

/// `DELETE /recurring-tasks/{id}` — remove a schedule.
async fn delete_recurring_task(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    make_service(&state).delete(&auth.scope, id).await?;
    Ok(Json(resource_delete_response()))
}

/// Build recurring task routes sub-router.
pub fn recurring_task_routes() -> Router<AppState> {
    Router::new()
        .route("/recurring-tasks", get(list_recurring_tasks).post(create_recurring_task))
        .route("/recurring-tasks/{id}", axum::routing::patch(update_recurring_task).delete(delete_recurring_task))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_input_deserializes_camel_case() {
        let input: CreateRecurringTaskInput = serde_json::from_str(
            r#"{"name":"Daily","title":"Daily summary","projectId":"00000000-0000-0000-0000-000000000001","groupId":"00000000-0000-0000-0000-000000000002","cadenceMinutes":1440}"#,
        )
        .unwrap();
        assert_eq!(input.cadence_minutes, 1_440);
        assert_eq!(input.priority, "normal");
        assert!(!input.requires_approval);
    }
}
