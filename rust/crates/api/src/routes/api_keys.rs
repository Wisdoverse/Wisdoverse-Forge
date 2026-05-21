//! API key endpoints (nested under `/api/v1`).
//!
//! - `POST   /api/v1/api-keys`          — create (returns plaintext key once)
//! - `GET    /api/v1/api-keys`          — list (prefix + name + scopes only)
//! - `DELETE /api/v1/api-keys/{id}`     — revoke
//! - `POST   /api/v1/auth/api-keys`     — legacy frontend alias
//! - `GET    /api/v1/auth/api-keys`     — legacy frontend alias
//! - `DELETE /api/v1/auth/api-keys/{id}` — legacy frontend alias

use axum::extract::{Path, Query, State};
use axum::routing::{delete, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::repositories::credential::api_key::ApiKeyRepository;
use crate::services::api_key::{
    ApiKeyService, api_key_create_response, api_key_list_response, credential_delete_response,
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

/// Request body for creating an API key.
#[derive(Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Build an ApiKeyService from shared state.
fn make_service(state: &AppState) -> ApiKeyService {
    ApiKeyService::new(ApiKeyRepository::new(state.pool.clone()))
}

/// `POST /api/api-keys` — create a new API key.
async fn create_api_key(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateApiKeyRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let result = service.create_key(&auth.scope, &req.name, &req.scopes, req.expires_at).await?;
    Ok(Json(api_key_create_response(result)))
}

/// `GET /api/api-keys` — list API keys (no plaintext).
async fn list_api_keys(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let keys = service.list_keys(&auth.scope, query.limit, query.offset).await?;
    Ok(Json(api_key_list_response(&keys)))
}

/// `DELETE /api/api-keys/{id}` — revoke an API key.
async fn revoke_api_key(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.revoke_key(&auth.scope, id).await?;
    Ok(Json(credential_delete_response()))
}

/// Build API key routes sub-router.
pub fn api_key_routes() -> Router<AppState> {
    api_key_routes_at("/api-keys", "/api-keys/{id}")
}

/// Build legacy API key routes retained for cached settings frontends.
pub fn legacy_auth_api_key_routes() -> Router<AppState> {
    api_key_routes_at("/auth/api-keys", "/auth/api-keys/{id}")
}

fn api_key_routes_at(collection_path: &'static str, item_path: &'static str) -> Router<AppState> {
    Router::new()
        .route(collection_path, post(create_api_key).get(list_api_keys))
        .route(item_path, delete(revoke_api_key))
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
    fn create_request_deserialization() {
        let req: CreateApiKeyRequest =
            serde_json::from_str(r#"{"name": "CI Key", "scopes": ["read", "write"]}"#).unwrap();
        assert_eq!(req.name, "CI Key");
        assert_eq!(req.scopes, vec!["read", "write"]);
        assert!(req.expires_at.is_none());
    }

    #[test]
    fn create_request_with_expiry() {
        let req: CreateApiKeyRequest =
            serde_json::from_str(r#"{"name": "Temp Key", "scopes": ["read"], "expires_at": "2026-12-31T23:59:59Z"}"#)
                .unwrap();
        assert!(req.expires_at.is_some());
    }

    #[test]
    fn create_request_empty_scopes_default() {
        let req: CreateApiKeyRequest = serde_json::from_str(r#"{"name": "No Scopes"}"#).unwrap();
        assert!(req.scopes.is_empty());
    }

    #[test]
    fn list_response_keeps_legacy_api_keys_field() {
        let body = api_key_list_response(&[]);
        assert_eq!(body["ok"], true);
        assert_eq!(body["data"], serde_json::json!([]));
        assert_eq!(body["apiKeys"], serde_json::json!([]));
    }
}
