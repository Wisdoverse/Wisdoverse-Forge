//! User service — login, registration, and profile management.

use std::sync::Arc;

use agentforge_auth::JwtManager;
use agentforge_core::{AppResult, ErrorKind, TenantScope, UserId};
use agentforge_db::entities::User;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::repositories::user::{OrgUserSearchResult, UserRepository};
use crate::services::email::{EmailMessage, EmailSender};

/// Business logic layer for user operations.
pub struct UserService {
    repo: UserRepository,
    jwt: Arc<JwtManager>,
}

const REFRESH_EXPIRY_SECONDS: u64 = 7 * 24 * 60 * 60;
const REMEMBER_ME_REFRESH_EXPIRY_SECONDS: u64 = 30 * 24 * 60 * 60;
const PASSWORD_RESET_TTL_MINUTES: i64 = 60;

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

impl UserService {
    pub fn new(repo: UserRepository, jwt: Arc<JwtManager>) -> Self {
        Self { repo, jwt }
    }

    /// Authenticate with email + password and return a JWT.
    pub async fn login(&self, email: &str, password: &str, remember_me: bool) -> AppResult<LoginResult> {
        // 1. Find user by email
        let user = self.repo.find_by_email(email).await?.ok_or(ErrorKind::Unauthorized)?;

        // 2. Verify password
        let hash = user.password_hash.as_ref().ok_or(ErrorKind::Unauthorized)?;
        let verification = agentforge_auth::password::verify_password_compat(password, hash);
        if !verification.valid {
            return Err(ErrorKind::Unauthorized.into());
        }

        if verification.needs_upgrade {
            match agentforge_auth::password::hash_password(password) {
                Ok(new_hash) => {
                    if let Err(err) = self.repo.update_password_hash(user.id, &new_hash).await {
                        tracing::warn!(error = ?err, user_id = %user.id.as_uuid(), "Failed to upgrade legacy password hash");
                    }
                }
                Err(err) => {
                    tracing::warn!(error = ?err, user_id = %user.id.as_uuid(), "Failed to rehash legacy password");
                }
            }
        }

        // 3. Heal legacy users: if their email domain has a canonical org and
        //    they aren't a member yet, add them. Failures are logged so a
        //    transient DB hiccup can't lock anyone out of login.
        if let Err(err) = self.repo.ensure_domain_membership(user.id, &user.email).await {
            tracing::warn!(error = ?err, user_id = %user.id.as_uuid(), "Failed to backfill domain membership");
        }

        // 4. Get user's default org membership + role
        let (org_id, role) = self
            .repo
            .find_default_org(user.id)
            .await?
            .ok_or_else(|| ErrorKind::Validation("user has no organization membership".into()))?;

        // 5. Create JWT
        let token = self
            .jwt
            .create_token(user.id.as_uuid(), org_id, &role)
            .map_err(|e| ErrorKind::Internal(anyhow::anyhow!("JWT creation failed: {e}")))?;

        // 6. Update last_login (fire-and-forget, don't fail the login)
        if let Err(err) = self.repo.update_last_login(user.id).await {
            tracing::warn!(error = ?err, user_id = %user.id.as_uuid(), "Failed to update last_login_at");
        }

        self.build_auth_result(&user, org_id, &role, token, remember_me)
    }

    /// Register a new user account.
    pub async fn register(&self, email: &str, password: &str, display_name: Option<&str>) -> AppResult<LoginResult> {
        // Validate email format (basic check)
        if validate_email(email).is_err() {
            return Err(ErrorKind::Validation("invalid email format".into()).into());
        }
        // Validate password length
        if validate_password(password).is_err() {
            return Err(ErrorKind::Validation("password must be at least 8 characters".into()).into());
        }

        let hash = agentforge_auth::password::hash_password(password)
            .map_err(|e| ErrorKind::Internal(anyhow::anyhow!("password hashing failed: {e}")))?;

        let user = self.repo.create(email, &hash, display_name).await?;
        let (org_id, role) = self
            .repo
            .find_default_org(user.id)
            .await?
            .ok_or_else(|| ErrorKind::Validation("user has no organization membership".into()))?;

        let access_token = self
            .jwt
            .create_token(user.id.as_uuid(), org_id, &role)
            .map_err(|e| ErrorKind::Internal(anyhow::anyhow!("JWT creation failed: {e}")))?;

        self.build_auth_result(&user, org_id, &role, access_token, false)
    }

