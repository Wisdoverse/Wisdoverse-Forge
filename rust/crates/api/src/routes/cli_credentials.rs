//! Container CLI credential endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/cli-credentials`              — list connections for the
//!   authenticated user (no ciphertext).
//! - `PUT    /api/v1/cli-credentials/{cli_tool}`   — upload or replace the
//!   file map for a Container CLI (body: `{"files": {"auth.json": "...", ...}}`).
//! - `DELETE /api/v1/cli-credentials/{cli_tool}`   — disconnect (idempotent).
//!
//! Phase-2 stopgap: lets users bootstrap OAuth credentials from their local
//! machine (copy-paste `~/.claude/credentials.json`, `~/.codex/auth.json`,
//! etc.) until the full PKCE proxy (`CliAuthProxyService`) is ported.
//!
//! Files are encrypted at rest using `LLM_ENCRYPTION_KEY`. When the key is
//! unconfigured the upload endpoint refuses the request rather than storing
//! plaintext — see `CliCredentialService::upload`.

use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;
use secrecy::{ExposeSecret, SecretString};

use crate::health::AppState;
use crate::repositories::credential::cli::CliCredentialRepository;
use crate::repositories::user_llm_config::UserLlmConfigRepository;
use crate::services::cli_credential::CliCredentialService;

const DEFAULT_OAUTH_MOUNT_ROOT: &str = "/tmp/agentforge/oauth-mounts";

#[derive(Deserialize)]
pub struct UploadRequest {
    /// Map of filename → contents (typically UTF-8 JSON strings). Nested JSON
    /// objects are rejected by `serde_json::to_string` at the service layer
    /// with a validation error so the caller gets a clean 400.
    pub files: Value,
}

fn make_service(state: &AppState) -> CliCredentialService {
    let oauth_mount_root = state
        .config
        .oauth_mount_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_OAUTH_MOUNT_ROOT));
    CliCredentialService::new(
        CliCredentialRepository::new(state.pool.clone()),
        UserLlmConfigRepository::new(state.pool.clone()),
        state.encryption_key,
        oauth_mount_root,
        clone_secret(&state.config.container_anthropic_api_key),
        clone_secret(&state.config.container_google_api_key),
        clone_secret(&state.config.container_openai_api_key),
    )
}

fn clone_secret(s: &Option<SecretString>) -> Option<SecretString> {
    s.as_ref().map(|v| SecretString::from(v.expose_secret().to_string()))
}

async fn list_cli_credentials(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    let service = make_service(&state);
    let statuses = service.list_statuses(&auth.scope).await?;
    Ok(Json(json!({ "ok": true, "connections": statuses })))
}

async fn upload_cli_credential(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(cli_tool): Path<String>,
    Json(req): Json<UploadRequest>,
) -> AppResult<Json<Value>> {
    let service = make_service(&state);
    service.upload(&auth.scope, &cli_tool, &req.files).await?;
    Ok(Json(json!({ "ok": true, "cli_tool": cli_tool, "status": "stored" })))
}

async fn delete_cli_credential(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(cli_tool): Path<String>,
) -> AppResult<Json<Value>> {
    let service = make_service(&state);
    service.remove(&auth.scope, &cli_tool).await?;
    Ok(Json(json!({ "ok": true, "cli_tool": cli_tool, "status": "deleted" })))
}

pub fn cli_credential_routes() -> Router<AppState> {
    Router::new()
        .route("/cli-credentials", get(list_cli_credentials))
        .route("/cli-credentials/{cli_tool}", axum::routing::put(upload_cli_credential).delete(delete_cli_credential))
        .route("/cli-credentials/{cli_tool}/disconnect", delete(delete_cli_credential))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_request_accepts_nested_files() {
        let req: UploadRequest =
            serde_json::from_str(r#"{"files":{"auth.json":"{\"tokens\":{\"access_token\":\"x\"}}"}}"#).unwrap();
        assert!(req.files.get("auth.json").is_some());
    }

    #[test]
    fn upload_request_rejects_missing_files() {
        let result: Result<UploadRequest, _> = serde_json::from_str("{}");
        assert!(result.is_err(), "missing `files` field should fail to deserialize");
    }
}
