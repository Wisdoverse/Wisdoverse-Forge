//! Git credential endpoints (nested under `/api/v1`).
//!
//! - `POST   /api/v1/git-credentials`      — create
//! - `GET    /api/v1/git-credentials`      — list
//! - `GET    /api/v1/git-credentials/{id}` — get
//! - `PUT    /api/v1/git-credentials/{provider}` — legacy provider upsert
//! - `DELETE /api/v1/git-credentials/{id}` — delete

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, ErrorKind, crypto};

use crate::health::AppState;
use crate::repositories::credential::git::GitCredentialRepository;
use crate::services::git_credential::{GitCredentialService, git_credential_response, git_credentials_response};

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

/// Request body for creating a git credential.
#[derive(Deserialize)]
pub struct CreateGitCredentialRequest {
    pub name: String,
    pub provider: String,
    pub credential_type: String,
    pub remote_url: Option<String>,
    pub token: Option<String>,
}

/// Legacy request body used by the deployed bundle for provider-scoped saves.
#[derive(Deserialize)]
pub struct UpsertGitCredentialRequest {
    pub token: String,
    pub host: Option<String>,
    pub name: Option<String>,
    pub credential_type: Option<String>,
}

/// Build a GitCredentialService from shared state.
fn make_service(state: &AppState) -> GitCredentialService {
    GitCredentialService::new(GitCredentialRepository::new(state.pool.clone()))
}

fn trimmed_opt(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn encrypt_git_token(state: &AppState, token: Option<&str>) -> AppResult<Option<Vec<u8>>> {
    let Some(token) = trimmed_opt(token) else {
        return Ok(None);
    };

    let key = state.encryption_key.as_ref().ok_or_else(|| {
        ErrorKind::Validation("LLM_ENCRYPTION_KEY is not configured - refusing to store plaintext git tokens".into())
    })?;
    let encrypted = crypto::encrypt_base64(key, token)
        .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("encrypt git credential token failed: {err}")))?;
    Ok(Some(encrypted.into_bytes()))
}

/// `POST /api/git-credentials` — create a new git credential.
async fn create_git_credential(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateGitCredentialRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let token_encrypted = encrypt_git_token(&state, req.token.as_deref())?;
    let cred = service
        .upsert_for_provider(
            &auth.scope,
            &req.name,
            &req.provider,
            &req.credential_type,
            trimmed_opt(req.remote_url.as_deref()),
            token_encrypted.as_deref(),
            None,
        )
        .await?;
    Ok(Json(git_credential_response(&cred)))
}

/// `GET /api/git-credentials` — list git credentials.
async fn list_git_credentials(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let creds = service.list(&auth.scope, query.limit, query.offset).await?;
    Ok(Json(git_credentials_response(&creds)))
}

/// `GET /api/git-credentials/{id}` — get a git credential.
async fn get_git_credential(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let cred = service.get(&auth.scope, id).await?;
    Ok(Json(git_credential_response(&cred)))
}

/// `PUT /api/git-credentials/{provider}` — legacy provider-scoped upsert.
async fn upsert_git_credential(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(provider): Path<String>,
    Json(req): Json<UpsertGitCredentialRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let provider = provider.trim().to_ascii_lowercase();
    let remote_url = trimmed_opt(req.host.as_deref());
    let credential_type = trimmed_opt(req.credential_type.as_deref()).unwrap_or("token");
    let default_name = remote_url.map_or_else(|| provider.clone(), |host| format!("{provider} ({host})"));
    let name = trimmed_opt(req.name.as_deref()).unwrap_or(default_name.as_str());
    let token_encrypted = encrypt_git_token(&state, Some(req.token.as_str()))?;

    let service = make_service(&state);
    let cred = service
        .upsert_for_provider(
            &auth.scope,
            name,
            &provider,
            credential_type,
            remote_url,
            token_encrypted.as_deref(),
            None,
        )
        .await?;
    Ok(Json(git_credential_response(&cred)))
}

/// `DELETE /api/git-credentials/{id}` — delete a git credential.
async fn delete_git_credential(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Build git credential routes sub-router.
pub fn git_credential_routes() -> Router<AppState> {
    Router::new().route("/git-credentials", post(create_git_credential).get(list_git_credentials)).route(
        "/git-credentials/{id}",
        get(get_git_credential).put(upsert_git_credential).delete(delete_git_credential),
    )
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
        let req: CreateGitCredentialRequest =
            serde_json::from_str(r#"{"name": "GitHub Token", "provider": "github", "credential_type": "token"}"#)
                .unwrap();
        assert_eq!(req.name, "GitHub Token");
        assert_eq!(req.provider, "github");
        assert_eq!(req.credential_type, "token");
        assert!(req.remote_url.is_none());
        assert!(req.token.is_none());
    }

    #[test]
    fn create_request_with_remote_url() {
        let req: CreateGitCredentialRequest = serde_json::from_str(
            r#"{"name": "GitLab", "provider": "gitlab", "credential_type": "oauth", "remote_url": "https://gitlab.com/org/repo"}"#,
        )
        .unwrap();
        assert_eq!(req.remote_url.as_deref(), Some("https://gitlab.com/org/repo"));
    }

    #[test]
    fn create_request_accepts_token_field() {
        let req: CreateGitCredentialRequest = serde_json::from_str(
            r#"{"name": "GitLab", "provider": "gitlab", "credential_type": "token", "token": "gitlab-token-placeholder"}"#,
        )
        .unwrap();
        assert_eq!(req.token.as_deref(), Some("gitlab-token-placeholder"));
    }

    #[test]
    fn legacy_upsert_request_accepts_host() {
        let req: UpsertGitCredentialRequest =
            serde_json::from_str(r#"{"token": "gitlab-token-placeholder", "host": "https://gitlab.example.com"}"#)
                .unwrap();
        assert_eq!(req.token, "gitlab-token-placeholder");
        assert_eq!(req.host.as_deref(), Some("https://gitlab.example.com"));
        assert!(req.credential_type.is_none());
    }
}
