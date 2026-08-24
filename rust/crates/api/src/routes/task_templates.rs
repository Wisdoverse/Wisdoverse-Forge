//! Task template endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/task-templates`      — list the team space's saved templates
//! - `POST   /api/v1/task-templates`      — save a template
//! - `DELETE /api/v1/task-templates/{id}` — remove a template (creator or owner/admin)

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::domain::resource::{resource_data_response, resource_delete_response};
use crate::health::AppState;
use crate::services::task_template::{CreateTaskTemplateInput, TaskTemplateService};

/// Build a TaskTemplateService from shared state.
fn make_service(state: &AppState) -> TaskTemplateService {
    state.task_template_service()
}

/// Query parameters for the list endpoint (optional project filter).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTemplatesQuery {
    pub project_id: Option<Uuid>,
}

/// `GET /task-templates` — list templates for the authenticated team space
/// (team-wide + the optional project's own).
async fn list_templates(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListTemplatesQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let templates = make_service(&state).list(&auth.scope, query.project_id).await?;
    Ok(Json(resource_data_response(templates)))
}

/// `POST /task-templates` — save a new reusable task template.
async fn create_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateTaskTemplateInput>,
) -> AppResult<Json<serde_json::Value>> {
    let template = make_service(&state).create(&auth.scope, &req).await?;
    Ok(Json(resource_data_response(template)))
}

/// `DELETE /task-templates/{id}` — remove a template the caller may manage.
async fn delete_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    make_service(&state).delete(&auth.scope, id, &auth.role).await?;
    Ok(Json(resource_delete_response()))
}

/// Build task template routes sub-router.
pub fn task_template_routes() -> Router<AppState> {
    Router::new()
        .route("/task-templates", get(list_templates).post(create_template))
        .route("/task-templates/{id}", axum::routing::delete(delete_template))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_input_fills_defaults() {
        let input: CreateTaskTemplateInput =
            serde_json::from_str(r#"{"name":"Release","title":"Cut a release"}"#).unwrap();
        assert_eq!(input.priority, "normal");
        assert_eq!(input.description, "");
        assert!(!input.requires_approval);

        let input: CreateTaskTemplateInput = serde_json::from_str(
            r#"{"name":"Release","title":"Cut a release","priority":"high","requiresApproval":true}"#,
        )
        .unwrap();
        assert_eq!(input.priority, "high");
        assert!(input.requires_approval);
    }
}
