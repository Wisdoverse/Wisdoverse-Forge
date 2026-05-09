mod agent_directory;
mod handlers;
mod provisioner;
mod session;

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

use crate::state::AppState;

pub use agent_directory::{
    AgentDirectory, AgentParticipant, MemoryAgentDirectory, PgAgentDirectory, SharedAgentDirectory,
};
pub use provisioner::{
    MemoryParticipantStore, ParticipantProfile, PgParticipantStore, ProvisionedParticipant, Provisioner,
};
pub use session::{AccessClaims, SessionError, SessionManager, TokenPair};

#[derive(Debug, Clone)]
pub enum AuthContext {
    Session(AccessClaims),
    InternalToken,
    Anonymous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestIdentity {
    pub org_id: String,
    pub user_id: String,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/me", get(handlers::me)).route("/refresh", post(handlers::refresh))
}

#[allow(clippy::result_large_err)]
pub(crate) fn require_api_auth(state: &AppState, headers: &HeaderMap) -> Result<AuthContext, Response> {
    let auth_enabled = state.config.internal_token.is_some() || state.sessions.is_some();
    let Some(auth_header) = headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok()) else {
        return if auth_enabled {
            Err(auth_error(StatusCode::UNAUTHORIZED, "missing authorization header"))
        } else {
            Ok(AuthContext::Anonymous)
        };
    };

    let Some(token) = auth_header.strip_prefix("Bearer ") else {
        return if auth_enabled {
            Err(auth_error(StatusCode::UNAUTHORIZED, "invalid authorization format"))
        } else {
            Ok(AuthContext::Anonymous)
        };
    };

    if let Some(internal_token) = state.config.internal_token.as_deref()
        && token == internal_token
    {
        return Ok(AuthContext::InternalToken);
    }

    if let Some(sessions) = state.sessions.as_ref() {
        return match sessions.validate_access_token(token) {
            Ok(claims) => Ok(AuthContext::Session(claims)),
            Err(SessionError::InvalidToken) => Err(auth_error(StatusCode::UNAUTHORIZED, "invalid token")),
            Err(_) => Err(auth_error(StatusCode::UNAUTHORIZED, "invalid token")),
        };
    }

    if auth_enabled { Err(auth_error(StatusCode::UNAUTHORIZED, "invalid token")) } else { Ok(AuthContext::Anonymous) }
}

#[allow(clippy::result_large_err)]
pub(crate) fn require_org_context(state: &AppState, headers: &HeaderMap) -> Result<String, Response> {
    match require_api_auth(state, headers)? {
        AuthContext::Session(claims) => {
            if claims.org_id.trim().is_empty() {
                return Err(auth_handler_error(StatusCode::UNAUTHORIZED, "missing organization context"));
            }
            Ok(claims.org_id)
        }
        AuthContext::InternalToken => header_value(headers, "X-Org-ID")
            .ok_or_else(|| auth_handler_error(StatusCode::UNAUTHORIZED, "missing organization context")),
        AuthContext::Anonymous => Err(auth_handler_error(StatusCode::UNAUTHORIZED, "missing organization context")),
    }
}

pub(crate) async fn require_request_identity(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<RequestIdentity, Response> {
    match require_api_auth(state, headers)? {
        AuthContext::Session(claims) => {
            if claims.org_id.trim().is_empty() {
                return Err(auth_handler_error(StatusCode::UNAUTHORIZED, "missing organization context"));
            }
            if claims.sub.trim().is_empty() {
                return Err(auth_handler_error(StatusCode::UNAUTHORIZED, "missing user context"));
            }

            let user_id = match state.provisioner.as_ref() {
                Some(provisioner) => {
                    provisioner.ensure_participant(&claims).await.map(|participant| participant.id).map_err(|_| {
                        auth_handler_error(StatusCode::INTERNAL_SERVER_ERROR, "failed to provision participant")
                    })?
                }
                None => claims.sub.clone(),
            };
            Ok(RequestIdentity { org_id: claims.org_id, user_id })
        }
        AuthContext::InternalToken => {
            let org_id = header_value(headers, "X-Org-ID")
                .ok_or_else(|| auth_handler_error(StatusCode::UNAUTHORIZED, "missing organization context"))?;
            let raw_user_id = header_value(headers, "X-User-ID").unwrap_or_else(|| "internal".to_string());
            let user_id = match state.provisioner.as_ref() {
                Some(provisioner) => provisioner
                    .ensure_internal_participant(&org_id, &raw_user_id)
                    .await
                    .map(|participant| participant.id)
                    .map_err(|_| {
                        auth_handler_error(StatusCode::INTERNAL_SERVER_ERROR, "failed to provision participant")
                    })?,
                None => raw_user_id,
            };
            Ok(RequestIdentity { org_id, user_id })
        }
        AuthContext::Anonymous => Err(auth_handler_error(StatusCode::UNAUTHORIZED, "missing organization context")),
    }
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(crate) fn auth_handler_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"ok": false, "error": message}))).into_response()
}

fn auth_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"error": message}))).into_response()
}
