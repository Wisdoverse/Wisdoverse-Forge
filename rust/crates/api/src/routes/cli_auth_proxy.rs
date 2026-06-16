//! Container CLI auth proxy endpoints (nested under `/api/v1`) — PKCE OAuth
//! flow for Container CLIs that need browser login but can't receive the
//! callback themselves.
//! Ports the legacy `server/src/modules/cli-auth-proxy/*` routes.
//!
//! - `GET    /cli-auth-proxy/providers`            — available providers
//! - `GET    /cli-auth-proxy/status`               — per-provider connection status
//! - `POST   /cli-auth-proxy/{provider}/authorize` — start PKCE flow, returns URL
//! - `POST   /cli-auth-proxy/{provider}/complete-manual` — finish manual callback
//! - `DELETE /cli-auth-proxy/{provider}`           — disconnect

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse};
use axum::routing::{delete, get, post};
use serde::Deserialize;
use serde_json::Value;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
#[cfg(test)]
use crate::services::cli_auth_proxy::resolve_providers;
use crate::services::cli_auth_proxy::{
    CliAuthProxyService, cli_auth_authorize_response, cli_auth_callback_idp_error_html,
    cli_auth_callback_missing_params_html, cli_auth_callback_service_error_html, cli_auth_callback_success_html,
    cli_auth_connected_response, cli_auth_disconnected_response, cli_auth_providers_response,
    cli_auth_statuses_response,
};

fn make_service(state: &AppState) -> CliAuthProxyService {
    state.cli_auth_proxy_service()
}

/// Back-compat shim for callers that don't carry an `AppConfig`
/// (specifically the unit test below). Always returns the pure hardcoded
/// baseline — no admin overrides applied.
#[cfg(test)]
fn builtin_providers() -> Vec<crate::services::cli_auth_proxy::CliAuthProxyProvider> {
    resolve_providers(&default_test_config())
}

#[cfg(test)]
fn default_test_config() -> agentforge_core::AppConfig {
    agentforge_core::AppConfig {
        port: 4003,
        host: "0.0.0.0".into(),
        database_url: "postgres://test".into(),
        redis_url: None,
        presence_redis_enabled: false,
        nats_url: None,
        nats_agent_url: None,
        nats_container_url: None,
        nats_callout: agentforge_core::NatsCalloutConfig::default(),
        stripe: agentforge_core::StripeConfig::default(),
        jwt_secret: secrecy::SecretString::from("a".repeat(32)),
        jwt_expiry_seconds: 900,
        environment: "test".into(),
        log_level: "info".into(),
        cors_origin: None,
        static_dir: None,
        container_server_url: None,
        ollama_base_url: None,
        llm_encryption_key: None,
        container_anthropic_api_key: None,
        container_google_api_key: None,
        container_openai_api_key: None,
        codex_default_model: "gpt-5.5".to_string(),
        oauth_mount_dir: None,
        storage_provider: "local".to_string(),
        storage_local_path: "~/.agentforge/data/uploads".to_string(),
        storage_max_file_size: 10 * 1024 * 1024,
        storage_max_files_per_session: 20,
        storage_signed_url_expiry: 3600,
        minio_endpoint: None,
        minio_access_key: None,
        minio_secret_key: None,
        minio_bucket: "agentforge".to_string(),
        minio_use_ssl: false,
        minio_region: None,
        credential_sync_enabled: false,
        cli_auth_proxy_openai_client_id: None,
        cli_auth_proxy_openai_client_secret: None,
        cli_auth_proxy_openai_auth_endpoint: None,
        cli_auth_proxy_openai_token_endpoint: None,
        app_url: None,
        cli_auth_proxy_revoke_threshold: 2,
        smtp_host: None,
        smtp_port: None,
        smtp_user: None,
        smtp_password: None,
        smtp_from: None,
        smtp_secure: false,
        allow_plaintext_host_nats: false,
        host_join_binary_base_url: None,
        cli_image_auto_update_enabled: false,
        cli_image_auto_update_interval_secs: 900,
        cli_image_prune_enabled: false,
        cli_image_claude_auto_build: false,
        cli_image_npm_registry: None,
        project_clone_worker_enabled: false,
        project_clone_image: None,
        project_clone_secret_root: None,
        project_clone_timeout_secs: 600,
        github_app_id: None,
        github_app_installation_id: None,
        github_app_private_key: None,
        github_app_repo: None,
    }
}

#[derive(Deserialize)]
pub struct CompleteManualRequest {
    /// User-pasted input. Accepted formats: full callback URL, `code#state`,
    /// or bare query string. Validated by the service layer.
    pub input: String,
}

#[derive(Deserialize)]
pub struct ServerCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    /// Provider-returned error (e.g. `access_denied`). We surface it to the
    /// user instead of attempting a token exchange that would fail anyway.
    pub error: Option<String>,
    pub error_description: Option<String>,
}

async fn list_providers(State(state): State<AppState>, _auth: AuthUser) -> AppResult<Json<Value>> {
    let providers = make_service(&state).list_providers();
    Ok(Json(cli_auth_providers_response(providers)))
}

