//! User domain rules.
//!
//! This module owns account validation, authentication lifetimes, password
//! reset token policy, and user-list pagination that are independent of
//! repositories, JWT issuance, and email delivery.

use agentforge_core::{AppError, AppResult, ErrorKind, UserId};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Frontend-compatible authenticated user payload.
#[derive(Debug, Clone, Serialize)]
pub struct AuthenticatedUser {
    pub id: String,
    pub email: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Successful auth result containing the public user and token pair.
#[derive(Debug, Clone, Serialize)]
pub struct LoginResult {
    pub user: AuthenticatedUser,
    pub access_token: String,
    pub expires_in: u64,
    pub refresh_token: String,
    pub refresh_expires_in: u64,
}

/// Access token minted from a valid refresh session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefreshedAccessToken {
    access_token: String,
    expires_in: u64,
}

impl RefreshedAccessToken {
    pub(crate) fn new(access_token: String, expires_in: u64) -> Self {
        Self { access_token, expires_in }
    }

    pub(crate) fn access_token(&self) -> &str {
        &self.access_token
    }

    pub(crate) fn expires_in(&self) -> u64 {
        self.expires_in
    }
}

pub(crate) struct UserAccessPolicy;

impl UserAccessPolicy {
    pub(crate) fn ensure_self_profile(actor_user_id: UserId, target_user_id: UserId) -> AppResult<()> {
        Self::ensure_allowed(actor_user_id == target_user_id)
    }

    fn ensure_allowed(allowed: bool) -> AppResult<()> {
        if allowed { Ok(()) } else { Err(Self::forbidden()) }
    }

    fn forbidden() -> AppError {
        ErrorKind::Forbidden("forbidden".into()).into()
    }
}

/// User repository lookup and persistence error policy.
pub(crate) struct UserRepositoryPolicy;

impl UserRepositoryPolicy {
    pub(crate) fn user_not_found(id: UserId) -> AppError {
        ErrorKind::NotFound(format!("user {id}")).into()
    }

    pub(crate) fn email_already_registered() -> AppError {
        ErrorKind::Conflict("email already registered".into()).into()
    }

    pub(crate) fn personal_org_slug_allocation_failed() -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("failed to allocate unique personal organization slug")).into()
    }
}

pub(crate) fn user_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

pub(crate) fn user_members_response<T: Serialize>(members: T) -> Value {
    json!({ "ok": true, "members": members })
}

pub(crate) const PASSWORD_RESET_TTL_MINUTES: i64 = 60;

const REFRESH_COOKIE_NAME: &str = "af_rt";
const REFRESH_COOKIE_PATH: &str = "/api/v1/auth";
const REFRESH_EXPIRY_SECONDS: u64 = 7 * 24 * 60 * 60;
const REMEMBER_ME_REFRESH_EXPIRY_SECONDS: u64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenPayload {
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, Clone, Serialize)]
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

impl From<&AuthenticatedUser> for PublicUser {
    fn from(user: &AuthenticatedUser) -> Self {
        Self {
            id: user.id.clone(),
            email: user.email.clone(),
            username: user.username.clone(),
            org_id: user.org_id.clone(),
            role: user.role.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct AuthSuccessResponse {
    ok: bool,
    user: PublicUser,
    tokens: TokenPayload,
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, Clone, Serialize)]
struct RefreshSuccessResponse {
    ok: bool,
    tokens: TokenPayload,
    access_token: String,
    expires_in: u64,
}

pub(crate) fn auth_success_response_body(result: &LoginResult) -> Value {
    json!(AuthSuccessResponse {
        ok: true,
        user: PublicUser::from(&result.user),
        tokens: TokenPayload { access_token: result.access_token.clone(), expires_in: result.expires_in },
        access_token: result.access_token.clone(),
        expires_in: result.expires_in,
    })
}

pub(crate) fn auth_refresh_response(session: &RefreshedAccessToken) -> Value {
    json!(RefreshSuccessResponse {
        ok: true,
        tokens: TokenPayload { access_token: session.access_token().to_string(), expires_in: session.expires_in() },
        access_token: session.access_token().to_string(),
        expires_in: session.expires_in(),
    })
}

pub(crate) fn auth_message_response(message: &'static str) -> Value {
    json!({ "ok": true, "message": message })
}

pub(crate) fn auth_ok_response() -> Value {
    json!({ "ok": true })
}

pub(crate) fn auth_me_response(user_id: Uuid, org_id: Uuid, role: impl Serialize) -> Value {
    json!({ "ok": true, "user_id": user_id, "org_id": org_id, "role": role })
}

pub(crate) fn auth_providers_response() -> Value {
    json!({ "ok": true, "providers": Vec::<Value>::new() })
}

pub(crate) fn auth_error_response_body(code: &str, message: &str) -> Value {
    json!({ "ok": false, "error": code, "message": message })
}

/// Legacy auth route error response contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthErrorResponseContract {
    status: u16,
    code: &'static str,
    message: String,
    log_internal: bool,
}

impl AuthErrorResponseContract {
    pub(crate) fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into(), log_internal: false }
    }

    pub(crate) fn internal(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into(), log_internal: true }
    }

    pub(crate) fn status(&self) -> u16 {
        self.status
    }

    pub(crate) fn code(&self) -> &'static str {
        self.code
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn log_internal(&self) -> bool {
        self.log_internal
    }
}