    /// Request a password reset link. The response is intentionally generic so
    /// callers cannot enumerate users by email address.
    pub async fn request_password_reset(
        &self,
        email: &str,
        email_sender: &dyn EmailSender,
        app_url: Option<&str>,
    ) -> AppResult<()> {
        if !email_sender.is_configured() {
            return Err(ErrorKind::Internal(anyhow::anyhow!("SMTP is not configured for password reset")).into());
        }
        let app_url = app_url
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("APP_URL is required for password reset links")))?;

        let email = email.trim().to_ascii_lowercase();
        if validate_email(&email).is_err() {
            return Ok(());
        }

        let Some(user) = self.repo.find_by_email(&email).await? else {
            return Ok(());
        };

        let token = generate_reset_token();
        let token_hash = hash_reset_token(&token);
        let expires_at = Utc::now() + Duration::minutes(PASSWORD_RESET_TTL_MINUTES);
        self.repo.store_password_reset_token(user.id, &token_hash, expires_at).await?;

        let reset_url = format!("{}/login?reset_token={token}", app_url.trim_end_matches('/'));
        let message = EmailMessage {
            to: user.email.clone(),
            subject: "Reset your Wisdoverse Forge password".to_string(),
            body: password_reset_email_body(&reset_url),
        };

        if let Err(err) = email_sender.send(message).await {
            let _ = self.repo.delete_password_reset_token(&token_hash).await;
            return Err(err);
        }

        tracing::info!(
            user_id = %user.id.as_uuid(),
            email_domain = %email_domain_for_log(&user.email),
            "password reset email sent"
        );
        Ok(())
    }

    /// Consume a password reset token and set a new password.
    pub async fn reset_password(&self, token: &str, new_password: &str) -> AppResult<()> {
        if validate_password(new_password).is_err() {
            return Err(ErrorKind::Validation("password must be at least 8 characters".into()).into());
        }
        let token = token.trim();
        if token.len() < 32 {
            return Err(ErrorKind::Validation("invalid or expired reset token".into()).into());
        }
        let hash = agentforge_auth::password::hash_password(new_password)
            .map_err(|e| ErrorKind::Internal(anyhow::anyhow!("password hashing failed: {e}")))?;
        let token_hash = hash_reset_token(token);
        let updated = self.repo.reset_password_with_token(&token_hash, &hash).await?;
        if !updated {
            return Err(ErrorKind::Validation("invalid or expired reset token".into()).into());
        }
        Ok(())
    }

    /// Get a user by ID (tenant-scoped).
    pub async fn get(&self, scope: &TenantScope, id: UserId) -> AppResult<User> {
        self.repo.find_by_id(scope, id).await
    }

    /// Update user profile (tenant-scoped).
    pub async fn update_profile(&self, scope: &TenantScope, id: UserId, display_name: Option<&str>) -> AppResult<User> {
        self.repo.update_profile(scope, id, display_name).await
    }

    /// List users in the org (admin, paginated).
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<User>> {
        let limit = limit.clamp(1, 100);
        let offset = offset.max(0);
        self.repo.list_by_org(scope, limit, offset).await
    }

    pub async fn search_org_members(
        &self,
        scope: &TenantScope,
        query: &str,
        limit: i64,
    ) -> AppResult<Vec<OrgUserSearchResult>> {
        self.repo.search_org_members(scope, query, limit).await
    }

    fn build_auth_result(
        &self,
        user: &User,
        org_id: uuid::Uuid,
        role: &str,
        access_token: String,
        remember_me: bool,
    ) -> AppResult<LoginResult> {
        let refresh_expires_in = if remember_me { REMEMBER_ME_REFRESH_EXPIRY_SECONDS } else { REFRESH_EXPIRY_SECONDS };
        let refresh_token = self
            .jwt
            .create_token_with_expiry(user.id.as_uuid(), org_id, role, refresh_expires_in)
            .map_err(|e| ErrorKind::Internal(anyhow::anyhow!("refresh token creation failed: {e}")))?;

        Ok(LoginResult {
            user: AuthenticatedUser {
                id: user.id.as_uuid().to_string(),
                email: user.email.clone(),
                username: derive_username(user),
                org_id: Some(org_id.to_string()),
                role: Some(role.to_string()),
            },
            access_token,
            expires_in: self.jwt.expiry_seconds(),
            refresh_token,
            refresh_expires_in,
        })
    }
}

fn derive_username(user: &User) -> String {
    user.display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| user.email.split('@').next().unwrap_or("user").to_string())
}

/// Validate email format (basic check: must contain '@' and be at least 5 chars).
pub(crate) fn validate_email(email: &str) -> Result<(), &'static str> {
    if !email.contains('@') || email.len() < 5 {
        return Err("invalid email format");
    }
    Ok(())
}

/// Validate password length (must be at least 8 characters).
pub(crate) fn validate_password(password: &str) -> Result<(), &'static str> {
    if password.len() < 8 {
        return Err("password must be at least 8 characters");
    }
    Ok(())
}

fn generate_reset_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_reset_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn password_reset_email_body(reset_url: &str) -> String {
    format!(
        "Reset your Wisdoverse Forge password by opening this link:\n\n{reset_url}\n\nThis link expires in {PASSWORD_RESET_TTL_MINUTES} minutes. If you did not request a password reset, ignore this email."
    )
}

fn email_domain_for_log(email: &str) -> String {
    email.split('@').nth(1).unwrap_or("unknown").to_ascii_lowercase()
}

#[cfg(test)]
mod password_reset_tests {
    use super::*;

    #[test]
    fn reset_token_hash_is_stable_and_does_not_expose_raw_token() {
        let token = "raw-token-value";
        let hash = hash_reset_token(token);
        assert_eq!(hash.len(), 64);
        assert_ne!(hash, token);
        assert_eq!(hash, hash_reset_token(token));
    }

    #[test]
    fn reset_email_body_contains_link_and_expiry() {
        let body = password_reset_email_body("https://forge.example.com/?reset_token=abc");
        assert!(body.contains("https://forge.example.com/?reset_token=abc"));
        assert!(body.contains("60 minutes"));
    }
}
