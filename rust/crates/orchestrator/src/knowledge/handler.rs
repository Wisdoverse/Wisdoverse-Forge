use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::auth;
use crate::state::AppState;

use super::errors::KnowledgeError;
use super::model::{
    CreateKnowledgeRequest, EmbeddingStatus, EntryType, KnowledgeEntry, KnowledgeFilter, SearchMode, SearchRequest,
    UpdateKnowledgeRequest,
};

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/", axum::routing::get(list).post(create))
        .route("/search", axum::routing::post(search))
        .route("/bulk-index", axum::routing::post(bulk_index))
        .route("/{id}", axum::routing::get(get).patch(update).delete(delete))
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({
            "ok": false,
            "error": message.into(),
        })),
    )
        .into_response()
}

async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut req): Json<CreateKnowledgeRequest>,
) -> Response {
    let identity = match auth::require_request_identity(&state, &headers).await {
        Ok(identity) => identity,
        Err(response) => return response,
    };
    let Some(service) = state.knowledge.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "knowledge service not configured");
    };
    if req.title.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "title is required");
    }
    if req.content.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "content is required");
    }

    let mut entry = KnowledgeEntry {
        id: String::new(),
        entry_type: req.entry_type.unwrap_or(EntryType::Document),
        title: req.title,
        content: req.content,
        source_id: req.source_id,
        source_type: req.source_type,
        source_ref: req.source_ref,
        tags: std::mem::take(&mut req.tags),
        org_id: identity.org_id,
        created_by: identity.user_id,
        embedding_status: EmbeddingStatus::Pending,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    match service.create(&mut entry).await {
        Ok(()) => (
            StatusCode::CREATED,
            Json(json!({
                "ok": true,
                "entry": entry,
            })),
        )
            .into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "failed to create entry"),
    }
}

async fn get(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let org_id = match auth::require_org_context(&state, &headers) {
        Ok(org_id) => org_id,
        Err(response) => return response,
    };
    let Some(service) = state.knowledge.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "knowledge service not configured");
    };
    match service.get_by_id(&id, &org_id).await {
        Ok(entry) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "entry": entry,
            })),
        )
            .into_response(),
        Err(KnowledgeError::NotFound) => error(StatusCode::NOT_FOUND, "entry not found"),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
    }
}

async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateKnowledgeRequest>,
) -> Response {
    let org_id = match auth::require_org_context(&state, &headers) {
        Ok(org_id) => org_id,
        Err(response) => return response,
    };
    let Some(service) = state.knowledge.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "knowledge service not configured");
    };
    match service.update(&id, &org_id, req).await {
        Ok(entry) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "entry": entry,
            })),
        )
            .into_response(),
        Err(KnowledgeError::NotFound) => error(StatusCode::NOT_FOUND, "entry not found"),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "failed to update entry"),
    }
}

async fn delete(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let org_id = match auth::require_org_context(&state, &headers) {
        Ok(org_id) => org_id,
        Err(response) => return response,
    };
    let Some(service) = state.knowledge.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "knowledge service not configured");
    };
    match service.delete(&id, &org_id).await {
        Ok(()) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(KnowledgeError::NotFound) => error(StatusCode::NOT_FOUND, "entry not found"),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "failed to delete entry"),
    }
}

async fn list(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let org_id = match auth::require_org_context(&state, &headers) {
        Ok(org_id) => org_id,
        Err(response) => return response,
    };
    let Some(service) = state.knowledge.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "knowledge service not configured");
    };
    let filter = KnowledgeFilter { org_id, entry_type: None, tags: vec![], status: None, limit: 50, offset: 0 };
    match service.list(filter).await {
        Ok(entries) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "entries": entries,
            })),
        )
            .into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
    }
}

async fn search(State(state): State<AppState>, headers: HeaderMap, Json(mut req): Json<SearchRequest>) -> Response {
    let org_id = match auth::require_org_context(&state, &headers) {
        Ok(org_id) => org_id,
        Err(response) => return response,
    };
    let Some(service) = state.knowledge.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "knowledge service not configured");
    };
    if req.query.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "query is required");
    }
    if matches!(req.mode, SearchMode::Keyword) && req.limit == 0 {
        req.limit = 20;
    }
    if req.limit == 0 {
        req.limit = 20;
    }
    req.org_id = Some(org_id);
    match service.search(req).await {
        Ok(resp) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "results": resp.results,
                "requestedMode": resp.requested_mode,
                "actualMode": resp.actual_mode,
                "degraded": resp.degraded,
                "degradedReason": resp.degraded_reason,
            })),
        )
            .into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "search failed"),
    }
}

async fn bulk_index(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let org_id = match auth::require_org_context(&state, &headers) {
        Ok(org_id) => org_id,
        Err(response) => return response,
    };
    let Some(service) = state.knowledge.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "knowledge service not configured");
    };
    match service.bulk_index(&org_id).await {
        Ok(submitted) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "submitted": submitted,
            })),
        )
            .into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "bulk index failed"),
    }
}
