//! Governed memory item endpoints.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, MemoryItemId};

use crate::domain::memory::MemoryScopeKind;
use crate::health::AppState;
use crate::services::memory::{
    CreateMemoryInput, MemoryService, ReclassifyScopeInput, UpdateMemoryInput, memory_data_response,
};

#[derive(Debug, Deserialize)]
struct ListMemoryQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMemoryRequest {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub redacted: bool,
    pub scope_kind: MemoryScopeKind,
    pub scope_id: Option<Uuid>,
    pub source_task_id: Option<Uuid>,
    pub source_run_id: Option<Uuid>,
    pub provenance: Option<Value>,
    pub visibility: Option<String>,
    pub ttl_expires_at: Option<DateTime<Utc>>,
    pub confidence: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMemoryRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    #[serde(default)]
    pub redacted: bool,
    pub provenance: Option<Value>,
    pub visibility: Option<String>,
    pub confidence: Option<f64>,
    pub last_verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct ExtendTtlRequest {
    pub ttl_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct ReclassifyScopeRequest {
    pub scope_kind: MemoryScopeKind,
    pub scope_id: Option<Uuid>,
    #[serde(default)]
    pub confirm_sensitive: bool,
    #[serde(default)]
    pub confirm_expansion: bool,
}

fn make_service(state: &AppState) -> MemoryService {
    MemoryService::new(state.pool.clone())
}

async fn list_memory(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListMemoryQuery>,
) -> AppResult<Json<Value>> {
    let items = make_service(&state).list(&auth.scope, query.limit, query.offset).await?;
    Ok(Json(memory_data_response(items)))
}

async fn create_memory(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateMemoryRequest>,
) -> AppResult<Json<Value>> {
    let item = make_service(&state)
        .create(
            &auth.scope,
            CreateMemoryInput {
                title: req.title,
                content: req.content,
                redacted: req.redacted,
                scope_kind: req.scope_kind,
                scope_id: req.scope_id,
                source_task_id: req.source_task_id,
                source_run_id: req.source_run_id,
                provenance: req.provenance,
                visibility: req.visibility,
                ttl_expires_at: req.ttl_expires_at,
                confidence: req.confidence,
            },
        )
        .await?;
    Ok(Json(memory_data_response(item)))
}

async fn get_memory(State(state): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let item = make_service(&state).get(&auth.scope, MemoryItemId::from(id)).await?;
    Ok(Json(memory_data_response(item)))
}

async fn read_memory_content(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let content = make_service(&state).read_content(&auth.scope, MemoryItemId::from(id)).await?;
    Ok(Json(memory_data_response(content)))
}

async fn update_memory(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateMemoryRequest>,
) -> AppResult<Json<Value>> {
    let item = make_service(&state)
        .update(
            &auth.scope,
            MemoryItemId::from(id),
            UpdateMemoryInput {
                title: req.title,
                content: req.content,
                redacted: req.redacted,
                provenance: req.provenance,
                visibility: req.visibility,
                confidence: req.confidence,
                last_verified_at: req.last_verified_at,
            },
        )
        .await?;
    Ok(Json(memory_data_response(item)))
}

async fn revoke_memory(State(state): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let item = make_service(&state).revoke(&auth.scope, MemoryItemId::from(id)).await?;
    Ok(Json(memory_data_response(item)))
}

async fn extend_memory_ttl(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ExtendTtlRequest>,
) -> AppResult<Json<Value>> {
    let item = make_service(&state).extend_ttl(&auth.scope, MemoryItemId::from(id), req.ttl_expires_at).await?;
    Ok(Json(memory_data_response(item)))
}

async fn reclassify_memory_scope(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ReclassifyScopeRequest>,
) -> AppResult<Json<Value>> {
    let item = make_service(&state)
        .reclassify_scope(
            &auth.scope,
            MemoryItemId::from(id),
            ReclassifyScopeInput {
                scope_kind: req.scope_kind,
                scope_id: req.scope_id,
                confirm_sensitive: req.confirm_sensitive,
                confirm_expansion: req.confirm_expansion,
            },
        )
        .await?;
    Ok(Json(memory_data_response(item)))
}

pub fn memory_routes() -> Router<AppState> {
    Router::new()
        .route("/context/memory-items", get(list_memory).post(create_memory))
        .route("/context/memory-items/{id}", get(get_memory).patch(update_memory))
        .route("/context/memory-items/{id}/content", get(read_memory_content))
        .route("/context/memory-items/{id}/revoke", post(revoke_memory))
        .route("/context/memory-items/{id}/extend-ttl", post(extend_memory_ttl))
        .route("/context/memory-items/{id}/reclassify-scope", post(reclassify_memory_scope))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_memory_request_deserializes_user_scope() {
        let req: CreateMemoryRequest = serde_json::from_str(
            r#"{"title":"Deploy path","content":"Use prod-ext","scope_kind":"user","redacted":false}"#,
        )
        .expect("request");
        assert_eq!(req.title, "Deploy path");
        assert_eq!(req.scope_kind, MemoryScopeKind::User);
        assert!(req.scope_id.is_none());
    }

    #[test]
    fn reclassify_scope_request_defaults_confirmation_to_false() {
        let req: ReclassifyScopeRequest =
            serde_json::from_str(r#"{"scope_kind":"team","scope_id":"550e8400-e29b-41d4-a716-446655440000"}"#)
                .expect("request");
        assert_eq!(req.scope_kind, MemoryScopeKind::Team);
        assert!(!req.confirm_sensitive);
        assert!(!req.confirm_expansion);
    }
}