pub(crate) fn auth_error_response_contract(
    err: &AppError,
    unauthorized_message: Option<&str>,
) -> AuthErrorResponseContract {
    match &err.kind {
        ErrorKind::Unauthorized => {
            AuthErrorResponseContract::new(401, "UNAUTHORIZED", unauthorized_message.unwrap_or("Unauthorized"))
        }
        ErrorKind::Forbidden(_) => AuthErrorResponseContract::new(403, "FORBIDDEN", "Forbidden"),
        ErrorKind::ForbiddenWithCode { message, .. } => {
            AuthErrorResponseContract::new(403, "FORBIDDEN", message.clone())
        }
        ErrorKind::Validation(message) => AuthErrorResponseContract::new(400, "VALIDATION_ERROR", message.clone()),
        ErrorKind::ValidationWithCode { message, .. } => {
            AuthErrorResponseContract::new(400, "VALIDATION_ERROR", message.clone())
        }
        ErrorKind::Unprocessable(message) => {
            AuthErrorResponseContract::new(422, "UNPROCESSABLE_ENTITY", message.clone())
        }
        ErrorKind::Conflict(message) => AuthErrorResponseContract::new(409, "CONFLICT", message.clone()),
        ErrorKind::NotFound(message) => AuthErrorResponseContract::new(404, "NOT_FOUND", message.clone()),
        ErrorKind::Unavailable(message) => AuthErrorResponseContract::new(503, "SERVICE_UNAVAILABLE", message.clone()),
        ErrorKind::Internal(_) => AuthErrorResponseContract::internal(500, "INTERNAL_ERROR", "Internal server error"),
    }
}

pub(crate) fn password_reset_error_response_contract(err: &AppError) -> AuthErrorResponseContract {
    match &err.kind {
        ErrorKind::Validation(message) => AuthErrorResponseContract::new(400, "VALIDATION_ERROR", message.clone()),
        ErrorKind::Internal(_) => {
            AuthErrorResponseContract::internal(503, "EMAIL_UNAVAILABLE", "Password reset email service is unavailable")
        }
        _ => auth_error_response_contract(err, None),
    }
}

pub(crate) fn missing_refresh_token_response_contract() -> AuthErrorResponseContract {
    AuthErrorResponseContract::new(401, "UNAUTHORIZED", "Missing refresh token")
}

pub(crate) fn invalid_refresh_token_response_contract() -> AuthErrorResponseContract {
    AuthErrorResponseContract::new(401, "UNAUTHORIZED", "Invalid or expired refresh token")
}

pub(crate) fn is_unauthorized_error(err: &AppError) -> bool {
    matches!(err.kind, ErrorKind::Unauthorized)
}

/// HTTP refresh-cookie policy derived from deployment runtime settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuthRefreshCookiePolicy {
    secure: bool,
}

impl AuthRefreshCookiePolicy {
    pub(crate) fn new(secure: bool) -> Self {
        Self { secure }
    }

    pub(crate) fn cookie_name(self) -> &'static str {
        REFRESH_COOKIE_NAME
    }

    pub(crate) fn refresh_cookie(self, token: &str, max_age: u64) -> String {
        let mut cookie = format!(
            "{REFRESH_COOKIE_NAME}={token}; Path={REFRESH_COOKIE_PATH}; Max-Age={max_age}; HttpOnly; SameSite=Strict"
        );
        if self.secure {
            cookie.push_str("; Secure");
        }
        cookie
    }

    pub(crate) fn clear_cookie(self) -> String {
        let mut cookie =
            format!("{REFRESH_COOKIE_NAME}=; Path={REFRESH_COOKIE_PATH}; Max-Age=0; HttpOnly; SameSite=Strict");
        if self.secure {
            cookie.push_str("; Secure");
        }
        cookie
    }
}

