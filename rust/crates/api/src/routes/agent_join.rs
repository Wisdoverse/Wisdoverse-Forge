//! One-command local agent join — public endpoints.
//!
//! - `GET  /api/v1/agents/local-join/script`      — POSIX sh bootstrap
//! - `GET  /api/v1/agents/local-join/script.ps1`  — PowerShell bootstrap
//! - `POST /api/v1/agents/local-join/claim`       — exchange a pairing code
//!
//! These routes are intentionally unauthenticated (no [`AuthUser`] extractor):
//! the bootstrap runs on an operator machine that has no session, and the
//! pairing code itself is the credential. The scripts contain no secrets; the
//! claim response carries per-agent credentials and is marked `no-store`.

use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::health::AppState;
use crate::services::agent_enrollment::agent_join_claim_response;

const JOIN_SCRIPT_SH: &str = include_str!("scripts/local_join.sh");
const JOIN_SCRIPT_PS1: &str = include_str!("scripts/local_join.ps1");

/// Default download base for sidecar release binaries. Overridable per
/// deployment with `HOST_JOIN_BINARY_BASE_URL` (e.g. an internal mirror).
const DEFAULT_BINARY_BASE_URL: &str = "https://github.com/Wisdoverse/Wisdoverse-Forge/releases/latest/download";

#[derive(Debug, Deserialize)]
pub(crate) struct ClaimJoinCodeRequest {
    code: String,
    #[serde(default)]
    format: Option<String>,
}

fn render_script(template: &str, state: &AppState) -> String {
    let server_url = state
        .config
        .app_url
        .clone()
        .or_else(|| state.config.container_server_url.clone())
        .unwrap_or_default();
    let binary_base = state
        .config
        .host_join_binary_base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_BINARY_BASE_URL.to_string());
    template
        .replace("__AGENTFORGE_SERVER_URL__", server_url.trim_end_matches('/'))
        .replace("__AGENTFORGE_BINARY_BASE_URL__", binary_base.trim_end_matches('/'))
}

fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
}

fn script_response(body: String, content_type: &'static str) -> Response {
    let mut response = body.into_response();
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    no_store(response)
}

/// `GET /api/v1/agents/local-join/script` — rendered sh bootstrap (public).
async fn join_script_sh(State(state): State<AppState>) -> Response {
    script_response(render_script(JOIN_SCRIPT_SH, &state), "text/x-shellscript; charset=utf-8")
}

/// `GET /api/v1/agents/local-join/script.ps1` — rendered PowerShell bootstrap (public).
async fn join_script_ps1(State(state): State<AppState>) -> Response {
    script_response(render_script(JOIN_SCRIPT_PS1, &state), "text/plain; charset=utf-8")
}

/// `POST /api/v1/agents/local-join/claim` — exchange a pairing code for the
/// agent's sidecar environment (public; the code is the credential).
///
/// `format` selects the body the bootstrap script can consume directly:
/// `exports` (bash lines), `psexports` (PowerShell lines), default JSON.
async fn claim_join_code(
    State(state): State<AppState>,
    Json(req): Json<ClaimJoinCodeRequest>,
) -> agentforge_core::AppResult<Response> {
    let service = state.host_agent_enrollment_service();
    let claimed = service.claim(&req.code).await?;

    let response = match req.format.as_deref() {
        Some("exports") => script_response(claimed.shell_export_lines, "text/plain; charset=utf-8"),
        Some("psexports") => script_response(claimed.powershell_export_lines, "text/plain; charset=utf-8"),
        _ => Json(agent_join_claim_response(claimed)).into_response(),
    };
    Ok(no_store(response))
}

/// Build the public local-join sub-router (merged into `/api/v1`).
pub fn agent_join_routes() -> Router<AppState> {
    Router::new()
        .route("/agents/local-join/script", get(join_script_sh))
        .route("/agents/local-join/script.ps1", get(join_script_ps1))
        .route("/agents/local-join/claim", post(claim_join_code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_embed_no_unrendered_placeholders_after_render() {
        for template in [JOIN_SCRIPT_SH, JOIN_SCRIPT_PS1] {
            let rendered = template
                .replace("__AGENTFORGE_SERVER_URL__", "https://forge.example.com")
                .replace("__AGENTFORGE_BINARY_BASE_URL__", DEFAULT_BINARY_BASE_URL);
            assert!(!rendered.contains("__AGENTFORGE_"), "unrendered placeholder left in script");
            assert!(rendered.contains("local-join/claim"), "script must call the claim endpoint");
        }
    }

    #[test]
    fn sh_script_is_posix_shebang_and_warns_on_expiry() {
        assert!(JOIN_SCRIPT_SH.starts_with("#!/bin/sh\n"));
        assert!(JOIN_SCRIPT_SH.contains("Codes expire after 15 minutes"));
    }
}
