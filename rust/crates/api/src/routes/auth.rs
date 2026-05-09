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
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, query_scalar};
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppError, AppResult, ErrorKind};

use crate::health::AppState;
use crate::repositories::user::UserRepository;
use crate::services::user::{AuthenticatedUser, LoginResult, UserService};

const REFRESH_COOKIE_NAME: &str = "af_rt";
const REFRESH_COOKIE_PATH: &str = "/api/v1/auth";
const SWITCH_CONTEXT_REFRESH_EXPIRY_SECONDS: u64 = 7 * 24 * 60 * 60;

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenPayload {
    access_token: String,
    expires_in: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicUser {
    id: String,
    email: String,
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    org_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
}

#[derive(Serialize)]
struct AuthSuccessResponse {
    ok: bool,
    user: PublicUser,
    tokens: TokenPayload,
    access_token: String,
    expires_in: u64,
}

#[derive(Serialize)]
struct RefreshSuccessResponse {
    ok: bool,
    tokens: TokenPayload,
    access_token: String,
    expires_in: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SwitchContextSuccessResponse {
    ok: bool,
    access_token: String,
    expires_in: u64,
}

/// Build a UserService from shared state.
fn make_service(state: &AppState) -> UserService {
    UserService::new(UserRepository::new(state.pool.clone()), state.jwt.clone())
}

/// `POST /api/v1/auth/login` — authenticate with email and password.
pub async fn login(State(state): State<AppState>, Json(req): Json<LoginRequest>) -> Response {
    let service = make_service(&state);
    match service.login(&req.email, &req.password, req.remember_me).await {
        Ok(result) => auth_success_response(StatusCode::OK, &state, result),
        Err(err) => auth_error_response(err, Some("Invalid email or password")),
    }
}

/// `POST /api/v1/auth/register` — register a new user account.
pub async fn register(State(state): State<AppState>, Json(req): Json<RegisterRequest>) -> Response {
    let service = make_service(&state);
    match service.register(&req.email, &req.password, req.username.as_deref()).await {
        Ok(result) => auth_success_response(StatusCode::CREATED, &state, result),
        Err(err) => auth_error_response(err, None),
    }
}

/// `POST /api/v1/auth/forgot-password` — send a reset link when the account exists.
pub async fn forgot_password(State(state): State<AppState>, Json(req): Json<ForgotPasswordRequest>) -> Response {
    let service = make_service(&state);
    match service.request_password_reset(&req.email, state.email_sender.as_ref(), state.config.app_url.as_deref()).await
    {
        Ok(()) => Json(json!({
            "ok": true,
            "message": "If an account exists for that email, password reset instructions have been sent.",
        }))
        .into_response(),
        Err(err) => match err.kind {
            ErrorKind::Validation(message) => auth_json_error(StatusCode::BAD_REQUEST, "VALIDATION_ERROR", &message),
            ErrorKind::Internal(error) => {
                tracing::error!(error = %error, "password reset email failed");
                auth_json_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "EMAIL_UNAVAILABLE",
                    "Password reset email service is unavailable",
                )
            }
            other => auth_error_response(AppError { kind: other }, None),
        },
    }
}

/// `POST /api/v1/auth/reset-password` — consume a reset token and set a password.
pub async fn reset_password(State(state): State<AppState>, Json(req): Json<ResetPasswordRequest>) -> Response {
    let service = make_service(&state);
    match service.reset_password(&req.token, &req.new_password).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(err) => auth_error_response(err, None),
    }
}

/// `GET /api/me` — return current authenticated user info.
///
/// Requires a valid JWT in the `Authorization: Bearer <token>` header.
/// The `AuthUser` extractor handles token validation automatically.
pub async fn me(auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(json!({
        "ok": true,
        "user_id": auth.scope.user_id().as_uuid(),
        "org_id": auth.scope.org_id().as_uuid(),
        "role": auth.role,
    })))
}

