//! User service — login, registration, and profile management.

use std::sync::Arc;

use agentforge_auth::JwtManager;
use agentforge_core::{AppResult, ErrorKind, TenantScope, UserId};
use agentforge_db::entities::User;
use chrono::{Duration, Utc};

pub use crate::domain::user::{AuthenticatedUser, LoginResult};
use crate::domain::user::{
    GeneratedPasswordResetToken, PASSWORD_RESET_TTL_MINUTES, PasswordResetRequestEmail, PasswordResetToken,
    RefreshSessionPolicy, UserEmail, UserListPage, UserPassword, derive_username, email_domain_for_log,
    password_reset_email_body,
};
pub(crate) use crate::domain::user::{user_data_response, user_members_response};
use crate::repositories::user::{OrgUserSearchResult, UserRepository};
use crate::services::email::{EmailMessage, EmailSender};

/// Service input for a user profile update initiated by the authenticated user.
pub(crate) struct UpdateUserProfileInput {
    pub(crate) target_user_id: UserId,
    pub(crate) display_name: Option<String>,
}

/// Business logic layer for user operations.
pub struct UserService {
    repo: UserRepository,
    jwt: Arc<JwtManager>,
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
        let email = UserEmail::parse(email)?;
        let password = UserPassword::parse(password)?;

        let hash = agentforge_auth::password::hash_password(password.value())
            .map_err(|e| ErrorKind::Internal(anyhow::anyhow!("password hashing failed: {e}")))?;

        let user = self.repo.create(email.value(), &hash, display_name).await?;
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

        let Some(email) = PasswordResetRequestEmail::normalize(email) else {
            return Ok(());
        };

        let Some(user) = self.repo.find_by_email(email.value()).await? else {
            return Ok(());
        };

        let token = GeneratedPasswordResetToken::generate();
        let token_hash = token.hash();
        let expires_at = Utc::now() + Duration::minutes(PASSWORD_RESET_TTL_MINUTES);
        self.repo.store_password_reset_token(user.id, &token_hash, expires_at).await?;

        let reset_url = format!("{}/login?reset_token={}", app_url.trim_end_matches('/'), token.value());
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
        let new_password = UserPassword::parse(new_password)?;
        let token = PasswordResetToken::parse(token)?;
        let hash = agentforge_auth::password::hash_password(new_password.value())
            .map_err(|e| ErrorKind::Internal(anyhow::anyhow!("password hashing failed: {e}")))?;
        let token_hash = token.hash();
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

    /// Update the authenticated user's own profile.
    pub(crate) async fn update_own_profile(
        &self,
        scope: &TenantScope,
        input: UpdateUserProfileInput,
    ) -> AppResult<User> {
        if scope.user_id() != input.target_user_id {
            return Err(ErrorKind::Forbidden.into());
        }

        self.repo.update_profile(scope, input.target_user_id, input.display_name.as_deref()).await
    }

    /// List users in the org (admin, paginated).
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<User>> {
        let page = UserListPage::new(limit, offset);
        self.repo.list_by_org(scope, page.limit(), page.offset()).await
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
        let refresh_expires_in = RefreshSessionPolicy::refresh_expiry_seconds(remember_me);
        let refresh_token = self
            .jwt
            .create_token_with_expiry(user.id.as_uuid(), org_id, role, refresh_expires_in)
            .map_err(|e| ErrorKind::Internal(anyhow::anyhow!("refresh token creation failed: {e}")))?;

        Ok(LoginResult {
            user: AuthenticatedUser {
                id: user.id.as_uuid().to_string(),
                email: user.email.clone(),
                username: derive_username(user.display_name.as_deref(), &user.email),
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