/// Refresh-token lifetime policy.
pub(crate) struct RefreshSessionPolicy;

impl RefreshSessionPolicy {
    pub(crate) fn refresh_expiry_seconds(remember_me: bool) -> u64 {
        if remember_me { REMEMBER_ME_REFRESH_EXPIRY_SECONDS } else { REFRESH_EXPIRY_SECONDS }
    }
}

/// User account error policy shared by auth/session services.
pub(crate) struct UserAccountPolicy;

impl UserAccountPolicy {
    pub(crate) fn invalid_credentials() -> ErrorKind {
        ErrorKind::Unauthorized
    }

    pub(crate) fn require_password_hash(hash: Option<&str>) -> AppResult<&str> {
        hash.ok_or_else(|| Self::invalid_credentials().into())
    }

    pub(crate) fn ensure_password_verified(valid: bool) -> AppResult<()> {
        if valid { Ok(()) } else { Err(Self::invalid_credentials().into()) }
    }

    pub(crate) fn invalid_refresh_token() -> ErrorKind {
        ErrorKind::Unauthorized
    }

    pub(crate) fn missing_default_org_membership() -> ErrorKind {
        ErrorKind::Validation("user has no organization membership".into())
    }

    pub(crate) fn jwt_creation_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("JWT creation failed: {err}"))
    }

    pub(crate) fn password_hashing_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("password hashing failed: {err}"))
    }

    pub(crate) fn access_token_refresh_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("access token refresh failed: {err}"))
    }

    pub(crate) fn refresh_token_creation_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("refresh token creation failed: {err}"))
    }

    pub(crate) fn password_reset_delivery_not_configured() -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("password reset delivery is not configured for this service"))
    }

    pub(crate) fn password_reset_smtp_not_configured() -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("SMTP is not configured for password reset"))
    }

    pub(crate) fn password_reset_app_url_required() -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("APP_URL is required for password reset links"))
    }

    pub(crate) fn invalid_or_expired_reset_token() -> ErrorKind {
        ErrorKind::Validation("invalid or expired reset token".into())
    }
}

/// Validated pagination request for user lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UserListPage {
    limit: i64,
    offset: i64,
}

impl UserListPage {
    pub(crate) fn new(limit: i64, offset: i64) -> Self {
        Self { limit: limit.clamp(1, 100), offset: offset.max(0) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn offset(self) -> i64 {
        self.offset
    }
}

/// Account email policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UserEmail<'a> {
    value: &'a str,
}

impl<'a> UserEmail<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        if !value.contains('@') || value.len() < 5 {
            return Err(ErrorKind::Validation("invalid email format".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Password policy for local accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UserPassword<'a> {
    value: &'a str,
}

impl<'a> UserPassword<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        if value.len() < 8 {
            return Err(ErrorKind::Validation("password must be at least 8 characters".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Password-reset lookup email. Invalid addresses are intentionally rejected
/// without an error so callers cannot enumerate users.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PasswordResetRequestEmail {
    value: String,
}

impl PasswordResetRequestEmail {
    pub(crate) fn normalize(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        UserEmail::parse(&value).ok()?;
        Some(Self { value })
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }
}

/// Raw password-reset token from a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PasswordResetToken<'a> {
    value: &'a str,
}

impl<'a> PasswordResetToken<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        let value = value.trim();
        if value.len() < 32 {
            return Err(UserAccountPolicy::invalid_or_expired_reset_token().into());
        }
        Ok(Self { value })
    }

    pub(crate) fn hash(self) -> String {
        hash_reset_token(self.value)
    }
}

/// Generated password-reset token pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GeneratedPasswordResetToken {
    value: String,
}

impl GeneratedPasswordResetToken {
    pub(crate) fn generate() -> Self {
        let mut bytes = [0_u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        Self { value: URL_SAFE_NO_PAD.encode(bytes) }
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }

    pub(crate) fn hash(&self) -> String {
        hash_reset_token(&self.value)
    }
}

pub(crate) fn derive_username(display_name: Option<&str>, email: &str) -> String {
    display_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| email.split('@').next().unwrap_or("user").to_string())
}

pub(crate) fn password_reset_email_body(reset_url: &str) -> String {
    format!(
        "Reset your Wisdoverse Forge password by opening this link:\n\n{reset_url}\n\nThis link expires in {PASSWORD_RESET_TTL_MINUTES} minutes. If you did not request a password reset, ignore this email."
    )
}

pub(crate) fn email_domain_for_log(email: &str) -> String {
    email.split('@').nth(1).unwrap_or("unknown").to_ascii_lowercase()
}