/// `GET /api/v1/auth/providers` — list configured SSO providers.
///
/// Password auth is always available through `/auth/login`; this endpoint is for
/// optional external provider buttons on the login page. Returning an empty list
/// is the stable contract when no SSO providers are configured.
pub async fn providers() -> Json<serde_json::Value> {
    Json(json!({
        "ok": true,
        "providers": Vec::<serde_json::Value>::new(),
    }))
}

/// `POST /api/v1/auth/logout` — clear the refresh cookie and local session.
pub async fn logout(State(state): State<AppState>) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, clear_refresh_cookie(state.config.is_production()));
    (headers, Json(json!({ "ok": true }))).into_response()
}

/// `POST /api/v1/auth/refresh` — exchange the cookie refresh token for a new access token.
pub async fn refresh_token(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(refresh_token) = read_cookie(&headers, REFRESH_COOKIE_NAME) else {
        return auth_json_error(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "Missing refresh token");
    };

    let claims = match state.jwt.verify_token(&refresh_token) {
        Ok(claims) => claims,
        Err(_) => {
            let mut response_headers = HeaderMap::new();
            response_headers.insert(header::SET_COOKIE, clear_refresh_cookie(state.config.is_production()));
            return (
                StatusCode::UNAUTHORIZED,
                response_headers,
                Json(json!({
                    "ok": false,
                    "error": "UNAUTHORIZED",
                    "message": "Invalid or expired refresh token",
                })),
            )
                .into_response();
        }
    };

    let access_token = match state.jwt.create_token_with_axes(
        claims.sub,
        claims.org,
        &claims.role,
        claims.workspace_id,
        claims.team_id,
        claims.project_id,
    ) {
        Ok(token) => token,
        Err(err) => {
            return auth_error_response(
                AppError::from(ErrorKind::Internal(anyhow::anyhow!("token creation failed: {err}"))),
                None,
            );
        }
    };

    let body = RefreshSuccessResponse {
        ok: true,
        tokens: TokenPayload { access_token: access_token.clone(), expires_in: state.jwt.expiry_seconds() },
        access_token,
        expires_in: state.jwt.expiry_seconds(),
    };

    Json(body).into_response()
}

/// `POST /api/v1/auth/switch-context` — mint a new token pair for another org the user belongs to.
pub async fn switch_context(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<SwitchContextRequest>,
) -> Response {
    let role = match query_scalar::<_, String>(
        r#"SELECT role
           FROM organization_members
          WHERE organization_id = $1
            AND user_id = $2
          LIMIT 1"#,
    )
    .bind(req.org_id)
    .bind(auth.scope.user_id().as_uuid())
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(role)) => role,
        Ok(None) => {
            return auth_json_error(StatusCode::FORBIDDEN, "FORBIDDEN", "You are not a member of this organization");
        }
        Err(err) => {
            return auth_error_response(
                AppError::from(ErrorKind::Internal(anyhow::anyhow!("context switch role lookup failed: {err}"))),
                None,
            );
        }
    };

    if let Err(err) = validate_switch_context_axes(
        &state.pool,
        auth.scope.user_id().as_uuid(),
        req.org_id,
        req.workspace_id,
        req.team_id,
        req.project_id,
    )
    .await
    {
        return auth_error_response(err, None);
    }

    let access_token = match state.jwt.create_token_with_axes(
        auth.scope.user_id().as_uuid(),
        req.org_id,
        &role,
        req.workspace_id,
        req.team_id,
        req.project_id,
    ) {
        Ok(token) => token,
        Err(err) => {
            return auth_error_response(
                AppError::from(ErrorKind::Internal(anyhow::anyhow!("context switch token creation failed: {err}"))),
                None,
            );
        }
    };

    let refresh_token = match state.jwt.create_token_with_axes_and_expiry(
        auth.scope.user_id().as_uuid(),
        req.org_id,
        &role,
        req.workspace_id,
        req.team_id,
        req.project_id,
        SWITCH_CONTEXT_REFRESH_EXPIRY_SECONDS,
    ) {
        Ok(token) => token,
        Err(err) => {
            return auth_error_response(
                AppError::from(ErrorKind::Internal(anyhow::anyhow!(
                    "context switch refresh token creation failed: {err}"
                ))),
                None,
            );
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        build_refresh_cookie(&refresh_token, SWITCH_CONTEXT_REFRESH_EXPIRY_SECONDS, state.config.is_production()),
    );

    let body = SwitchContextSuccessResponse { ok: true, access_token, expires_in: state.jwt.expiry_seconds() };

    (StatusCode::OK, headers, Json(body)).into_response()
}

