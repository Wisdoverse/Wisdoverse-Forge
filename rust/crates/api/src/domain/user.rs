//! User domain rules.
//!
//! This module owns account validation, authentication lifetimes, password
//! reset token policy, and user-list pagination that are independent of
//! repositories, JWT issuance, and email delivery.

use agentforge_core::{AppResult, ErrorKind};
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
pub(crate) const SWITCH_CONTEXT_REFRESH_EXPIRY_SECONDS: u64 = 7 * 24 * 60 * 60;

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SwitchContextSuccessResponse {
    ok: bool,
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

pub(crate) fn auth_switch_context_response(access_token: String, expires_in: u64) -> Value {
    json!(SwitchContextSuccessResponse { ok: true, access_token, expires_in })
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
    pub(crate) fn missing_default_org_membership() -> ErrorKind {
        ErrorKind::Validation("user has no organization membership".into())
    }

    pub(crate) fn invalid_or_expired_reset_token() -> ErrorKind {
        ErrorKind::Validation("invalid or expired reset token".into())
    }
}

/// Validated context axes for an auth context switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SwitchContextAxes {
    workspace_id: Option<Uuid>,
    team_id: Option<Uuid>,
    project_id: Option<Uuid>,
}

impl SwitchContextAxes {
    pub(crate) fn new(workspace_id: Option<Uuid>, team_id: Option<Uuid>, project_id: Option<Uuid>) -> AppResult<Self> {
        if project_id.is_some() && workspace_id.is_none() {
            return Err(ErrorKind::Validation("workspaceId is required when projectId is selected".into()).into());
        }

        Ok(Self { workspace_id, team_id, project_id })
    }

    pub(crate) fn workspace_id(&self) -> Option<Uuid> {
        self.workspace_id
    }

    pub(crate) fn team_id(&self) -> Option<Uuid> {
        self.team_id
    }

    pub(crate) fn project_id(&self) -> Option<Uuid> {
        self.project_id
    }

    pub(crate) fn project_workspace_pair(&self) -> Option<(Uuid, Uuid)> {
        match (self.project_id, self.workspace_id) {
            (Some(project_id), Some(workspace_id)) => Some((project_id, workspace_id)),
            _ => None,
        }
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
    fn switch_context_axes_allow_workspace_team_and_project_selection() {
        let workspace_id = Uuid::new_v4();
        let team_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();

        let axes = SwitchContextAxes::new(Some(workspace_id), Some(team_id), Some(project_id)).unwrap();

        assert_eq!(axes.workspace_id(), Some(workspace_id));
        assert_eq!(axes.team_id(), Some(team_id));
        assert_eq!(axes.project_id(), Some(project_id));
        assert_eq!(axes.project_workspace_pair(), Some((project_id, workspace_id)));
    }

    #[test]
    fn switch_context_axes_require_workspace_for_project_selection() {
        let err = SwitchContextAxes::new(None, None, Some(Uuid::new_v4())).unwrap_err();

        assert!(
            matches!(err.kind, ErrorKind::Validation(message) if message == "workspaceId is required when projectId is selected")
        );
    }

    #[test]
    fn switch_context_axes_allow_org_only_and_workspace_only_selection() {
        let workspace_id = Uuid::new_v4();

        let org_only = SwitchContextAxes::new(None, None, None).unwrap();
        let workspace_only = SwitchContextAxes::new(Some(workspace_id), None, None).unwrap();

        assert_eq!(org_only.project_workspace_pair(), None);
        assert_eq!(workspace_only.workspace_id(), Some(workspace_id));
        assert_eq!(workspace_only.project_workspace_pair(), None);
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
        assert!(
            format!("{}", UserAccountPolicy::missing_default_org_membership()).contains("no organization membership")
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
}
