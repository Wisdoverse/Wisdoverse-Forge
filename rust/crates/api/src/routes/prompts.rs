//! Prompt endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/prompts`      — list prompts (query: shared?, tags?)
//! - `POST   /api/v1/prompts`      — create prompt
//! - `GET    /api/v1/prompts/{id}` — get prompt
//! - `PATCH  /api/v1/prompts/{id}` — update prompt
//! - `DELETE /api/v1/prompts/{id}` — delete prompt

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::repositories::prompt::PromptRepository;
use crate::services::prompt_library::{
    PromptLibraryService, prompt_library_data_response, prompt_library_delete_response,
};

/// Query parameters for listing prompts.
#[derive(Deserialize)]
pub struct ListPromptsQuery {
    pub shared: Option<bool>,
    pub tags: Option<String>,
}

/// Request body for creating a prompt.
#[derive(Deserialize)]
pub struct CreatePromptRequest {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub is_shared: bool,
}

/// Request body for updating a prompt.
#[derive(Deserialize)]
pub struct UpdatePromptRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub is_shared: Option<bool>,
}

/// Build a PromptLibraryService from shared state.
fn make_service(state: &AppState) -> PromptLibraryService {
    PromptLibraryService::new(PromptRepository::new(state.pool.clone()))
}

/// `GET /api/v1/prompts` — list prompts.
async fn list_prompts(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListPromptsQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let tags = query.tags.map(|t| t.split(',').map(|s| s.trim().to_string()).collect::<Vec<_>>());
    let prompts = service.list(&auth.scope, query.shared, tags).await?;
    Ok(Json(prompt_library_data_response(prompts)))
}

/// `POST /api/v1/prompts` — create a prompt.
async fn create_prompt(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreatePromptRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let prompt = service.create(&auth.scope, &req.title, &req.content, &req.tags, req.is_shared).await?;
    Ok(Json(prompt_library_data_response(prompt)))
}

/// `GET /api/v1/prompts/{id}` — get a prompt.
async fn get_prompt(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let prompt = service.get(&auth.scope, id).await?;
    Ok(Json(prompt_library_data_response(prompt)))
}

/// `PATCH /api/v1/prompts/{id}` — update a prompt.
async fn update_prompt(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePromptRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let prompt = service
        .update(&auth.scope, id, req.title.as_deref(), req.content.as_deref(), req.tags.as_deref(), req.is_shared)
        .await?;
    Ok(Json(prompt_library_data_response(prompt)))
}

/// `DELETE /api/v1/prompts/{id}` — delete a prompt.
async fn delete_prompt(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, id).await?;
    Ok(Json(prompt_library_delete_response()))
}

/// Build prompt routes sub-router.
pub fn prompt_routes() -> Router<AppState> {
    Router::new()
        .route("/prompts", get(list_prompts).post(create_prompt))
        .route("/prompts/{id}", get(get_prompt).patch(update_prompt).delete(delete_prompt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_request_deserialization() {
        let req: CreatePromptRequest =
            serde_json::from_str(r#"{"title": "Test", "content": "Hello world", "tags": ["test"], "is_shared": true}"#)
                .unwrap();
        assert_eq!(req.title, "Test");
        assert_eq!(req.content, "Hello world");
        assert_eq!(req.tags, vec!["test"]);
        assert!(req.is_shared);
    }

    #[test]
    fn create_request_minimal() {
        let req: CreatePromptRequest = serde_json::from_str(r#"{"title": "Test", "content": "Hello"}"#).unwrap();
        assert!(req.tags.is_empty());
        assert!(!req.is_shared);
    }

    #[test]
    fn update_request_partial() {
        let req: UpdatePromptRequest = serde_json::from_str(r#"{"title": "New Title"}"#).unwrap();
        assert_eq!(req.title.as_deref(), Some("New Title"));
        assert!(req.content.is_none());
        assert!(req.tags.is_none());
        assert!(req.is_shared.is_none());
    }

    #[test]
    fn list_query_deserialization() {
        let query: ListPromptsQuery = serde_json::from_str(r#"{"shared": true, "tags": "review,security"}"#).unwrap();
        assert_eq!(query.shared, Some(true));
        assert_eq!(query.tags.as_deref(), Some("review,security"));
    }
}