async fn validate_switch_context_axes(
    pool: &PgPool,
    user_id: Uuid,
    org_id: Uuid,
    workspace_id: Option<Uuid>,
    team_id: Option<Uuid>,
    project_id: Option<Uuid>,
) -> AppResult<()> {
    if let Some(workspace_id) = workspace_id {
        let workspace_exists = query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1 FROM workspaces
                    WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL
               )"#,
        )
        .bind(workspace_id)
        .bind(org_id)
        .fetch_one(pool)
        .await?;
        if !workspace_exists {
            return Err(ErrorKind::Forbidden.into());
        }
    }

    if let Some(team_id) = team_id {
        let can_read_team = query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1
                     FROM teams t
                     JOIN team_members tm ON tm.team_id = t.id
                    WHERE t.id = $1
                      AND t.organization_id = $2
                      AND t.deleted_at IS NULL
                      AND tm.user_id = $3
               )"#,
        )
        .bind(team_id)
        .bind(org_id)
        .bind(user_id)
        .fetch_one(pool)
        .await?;
        if !can_read_team {
            return Err(ErrorKind::Forbidden.into());
        }
    }

    if let Some(project_id) = project_id {
        let Some(workspace_id) = workspace_id else {
            return Err(ErrorKind::Validation("workspaceId is required when projectId is selected".into()).into());
        };
        let can_read_project = query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1
                     FROM projects p
                    WHERE p.id = $1
                      AND p.organization_id = $2
                      AND p.workspace_id = $3
                      AND p.deleted_at IS NULL
                      AND (
                          EXISTS (
                              SELECT 1 FROM project_members pm
                               WHERE pm.project_id = p.id AND pm.user_id = $4
                          )
                          OR EXISTS (
                              SELECT 1 FROM team_members tm
                               WHERE tm.team_id = p.team_id AND tm.user_id = $4
                          )
                      )
               )"#,
        )
        .bind(project_id)
        .bind(org_id)
        .bind(workspace_id)
        .bind(user_id)
        .fetch_one(pool)
        .await?;
        if !can_read_project {
            return Err(ErrorKind::Forbidden.into());
        }
    }

    Ok(())
}

fn auth_success_response(status: StatusCode, state: &AppState, result: LoginResult) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        build_refresh_cookie(&result.refresh_token, result.refresh_expires_in, state.config.is_production()),
    );

    let user = public_user_from(result.user);
    let tokens = TokenPayload { access_token: result.access_token.clone(), expires_in: result.expires_in };
    let body = AuthSuccessResponse {
        ok: true,
        user,
        tokens,
        access_token: result.access_token,
        expires_in: result.expires_in,
    };

    (status, headers, Json(body)).into_response()
}

fn public_user_from(user: AuthenticatedUser) -> PublicUser {
    PublicUser { id: user.id, email: user.email, username: user.username, org_id: user.org_id, role: user.role }
}

