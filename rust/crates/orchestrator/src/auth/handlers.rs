use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::state::AppState;

use super::{AuthContext, auth_handler_error, require_api_auth};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefreshRequest {
    #[serde(default)]
    refresh_token: String,
}

pub async fn me(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let auth_context = match require_api_auth(&state, &headers) {
        Ok(auth_context) => auth_context,
        Err(response) => return response,
    };

    let claims = match auth_context {
        AuthContext::Session(claims) => claims,
        AuthContext::InternalToken | AuthContext::Anonymous => {
            return auth_handler_error(StatusCode::UNAUTHORIZED, "missing authentication context");
        }
    };

    let mut user = json!({
        "sub": claims.sub,
        "email": claims.email,
        "displayName": claims.display_name,
        "orgId": claims.org_id,
    });

    if let Some(provisioner) = state.provisioner.as_ref()
        && let Ok(participant) = provisioner.ensure_participant(&claims).await
    {
        user["id"] = json!(participant.id);
        user["type"] = json!(participant.kind);
        user["createdAt"] = json!(participant.created_at);
    }

    (StatusCode::OK, Json(json!({"ok": true, "user": user}))).into_response()
}

pub async fn refresh(State(state): State<AppState>, body: Bytes) -> Response {
    let Some(sessions) = state.sessions.as_ref() else {
        return auth_handler_error(StatusCode::SERVICE_UNAVAILABLE, "token management not configured");
    };

    let request: RefreshRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return auth_handler_error(StatusCode::BAD_REQUEST, "invalid request body"),
    };
    if request.refresh_token.is_empty() {
        return auth_handler_error(StatusCode::BAD_REQUEST, "refreshToken is required");
    }

    match sessions.refresh_tokens(&request.refresh_token).await {
        Ok(pair) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "accessToken": pair.access_token,
                "refreshToken": pair.refresh_token,
                "expiresAt": pair.expires_at,
            })),
        )
            .into_response(),
        Err(_) => auth_handler_error(StatusCode::UNAUTHORIZED, "invalid or expired refresh token"),
    }
}
