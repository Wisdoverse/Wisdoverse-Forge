//! Authentication routes (nested under `/api/v1`).
//!
//! These routes intentionally preserve the legacy frontend contract:
//! - login/register return `{ ok, user, tokens }`
//! - refresh token lives in the `af_rt` httpOnly cookie
//! - token payload also exposes legacy snake_case fields for compatibility

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppError, AppResult};

use crate::health::AppState;
use crate::services::auth::{AuthService, SwitchContextAxes, SwitchContextSuccessResponse};
use crate::services::user::{
    AuthErrorResponseContract, LoginResult, UserService, auth_error_response_body, auth_error_response_contract,
    auth_me_response, auth_message_response, auth_ok_response, auth_providers_response, auth_refresh_response,
    auth_success_response_body, invalid_refresh_token_response_contract, is_unauthorized_error,
    missing_refresh_token_response_contract, password_reset_error_response_contract,
};

/// Login request body.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub remember_me: bool,
}

/// Registration request body.
#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    #[serde(default, alias = "display_name", alias = "displayName")]
    pub username: Option<String>,
}

/// Forgot-password request body.
#[derive(Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

/// Reset-password request body.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

/// Context switch request body.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchContextRequest {
    pub org_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
}

/// Build a UserService from shared state.
fn make_service(state: &AppState) -> UserService {
    state.auth_user_service()
}

/// Build an AuthService from shared state.
fn make_auth_service(state: &AppState) -> AuthService {
    state.auth_service()
}

/// `POST /api/v1/auth/login` — authenticate with email and password.
pub async fn login(State(state): State<AppState>, Json(req): Json<LoginRequest>) -> Response {
    let service = make_service(&state);
    match service.login(&req.email, &req.password, req.remember_me).await {
        Ok(result) => auth_success_response(StatusCode::OK, &service, result),
        Err(err) => auth_error_response(err, Some("Invalid email or password")),
    }
}

/// `POST /api/v1/auth/register` — register a new user account.
pub async fn register(State(state): State<AppState>, Json(req): Json<RegisterRequest>) -> Response {
    let service = make_service(&state);
    match service.register(&req.email, &req.password, req.username.as_deref()).await {
        Ok(result) => auth_success_response(StatusCode::CREATED, &service, result),
        Err(err) => auth_error_response(err, None),
    }
}

/// `POST /api/v1/auth/forgot-password` — send a reset link when the account exists.
pub async fn forgot_password(State(state): State<AppState>, Json(req): Json<ForgotPasswordRequest>) -> Response {
    let service = make_service(&state);
    match service.request_password_reset(&req.email).await {
        Ok(()) => Json(auth_message_response(
            "If an account exists for that email, password reset instructions have been sent.",
        ))
        .into_response(),
        Err(err) => auth_contract_response(err, password_reset_error_response_contract),
    }
}

/// `POST /api/v1/auth/reset-password` — consume a reset token and set a password.
pub async fn reset_password(State(state): State<AppState>, Json(req): Json<ResetPasswordRequest>) -> Response {
    let service = make_service(&state);
    match service.reset_password(&req.token, &req.new_password).await {
        Ok(()) => Json(auth_ok_response()).into_response(),
        Err(err) => auth_error_response(err, None),
    }
}

/// `GET /api/me` — return current authenticated user info.
///
/// Requires a valid JWT in the `Authorization: Bearer <token>` header.
/// The `AuthUser` extractor handles token validation automatically.
pub async fn me(auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(auth_me_response(auth.scope.user_id().as_uuid(), auth.scope.org_id().as_uuid(), auth.role)))
}

/// `GET /api/v1/auth/providers` — list configured SSO providers.
///
/// Password auth is always available through `/auth/login`; this endpoint is for
/// optional external provider buttons on the login page. Returning an empty list
/// is the stable contract when no SSO providers are configured.
pub async fn providers() -> Json<serde_json::Value> {
    Json(auth_providers_response())
}

/// `POST /api/v1/auth/logout` — clear the refresh cookie and local session.
pub async fn logout(State(state): State<AppState>) -> Response {
    let service = make_service(&state);
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie_header_value(service.clear_refresh_cookie()));
    (headers, Json(auth_ok_response())).into_response()
}

/// `POST /api/v1/auth/refresh` — exchange the cookie refresh token for a new access token.
pub async fn refresh_token(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let service = make_service(&state);
    let Some(refresh_token) = read_cookie(&headers, service.refresh_cookie_name()) else {
        return auth_json_error(missing_refresh_token_response_contract());
    };

    match service.refresh_session(&refresh_token) {
        Ok(session) => Json(auth_refresh_response(&session)).into_response(),
        Err(err) => {
            if is_unauthorized_error(&err) {
                return invalid_refresh_response(&service);
            }

            auth_error_response(err, None)
        }
    }
}

fn invalid_refresh_response(service: &UserService) -> Response {
    let mut response_headers = HeaderMap::new();
    response_headers.insert(header::SET_COOKIE, cookie_header_value(service.clear_refresh_cookie()));
    let contract = invalid_refresh_token_response_contract();
    (status_code(&contract), response_headers, Json(auth_error_response_body(contract.code(), contract.message())))
        .into_response()
}