fn auth_error_response(err: AppError, unauthorized_message: Option<&str>) -> Response {
    match err.kind {
        ErrorKind::Unauthorized => {
            auth_json_error(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", unauthorized_message.unwrap_or("Unauthorized"))
        }
        ErrorKind::Forbidden => auth_json_error(StatusCode::FORBIDDEN, "FORBIDDEN", "Forbidden"),
        ErrorKind::Validation(message) => auth_json_error(StatusCode::BAD_REQUEST, "VALIDATION_ERROR", &message),
        ErrorKind::Unprocessable(message) => {
            auth_json_error(StatusCode::UNPROCESSABLE_ENTITY, "UNPROCESSABLE_ENTITY", &message)
        }
        ErrorKind::Conflict(message) => auth_json_error(StatusCode::CONFLICT, "CONFLICT", &message),
        ErrorKind::NotFound(message) => auth_json_error(StatusCode::NOT_FOUND, "NOT_FOUND", &message),
        ErrorKind::Unavailable(message) => {
            auth_json_error(StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE", &message)
        }
        ErrorKind::Internal(err) => {
            tracing::error!(error = %err, "internal server error");
            auth_json_error(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", "Internal server error")
        }
    }
}

fn auth_json_error(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(json!({ "ok": false, "error": code, "message": message }))).into_response()
}

fn build_refresh_cookie(token: &str, max_age: u64, secure: bool) -> HeaderValue {
    let mut cookie = format!(
        "{REFRESH_COOKIE_NAME}={token}; Path={REFRESH_COOKIE_PATH}; Max-Age={max_age}; HttpOnly; SameSite=Strict"
    );
    if secure {
        cookie.push_str("; Secure");
    }
    HeaderValue::from_str(&cookie).expect("refresh cookie header should be valid ASCII")
}

fn clear_refresh_cookie(secure: bool) -> HeaderValue {
    let mut cookie =
        format!("{REFRESH_COOKIE_NAME}=; Path={REFRESH_COOKIE_PATH}; Max-Age=0; HttpOnly; SameSite=Strict");
    if secure {
        cookie.push_str("; Secure");
    }
    HeaderValue::from_str(&cookie).expect("clear refresh cookie header should be valid ASCII")
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
        let resp = AuthSuccessResponse {
            ok: true,
            user: PublicUser {
                id: "user-1".to_string(),
                email: "dev@example.com".to_string(),
                username: "dev".to_string(),
                org_id: Some("org-1".to_string()),
                role: Some("owner".to_string()),
            },
            tokens: TokenPayload { access_token: "access-token".to_string(), expires_in: 3600 },
            access_token: "access-token".to_string(),
            expires_in: 3600,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["user"]["username"], "dev");
        assert_eq!(json["user"]["orgId"], "org-1");
        assert_eq!(json["tokens"]["accessToken"], "access-token");
        assert_eq!(json["tokens"]["expiresIn"], 3600);
        assert_eq!(json["access_token"], "access-token");
        assert_eq!(json["expires_in"], 3600);
    }

    #[test]
    fn switch_context_response_serialization() {
        let resp = SwitchContextSuccessResponse { ok: true, access_token: "new-access".to_string(), expires_in: 900 };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["accessToken"], "new-access");
        assert_eq!(json["expiresIn"], 900);
    }

    #[test]
    fn build_refresh_cookie_sets_expected_flags() {
        let header = build_refresh_cookie("token-value", 600, true);
        let value = header.to_str().unwrap();
        assert!(value.contains("af_rt=token-value"));
        assert!(value.contains("Path=/api/v1/auth"));
        assert!(value.contains("Max-Age=600"));
        assert!(value.contains("HttpOnly"));
        assert!(value.contains("SameSite=Strict"));
        assert!(value.contains("Secure"));
    }

    #[test]
    fn clear_refresh_cookie_expires_immediately() {
        let header = clear_refresh_cookie(false);
        let value = header.to_str().unwrap();
        assert!(value.contains("af_rt="));
        assert!(value.contains("Max-Age=0"));
        assert!(!value.contains("Secure"));
    }

    #[test]
    fn read_cookie_finds_named_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_static("foo=bar; af_rt=token-123; theme=dark"));
        assert_eq!(read_cookie(&headers, "af_rt").as_deref(), Some("token-123"));
    }
}
