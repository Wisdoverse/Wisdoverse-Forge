//! SSH key endpoints (nested under `/api/v1`).
//!
//! - `POST   /api/v1/ssh-keys`          — add
//! - `GET    /api/v1/ssh-keys`          — list
//! - `DELETE /api/v1/ssh-keys/{id}`     — remove
//! - `POST   /api/v1/user/ssh-keys`     — legacy frontend alias
//! - `GET    /api/v1/user/ssh-keys`     — legacy frontend alias
//! - `DELETE /api/v1/user/ssh-keys/{id}` — legacy frontend alias

use axum::extract::{Path, Query, State};
use axum::routing::{delete, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::ssh_key::{
    SshKeyService, credential_delete_response, ssh_key_create_response, ssh_key_list_response,
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

/// Request body for adding an SSH key.
#[derive(Deserialize)]
pub struct AddSshKeyRequest {
    pub name: String,
    pub public_key: String,
}

/// Build an SshKeyService from shared state.
fn make_service(state: &AppState) -> SshKeyService {
    state.ssh_key_service()
}

/// `POST /api/ssh-keys` — add a new SSH key.
async fn add_ssh_key(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<AddSshKeyRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let key = service.add_key(&auth.scope, &req.name, &req.public_key).await?;
    Ok(Json(ssh_key_create_response(key)))
}

/// `GET /api/ssh-keys` — list SSH keys.
async fn list_ssh_keys(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let keys = service.list_keys(&auth.scope, query.limit, query.offset).await?;
    Ok(Json(ssh_key_list_response(&keys)))
}

/// `DELETE /api/ssh-keys/{id}` — remove an SSH key.
async fn delete_ssh_key(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete_key(&auth.scope, id).await?;
    Ok(Json(credential_delete_response()))
}

/// Build SSH key routes sub-router.
pub fn ssh_key_routes() -> Router<AppState> {
    ssh_key_routes_at("/ssh-keys", "/ssh-keys/{id}")
}

/// Build legacy SSH key routes retained for cached settings frontends.
pub fn legacy_user_ssh_key_routes() -> Router<AppState> {
    ssh_key_routes_at("/user/ssh-keys", "/user/ssh-keys/{id}")
}

fn ssh_key_routes_at(collection_path: &'static str, item_path: &'static str) -> Router<AppState> {
    Router::new().route(collection_path, post(add_ssh_key).get(list_ssh_keys)).route(item_path, delete(delete_ssh_key))
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
    fn add_ssh_key_request_deserialization() {
        let req: AddSshKeyRequest = serde_json::from_str(
            r#"{"name": "My Key", "public_key": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 dev@example.com"}"#,
        )
        .unwrap();
        assert_eq!(req.name, "My Key");
        assert!(req.public_key.starts_with("ssh-ed25519"));
    }

    #[test]
    fn list_response_keeps_legacy_keys_field() {
        let body = ssh_key_list_response(&[]);
        assert_eq!(body["ok"], true);
        assert_eq!(body["data"], serde_json::json!([]));
        assert_eq!(body["keys"], serde_json::json!([]));
    }
}