/// `POST /api/v1/auth/switch-context` — mint a new token pair for another org the user belongs to.
pub async fn switch_context(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<SwitchContextRequest>,
) -> Response {
    let axes = match SwitchContextAxes::new(req.workspace_id, req.team_id, req.project_id) {
        Ok(axes) => axes,
        Err(err) => return auth_error_response(err, None),
    };

    let service = make_auth_service(&state);
    let result = match service.switch_context(auth.scope.user_id(), req.org_id, axes).await {
        Ok(result) => result,
        Err(err) => return auth_error_response(err, None),
    };

    let mut headers = HeaderMap::new();
    let user_service = make_service(&state);
    headers.insert(
        header::SET_COOKIE,
        cookie_header_value(user_service.refresh_cookie(&result.refresh_token, result.refresh_expires_in)),
    );

    let body = SwitchContextSuccessResponse::ok(result.access_token, result.access_expires_in);

    (StatusCode::OK, headers, Json(body)).into_response()
}

fn auth_success_response(status: StatusCode, service: &UserService, result: LoginResult) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        cookie_header_value(service.refresh_cookie(&result.refresh_token, result.refresh_expires_in)),
    );

    (status, headers, Json(auth_success_response_body(&result))).into_response()
}

fn auth_error_response(err: AppError, unauthorized_message: Option<&str>) -> Response {
    let contract = auth_error_response_contract(&err, unauthorized_message);
    if contract.log_internal() {
        tracing::error!(error = ?err, "internal server error");
    }
    auth_json_error(contract)
}

fn auth_contract_response(
    err: AppError,
    contract_for: impl FnOnce(&AppError) -> AuthErrorResponseContract,
) -> Response {
    let contract = contract_for(&err);
    if contract.log_internal() {
        tracing::error!(error = ?err, "auth route error");
    }
    auth_json_error(contract)
}

fn auth_json_error(contract: AuthErrorResponseContract) -> Response {
    (status_code(&contract), Json(auth_error_response_body(contract.code(), contract.message()))).into_response()
}

fn status_code(contract: &AuthErrorResponseContract) -> StatusCode {
    StatusCode::from_u16(contract.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

fn cookie_header_value(cookie: String) -> HeaderValue {
    HeaderValue::from_str(&cookie).expect("refresh cookie header should be valid ASCII")
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookie_header| cookie_header.split(';'))
        .filter_map(|entry| {
            let (key, value) = entry.trim().split_once('=')?;
            if key == name { Some(value.to_string()) } else { None }
        })
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_request_deserialization() {
        let req: LoginRequest =
            serde_json::from_str(r#"{"email":"dev@example.com","password":"secret123","rememberMe":true}"#).unwrap();
        assert_eq!(req.email, "dev@example.com");
        assert_eq!(req.password, "secret123");
        assert!(req.remember_me);
    }

    #[test]
    fn login_request_missing_fields() {
        let result = serde_json::from_str::<LoginRequest>(r#"{"email":"dev@example.com"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn register_request_deserialization() {
        let req: RegisterRequest =
            serde_json::from_str(r#"{"email":"dev@example.com","password":"securepass","username":"Dev User"}"#)
                .unwrap();
        assert_eq!(req.email, "dev@example.com");
        assert_eq!(req.password, "securepass");
        assert_eq!(req.username.as_deref(), Some("Dev User"));
    }

    #[test]
    fn register_request_accepts_legacy_display_name() {
        let req: RegisterRequest =
            serde_json::from_str(r#"{"email":"dev@example.com","password":"securepass","display_name":"Dev User"}"#)
                .unwrap();
        assert_eq!(req.username.as_deref(), Some("Dev User"));
    }

    #[test]
    fn register_request_optional_username() {
        let req: RegisterRequest =
            serde_json::from_str(r#"{"email":"dev@example.com","password":"securepass"}"#).unwrap();
        assert!(req.username.is_none());
    }

    #[test]
    fn switch_context_request_deserialization() {
        let req: SwitchContextRequest = serde_json::from_str(
            r#"{
                "orgId":"550e8400-e29b-41d4-a716-446655440000",
                "workspaceId":"550e8400-e29b-41d4-a716-446655440001",
                "teamId":"550e8400-e29b-41d4-a716-446655440002",
                "projectId":"550e8400-e29b-41d4-a716-446655440003"
            }"#,
        )
        .unwrap();
        assert_eq!(req.org_id, Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap());
        assert_eq!(req.workspace_id, Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap()));
        assert_eq!(req.team_id, Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").unwrap()));
        assert_eq!(req.project_id, Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440003").unwrap()));
    }

    #[test]
    fn auth_success_response_serialization() {
        let result = LoginResult {
            user: crate::services::user::AuthenticatedUser {
                id: "user-1".to_string(),
                email: "dev@example.com".to_string(),
                username: "dev".to_string(),
                org_id: Some("org-1".to_string()),
                role: Some("owner".to_string()),
            },
            access_token: "access-token".to_string(),
            expires_in: 3600,
            refresh_token: "refresh-token".to_string(),
            refresh_expires_in: 86_400,
        };
        let json = auth_success_response_body(&result);
        assert_eq!(json["ok"], true);
        assert_eq!(json["user"]["username"], "dev");
        assert_eq!(json["user"]["orgId"], "org-1");
        assert_eq!(json["tokens"]["accessToken"], "access-token");
        assert_eq!(json["tokens"]["expiresIn"], 3600);
        assert_eq!(json["access_token"], "access-token");
        assert_eq!(json["expires_in"], 3600);
    }

    #[test]
    fn read_cookie_finds_named_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_static("foo=bar; af_rt=token-123; theme=dark"));
        assert_eq!(read_cookie(&headers, "af_rt").as_deref(), Some("token-123"));
    }
}