async fn status(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<Value>> {
    let statuses = make_service(&state).status(&auth.scope).await?;
    Ok(Json(cli_auth_statuses_response(statuses)))
}

async fn authorize(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(provider): Path<String>,
) -> AppResult<Json<Value>> {
    let url = make_service(&state).authorize(&auth.scope, &provider).await?;
    Ok(Json(cli_auth_authorize_response(url)))
}

async fn complete_manual(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(provider): Path<String>,
    Json(req): Json<CompleteManualRequest>,
) -> AppResult<Json<Value>> {
    make_service(&state).complete_manual(&auth.scope, &provider, &req.input).await?;
    Ok(Json(cli_auth_connected_response(&provider)))
}

async fn disconnect(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(provider): Path<String>,
) -> AppResult<Json<Value>> {
    make_service(&state).disconnect(&auth.scope, &provider).await?;
    Ok(Json(cli_auth_disconnected_response(&provider)))
}

/// Server-callback landing page. The IdP redirects the user's browser here
/// after they approve the OAuth app; we exchange the code + state for tokens
/// on their behalf. The state entry's stored `user_id` authenticates the
/// request, so this endpoint is intentionally unauthenticated — the browser
/// that completes the redirect may not carry the backend session cookie.
///
/// Response is an HTML page so the user gets a friendly landing message in
/// their browser tab; the actual UI polls the `/status` endpoint to detect
/// the connection and navigate.
async fn server_callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(params): Query<ServerCallbackQuery>,
) -> impl IntoResponse {
    if let Some(err) = params.error.as_deref() {
        let desc = params.error_description.as_deref().unwrap_or("");
        return Html(cli_auth_callback_idp_error_html(err, desc)).into_response();
    }
    let (Some(code), Some(oauth_state)) = (params.code.as_deref(), params.state.as_deref()) else {
        return Html(cli_auth_callback_missing_params_html().to_string()).into_response();
    };
    match make_service(&state).handle_server_callback(&provider, code, oauth_state).await {
        Ok(()) => Html(cli_auth_callback_success_html()).into_response(),
        Err(err) => {
            tracing::warn!(error = ?err, provider, "server-callback token exchange failed");
            Html(cli_auth_callback_service_error_html(&err.kind)).into_response()
        }
    }
}

pub fn cli_auth_proxy_routes() -> Router<AppState> {
    Router::new()
        .route("/cli-auth-proxy/providers", get(list_providers))
        .route("/cli-auth-proxy/status", get(status))
        .route("/cli-auth-proxy/{provider}/authorize", post(authorize))
        .route("/cli-auth-proxy/{provider}/complete-manual", post(complete_manual))
        .route("/cli-auth-proxy/{provider}/callback", get(server_callback))
        .route("/cli-auth-proxy/{provider}", delete(disconnect))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::cli_auth_proxy::CallbackMode;
    use secrecy::{ExposeSecret, SecretString};

    #[test]
    fn builtin_has_openai_codex() {
        let providers = builtin_providers();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].name, "openai");
        assert_eq!(providers[0].cli_tool, "codex");
        assert_eq!(providers[0].callback_mode, CallbackMode::Manual);
    }

    #[test]
    fn admin_override_client_id_keeps_manual_mode_without_app_url() {
        let mut cfg = default_test_config();
        cfg.cli_auth_proxy_openai_client_id = Some("my-app-id".into());
        cfg.cli_auth_proxy_openai_client_secret = Some(SecretString::from("my-secret".to_string()));
        let providers = resolve_providers(&cfg);
        assert_eq!(providers[0].client_id, "my-app-id");
        assert_eq!(providers[0].client_secret.as_ref().map(|s| s.expose_secret()), Some("my-secret"));
        assert_eq!(providers[0].callback_mode, CallbackMode::Manual, "app_url missing → stay manual");
        assert_eq!(providers[0].redirect_uri, "http://localhost:1455/auth/callback");
    }

    #[test]
    fn admin_override_with_app_url_flips_to_server_callback() {
        let mut cfg = default_test_config();
        cfg.cli_auth_proxy_openai_client_id = Some("my-app-id".into());
        cfg.app_url = Some("https://forge.example.com/".into());
        let providers = resolve_providers(&cfg);
        assert_eq!(providers[0].callback_mode, CallbackMode::Server);
        assert_eq!(
            providers[0].redirect_uri, "https://forge.example.com/api/v1/cli-auth-proxy/openai/callback",
            "trailing slash must be trimmed so we don't build //api/"
        );
    }

    #[test]
    fn admin_override_respects_custom_endpoints() {
        let mut cfg = default_test_config();
        cfg.cli_auth_proxy_openai_client_id = Some("my-app-id".into());
        cfg.cli_auth_proxy_openai_auth_endpoint = Some("https://idp.example/authorize".into());
        cfg.cli_auth_proxy_openai_token_endpoint = Some("https://idp.example/token".into());
        let providers = resolve_providers(&cfg);
        assert_eq!(providers[0].auth_endpoint, "https://idp.example/authorize");
        assert_eq!(providers[0].token_endpoint, "https://idp.example/token");
    }

    #[test]
    fn empty_override_strings_are_ignored() {
        let mut cfg = default_test_config();
        cfg.cli_auth_proxy_openai_client_id = Some("".into());
        cfg.cli_auth_proxy_openai_client_secret = Some(SecretString::from(String::new()));
        let providers = resolve_providers(&cfg);
        assert_eq!(
            providers[0].client_id, "app_EMoamEEZ73f0CkXaXp7hrann",
            "empty override should not clobber baseline"
        );
        assert!(providers[0].client_secret.is_none());
    }

    #[test]
    fn complete_manual_request_deserializes() {
        let req: CompleteManualRequest = serde_json::from_str(r#"{"input":"abc#xyz"}"#).unwrap();
        assert_eq!(req.input, "abc#xyz");
    }
}