fn hash_reset_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_list_page_clamps_bounds() {
        assert_eq!(UserListPage::new(0, -10).limit(), 1);
        assert_eq!(UserListPage::new(200, 50).limit(), 100);
        assert_eq!(UserListPage::new(50, -10).offset(), 0);
        assert_eq!(UserListPage::new(50, 10).offset(), 10);
    }

    #[test]
    fn user_access_policy_owns_self_profile_forbidden_contract() {
        let actor = UserId::new();
        let target = UserId::new();

        assert!(UserAccessPolicy::ensure_self_profile(actor, actor).is_ok());
        assert!(matches!(UserAccessPolicy::ensure_self_profile(actor, target).unwrap_err().kind, ErrorKind::Forbidden(_)));
    }

    #[test]
    fn user_repository_policy_owns_lookup_and_registration_errors() {
        let user_id = UserId::new();

        assert!(matches!(
            UserRepositoryPolicy::user_not_found(user_id).kind,
            ErrorKind::NotFound(message) if message == format!("user {user_id}")
        ));
        assert!(matches!(
            UserRepositoryPolicy::email_already_registered().kind,
            ErrorKind::Conflict(message) if message == "email already registered"
        ));
        assert!(matches!(
            UserRepositoryPolicy::personal_org_slug_allocation_failed().kind,
            ErrorKind::Internal(message) if message.to_string().contains("personal organization slug")
        ));
    }

    #[test]
    fn user_email_keeps_existing_basic_policy() {
        assert_eq!(UserEmail::parse("dev@example.com").unwrap().value(), "dev@example.com");
        assert!(UserEmail::parse("a@b.c").is_ok());
        assert!(UserEmail::parse("abcd").is_err());
        assert!(UserEmail::parse("dev.example.com").is_err());
    }

    #[test]
    fn user_password_requires_minimum_length() {
        assert_eq!(UserPassword::parse("12345678").unwrap().value(), "12345678");
        assert!(UserPassword::parse("1234567").is_err());
    }

    #[test]
    fn password_reset_email_normalizes_without_enumeration_error() {
        let email = PasswordResetRequestEmail::normalize(" Dev@Example.COM ").unwrap();
        assert_eq!(email.value(), "dev@example.com");
        assert!(PasswordResetRequestEmail::normalize("invalid").is_none());
    }

    #[test]
    fn password_reset_token_hash_is_stable_and_does_not_expose_raw_token() {
        let token = PasswordResetToken::parse("abcdefghijklmnopqrstuvwxyz123456").unwrap();
        let hash = token.hash();
        assert_eq!(hash.len(), 64);
        assert_ne!(hash, "abcdefghijklmnopqrstuvwxyz123456");
        assert_eq!(hash, token.hash());
        assert!(PasswordResetToken::parse("short").is_err());
    }

    #[test]
    fn generated_password_reset_token_has_hashable_secret() {
        let token = GeneratedPasswordResetToken::generate();
        assert!(token.value().len() >= 32);
        assert_eq!(token.hash().len(), 64);
        assert_ne!(token.hash(), token.value());
    }

    #[test]
    fn reset_email_body_contains_link_and_expiry() {
        let body = password_reset_email_body("https://forge.example.com/?reset_token=abc");
        assert!(body.contains("https://forge.example.com/?reset_token=abc"));
        assert!(body.contains("60 minutes"));
    }

    #[test]
    fn username_prefers_display_name_then_email_local_part() {
        assert_eq!(derive_username(Some(" Dev User "), "dev@example.com"), "Dev User");
        assert_eq!(derive_username(Some(" "), "dev@example.com"), "dev");
        assert_eq!(derive_username(None, "dev@example.com"), "dev");
    }

    #[test]
    fn email_domain_for_log_lowercases_domain() {
        assert_eq!(email_domain_for_log("dev@Example.COM"), "example.com");
        assert_eq!(email_domain_for_log("invalid"), "unknown");
    }

    #[test]
    fn refresh_session_policy_preserves_existing_lifetimes() {
        assert_eq!(RefreshSessionPolicy::refresh_expiry_seconds(false), 7 * 24 * 60 * 60);
        assert_eq!(RefreshSessionPolicy::refresh_expiry_seconds(true), 30 * 24 * 60 * 60);
    }

    #[test]
    fn user_account_policy_owns_auth_error_contracts() {
        assert!(matches!(UserAccountPolicy::invalid_credentials(), ErrorKind::Unauthorized));
        assert!(matches!(UserAccountPolicy::invalid_refresh_token(), ErrorKind::Unauthorized));
        assert!(UserAccountPolicy::require_password_hash(Some("hash")).is_ok());
        assert!(UserAccountPolicy::require_password_hash(None).is_err());
        assert!(UserAccountPolicy::ensure_password_verified(true).is_ok());
        assert!(UserAccountPolicy::ensure_password_verified(false).is_err());
        assert!(
            format!("{}", UserAccountPolicy::missing_default_org_membership()).contains("no organization membership")
        );
        assert!(format!("{}", UserAccountPolicy::jwt_creation_failed("bad")).contains("JWT creation failed"));
        assert!(format!("{}", UserAccountPolicy::password_hashing_failed("bad")).contains("password hashing failed"));
        assert!(format!("{}", UserAccountPolicy::access_token_refresh_failed("bad")).contains("access token refresh"));
        assert!(
            format!("{}", UserAccountPolicy::refresh_token_creation_failed("bad"))
                .contains("refresh token creation failed")
        );
        assert!(
            format!("{}", UserAccountPolicy::password_reset_delivery_not_configured())
                .contains("password reset delivery is not configured")
        );
        assert!(
            format!("{}", UserAccountPolicy::password_reset_smtp_not_configured())
                .contains("SMTP is not configured for password reset")
        );
        assert!(
            format!("{}", UserAccountPolicy::password_reset_app_url_required())
                .contains("APP_URL is required for password reset links")
        );
        assert!(
            format!("{}", UserAccountPolicy::invalid_or_expired_reset_token())
                .contains("invalid or expired reset token")
        );
    }

    #[test]
    fn auth_refresh_cookie_policy_sets_expected_flags() {
        let cookie = AuthRefreshCookiePolicy::new(true).refresh_cookie("token-value", 600);
        assert!(cookie.contains("af_rt=token-value"));
        assert!(cookie.contains("Path=/api/v1/auth"));
        assert!(cookie.contains("Max-Age=600"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Secure"));
    }

    #[test]
    fn auth_refresh_cookie_policy_clears_without_secure_in_dev() {
        let cookie = AuthRefreshCookiePolicy::new(false).clear_cookie();
        assert!(cookie.contains("af_rt="));
        assert!(cookie.contains("Max-Age=0"));
        assert!(!cookie.contains("Secure"));
    }

    #[test]
    fn auth_refresh_response_serializes_token_payload() {
        let session = RefreshedAccessToken::new("new-access".to_string(), 900);
        let json = auth_refresh_response(&session);

        assert_eq!(json["ok"], true);
        assert_eq!(json["tokens"]["accessToken"], "new-access");
        assert_eq!(json["tokens"]["expiresIn"], 900);
        assert_eq!(json["access_token"], "new-access");
        assert_eq!(json["expires_in"], 900);
    }

    #[test]
    fn auth_error_contract_maps_legacy_status_codes_and_messages() {
        let unauthorized = auth_error_response_contract(&ErrorKind::Unauthorized.into(), Some("Invalid login"));
        assert_eq!(unauthorized.status(), 401);
        assert_eq!(unauthorized.code(), "UNAUTHORIZED");
        assert_eq!(unauthorized.message(), "Invalid login");
        assert!(!unauthorized.log_internal());

        let validation = auth_error_response_contract(&ErrorKind::Validation("bad input".to_string()).into(), None);
        assert_eq!(validation.status(), 400);
        assert_eq!(validation.code(), "VALIDATION_ERROR");
        assert_eq!(validation.message(), "bad input");

        let internal =
            auth_error_response_contract(&ErrorKind::Internal(anyhow::anyhow!("database unavailable")).into(), None);
        assert_eq!(internal.status(), 500);
        assert_eq!(internal.code(), "INTERNAL_ERROR");
        assert_eq!(internal.message(), "Internal server error");
        assert!(internal.log_internal());
    }

    #[test]
    fn password_reset_error_contract_preserves_email_unavailable_contract() {
        let unavailable =
            password_reset_error_response_contract(&ErrorKind::Internal(anyhow::anyhow!("smtp unavailable")).into());
        assert_eq!(unavailable.status(), 503);
        assert_eq!(unavailable.code(), "EMAIL_UNAVAILABLE");
        assert_eq!(unavailable.message(), "Password reset email service is unavailable");
        assert!(unavailable.log_internal());

        assert_eq!(missing_refresh_token_response_contract().message(), "Missing refresh token");
        assert_eq!(invalid_refresh_token_response_contract().message(), "Invalid or expired refresh token");
        assert!(is_unauthorized_error(&ErrorKind::Unauthorized.into()));
    }
}
