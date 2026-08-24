//! User service — login, registration, and profile management.

use std::sync::Arc;

use agentforge_auth::JwtManager;
use agentforge_core::{AppConfig, AppResult, TenantScope, UserId};
use agentforge_db::entities::User;
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, KeyInit, Mac};
use secrecy::ExposeSecret;
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

pub(crate) use crate::domain::user::{
    AuthErrorResponseContract, auth_error_response_body, auth_error_response_contract, auth_me_response,
    auth_message_response, auth_ok_response, auth_providers_response, auth_refresh_response,
    auth_success_response_body, invalid_refresh_token_response_contract, is_unauthorized_error,
    missing_refresh_token_response_contract, password_reset_error_response_contract, user_data_response,
    user_members_response, user_preferences_response,
};
use crate::domain::user::{
    AuthRefreshCookiePolicy, GeneratedPasswordResetToken, PASSWORD_RESET_TTL_MINUTES, PasswordResetRequestEmail,
    PasswordResetToken, RefreshSessionPolicy, RefreshedAccessToken, UserAccessPolicy, UserAccountPolicy, UserEmail,
    UserListPage, UserPassword, UserPreferencesPatch, UserRepositoryPolicy, derive_username, email_domain_for_log,
    password_reset_email_body,
};
pub use crate::domain::user::{AuthenticatedUser, LoginResult};
use crate::repositories::user::{OrgUserSearchResult, UserRepository};
use crate::services::email::{EmailMessage, EmailSender};

/// Service input for a user profile update initiated by the authenticated user.
pub(crate) struct UpdateUserProfileInput {
    pub(crate) target_user_id: UserId,
    pub(crate) display_name: Option<String>,
}

#[derive(Clone)]
struct PasswordResetDelivery {
    email_sender: Arc<dyn EmailSender>,
    app_url: Option<String>,
}

/// Business logic layer for user operations.
pub struct UserService {
    repo: UserRepository,
    jwt: Arc<JwtManager>,
    password_reset_delivery: Option<PasswordResetDelivery>,
    refresh_cookie_policy: AuthRefreshCookiePolicy,
    /// Accept legacy unsalted SHA-256 password hashes at login (F004). Defaults
    /// OFF; enabled only outside production so the compat window is closed in
    /// prod (where a migration force-resets any remaining SHA-256 rows).
    allow_legacy_sha256_login: bool,
    bootstrap_admin_token: Option<secrecy::SecretString>,
    allow_unprotected_admin_bootstrap: bool,
}

impl UserService {
    pub fn new(repo: UserRepository, jwt: Arc<JwtManager>) -> Self {
        Self {
            repo,
            jwt,
            password_reset_delivery: None,
            refresh_cookie_policy: AuthRefreshCookiePolicy::new(false),
            allow_legacy_sha256_login: false,
            bootstrap_admin_token: None,
            allow_unprotected_admin_bootstrap: false,
        }
    }

    pub(crate) fn from_pool(pool: PgPool, jwt: Arc<JwtManager>) -> Self {
        Self::new(UserRepository::new(pool), jwt)
    }

    pub(crate) fn from_app_config(
        pool: PgPool,
        jwt: Arc<JwtManager>,
        email_sender: Arc<dyn EmailSender>,
        config: &AppConfig,
    ) -> Self {
        let mut service = Self::new(UserRepository::new(pool), jwt)
            .with_password_reset_delivery(email_sender, config.app_url.clone())
            .with_refresh_cookie_policy(AuthRefreshCookiePolicy::new(config.is_production()));
        service.allow_legacy_sha256_login = !config.is_production();
        service.bootstrap_admin_token = config.bootstrap_admin_token.clone();
        service.allow_unprotected_admin_bootstrap =
            config.allow_unprotected_admin_bootstrap && matches!(config.environment.as_str(), "development" | "test");
        service
    }

    pub(crate) fn with_password_reset_delivery(
        mut self,
        email_sender: Arc<dyn EmailSender>,
        app_url: Option<String>,
    ) -> Self {
        self.password_reset_delivery = Some(PasswordResetDelivery { email_sender, app_url });
        self
    }

    pub(crate) fn with_refresh_cookie_policy(mut self, policy: AuthRefreshCookiePolicy) -> Self {
        self.refresh_cookie_policy = policy;
        self
    }

    pub(crate) fn refresh_cookie_name(&self) -> &'static str {
        self.refresh_cookie_policy.cookie_name()
    }

    pub(crate) fn refresh_cookie(&self, token: &str, max_age: u64) -> String {
        self.refresh_cookie_policy.refresh_cookie(token, max_age)
    }

    pub(crate) fn clear_refresh_cookie(&self) -> String {
        self.refresh_cookie_policy.clear_cookie()
    }

    /// Authenticate with email + password and return a JWT.
    pub async fn login(&self, email: &str, password: &str, remember_me: bool) -> AppResult<LoginResult> {
        // 1. Find user by email
        let user = self.repo.find_by_email(email).await?.ok_or_else(UserAccountPolicy::invalid_credentials)?;

        // 2. Verify password
        let hash = UserAccountPolicy::require_password_hash(user.password_hash.as_deref())?;
        let verification =
            agentforge_auth::password::verify_password_compat(password, hash, self.allow_legacy_sha256_login);
        UserAccountPolicy::ensure_password_verified(verification.valid)?;

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
        let (org_id, role) =
            self.repo.find_default_org(user.id).await?.ok_or_else(UserAccountPolicy::missing_default_org_membership)?;

        // 5. Create JWT
        let token =
            self.jwt.create_token(user.id.as_uuid(), org_id, &role).map_err(UserAccountPolicy::jwt_creation_failed)?;

        // 6. Update last_login (fire-and-forget, don't fail the login)
        if let Err(err) = self.repo.update_last_login(user.id).await {
            tracing::warn!(error = ?err, user_id = %user.id.as_uuid(), "Failed to update last_login_at");
        }

        self.build_auth_result(&user, org_id, &role, token, remember_me)
    }

    /// Register a new user account.
    pub async fn register(
        &self,
        email: &str,
        password: &str,
        display_name: Option<&str>,
        setup_token: Option<&str>,
    ) -> AppResult<LoginResult> {
        let email = UserEmail::parse(email)?;
        let password = UserPassword::parse(password)?;

        let admin_bootstrap_authorized = self.allow_unprotected_admin_bootstrap
            || self
                .bootstrap_admin_token
                .as_ref()
                .is_some_and(|expected| setup_token_matches(expected.expose_secret(), setup_token));
        if !admin_bootstrap_authorized && !self.repo.has_active_platform_admin().await? {
            return Err(UserRepositoryPolicy::setup_token_required_or_invalid());
        }

        let hash = agentforge_auth::password::hash_password(password.value())
            .map_err(UserAccountPolicy::password_hashing_failed)?;

        let user =
            self.repo.create(email.value(), Some(hash.as_str()), display_name, admin_bootstrap_authorized).await?;
        let (org_id, role) =
            self.repo.find_default_org(user.id).await?.ok_or_else(UserAccountPolicy::missing_default_org_membership)?;

        let access_token =
            self.jwt.create_token(user.id.as_uuid(), org_id, &role).map_err(UserAccountPolicy::jwt_creation_failed)?;

        self.build_auth_result(&user, org_id, &role, access_token, false)
    }

    /// Find a user by email, or provision one (no password) plus their default
    /// org when they sign in through SSO for the first time.
    pub async fn ensure_sso_user(
        &self,
        email: &str,
        display_name: Option<&str>,
    ) -> AppResult<agentforge_db::entities::User> {
        let email = UserEmail::parse(email)?;
        if let Some(user) = self.repo.find_by_email(email.value()).await? {
            return Ok(user);
        }
        self.repo.create(email.value(), None, display_name, true).await
    }

    /// SCIM-style provisioning from a provider webhook: ensure the account
    /// exists and add memberships for the requested org slugs (unknown slugs
    /// are skipped). Roles may request `admin`; owners are never touched.
    pub async fn provision_user(
        &self,
        email: &str,
        display_name: Option<&str>,
        org_slugs: &[String],
        roles: &[String],
    ) -> AppResult<agentforge_db::entities::User> {
        let user = self.ensure_sso_user(email, display_name).await?;
        let is_admin = roles.iter().any(|role| role == "admin");
        for slug in org_slugs {
            let Some(org_id) = self.repo.find_org_id_by_slug(slug).await? else {
                continue;
            };
            let role = if is_admin { "admin" } else { "member" };
            if let Ok(true) = self.repo.add_membership(org_id, user.id, role).await {
                tracing::info!(user_id = %user.id.as_uuid(), org_id = %org_id, "SCIM provisioning: added membership");
            }
        }
        Ok(user)
    }

    /// Fetch the user row by id (SSO/invite flows need email without org ctx).
    pub async fn user_by_id(&self, user_id: UserId) -> AppResult<agentforge_db::entities::User> {
        self.repo.find_by_user_id(user_id).await?.ok_or_else(|| UserAccountPolicy::invalid_credentials().into())
    }

    /// SCIM: paged (id, email, display_name, created_at) listing, oldest first.
    pub async fn scim_page(
        &self,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<(Uuid, String, Option<String>, DateTime<Utc>)>> {
        self.repo.list_users_paged(limit, offset).await
    }

    /// SCIM: total active accounts (for totalResults).
    pub async fn scim_total(&self) -> AppResult<i64> {
        self.repo.count_users().await
    }

    /// SCIM: lookup by id without tenant scope (webhook-authenticated).
    pub async fn scim_user_by_id(&self, user_id: UserId) -> AppResult<Option<agentforge_db::entities::User>> {
        self.repo.find_by_user_id(user_id).await
    }

    /// SCIM delete: strip memberships then deactivate the account.
    pub async fn scim_delete_user(&self, user_id: UserId) -> AppResult<(bool, usize)> {
        let Some(user) = self.repo.find_by_user_id(user_id).await? else {
            return Ok((false, 0));
        };
        let removed = self.deprovision_user(&user.email).await?.1;
        self.repo.deactivate_user(user_id).await?;
        Ok((true, removed))
    }

    /// Map the provider's groups onto the org membership role: a member found
    /// in an admin group is upgraded to `admin`. Nothing is ever lowered or
    /// demoted, and an owner's org is never touched. Returns true when the
    /// role changed.
    pub async fn sync_sso_role(
        &self,
        org_id: uuid::Uuid,
        user_id: UserId,
        groups: &[String],
        admin_groups: &[String],
    ) -> AppResult<bool> {
        if admin_groups.is_empty() {
            return Ok(false);
        }
        let is_admin = groups.iter().any(|group| admin_groups.iter().any(|admin| admin == group));
        if !is_admin {
            return Ok(false);
        }
        let Some(role) = self.repo.find_membership_role(user_id, org_id).await? else {
            return Ok(false);
        };
        if role != "member" {
            return Ok(false);
        }
        self.repo.set_membership_role(org_id, user_id, "admin").await
    }

    /// Org provisioning from the provider groups, per `org_group_map`:
    /// `orgSlug=group1;orgSlug2=group2`. A matching group adds the user to the
    /// org (as `member`, or `admin` when also in an admin group); with
    /// `deprovision` enabled, a missing group removes the membership — never an
    /// owner, and never the user's last remaining org.
    pub async fn sync_sso_org_memberships(
        &self,
        org_map: &[(String, String)],
        groups: &[String],
        admin_groups: &[String],
        user_id: UserId,
        deprovision: bool,
    ) -> AppResult<()> {
        if org_map.is_empty() {
            return Ok(());
        }
        let is_admin = admin_groups.iter().any(|group| groups.contains(group));
        for (org_slug, org_group) in org_map {
            let Some(org_id) = self.repo.find_org_id_by_slug(org_slug).await? else {
                continue;
            };
            if groups.iter().any(|group| group == org_group) {
                let role = if is_admin { "admin" } else { "member" };
                if self.repo.add_membership(org_id, user_id, role).await? {
                    tracing::info!(user_id = %user_id.as_uuid(), org_id = %org_id, "SSO org provisioning: added membership");
                }
            } else if deprovision {
                // Safety gate: removing the user from every org would lock them
                // out of sign-in entirely.
                if self.repo.membership_count(user_id).await? <= 1 {
                    continue;
                }
                if self.repo.remove_membership_if_member(org_id, user_id).await? {
                    tracing::info!(user_id = %user_id.as_uuid(), org_id = %org_id, "SSO deprovisioning: removed membership");
                }
            }
        }
        Ok(())
    }

    /// Team provisioning from the provider groups, per `team_group_map`
    /// (`teamName=group1;teamName2=group2`). Applies inside the user's
    /// default org; a matching group adds the team membership (`member`, or
    /// `admin` when also in an admin group). With `deprovision`, a missing
    /// group removes it. Unknown team names are skipped so a rename never
    /// blocks sign-in.
    pub async fn sync_sso_team_memberships(
        &self,
        team_map: &[(String, String)],
        groups: &[String],
        admin_groups: &[String],
        user_id: UserId,
        deprovision: bool,
    ) -> AppResult<()> {
        if team_map.is_empty() {
            return Ok(());
        }
        let Ok((org_id, _)) = self.default_membership(user_id).await else {
            return Ok(());
        };
        let is_admin = admin_groups.iter().any(|group| groups.contains(group));
        for (team_name, team_group) in team_map {
            let Some(team_id) = self.repo.find_team_id_by_name(org_id, team_name).await? else {
                tracing::debug!("SSO team provisioning: no team named {team_name} in org {org_id}");
                continue;
            };
            if groups.iter().any(|group| group == team_group) {
                let role = if is_admin { "admin" } else { "member" };
                if self.repo.add_team_membership(team_id, user_id, role).await? {
                    tracing::info!(user_id = %user_id.as_uuid(), team_id = %team_id, "SSO team provisioning: added membership");
                }
            } else if deprovision && self.repo.remove_team_membership(team_id, user_id).await? {
                tracing::info!(user_id = %user_id.as_uuid(), team_id = %team_id, "SSO deprovisioning: removed team membership");
            }
        }
        Ok(())
    }

    /// Instant-off deprovisioning from a provider/IdP event: removes every
    /// non-owner membership the user has, so a revoked account stops seeing
    /// team spaces on the next request. Owners are never auto-removed.
    /// Returns (user_found, memberships_removed).
    pub async fn deprovision_user(&self, email: &str) -> AppResult<(bool, usize)> {
        let Some(user) = self.repo.find_by_email(email).await? else {
            return Ok((false, 0));
        };
        let memberships = self.repo.memberships_of(user.id).await?;
        let mut removed = 0usize;
        for (org_id, role) in memberships {
            if role == "owner" {
                continue;
            }
            if self.repo.remove_membership_if_member(org_id, user.id).await? {
                removed += 1;
            }
        }
        Ok((true, removed))
    }

    /// The user's default org membership (org id + role).
    pub async fn default_membership(&self, user_id: UserId) -> AppResult<(uuid::Uuid, String)> {
        self.repo
            .find_default_org(user_id)
            .await?
            .ok_or_else(|| UserAccountPolicy::missing_default_org_membership().into())
    }

    /// Sign in an SSO-authenticated user (same result shape as password login,
    /// without any password check).
    pub async fn sso_sign_in(&self, user_id: UserId) -> AppResult<LoginResult> {
        let user = self.repo.find_by_user_id(user_id).await?.ok_or_else(UserAccountPolicy::invalid_credentials)?;
        let (org_id, role) = self.default_membership(user.id).await?;
        let access_token =
            self.jwt.create_token(user.id.as_uuid(), org_id, &role).map_err(UserAccountPolicy::jwt_creation_failed)?;
        if let Err(err) = self.repo.update_last_login(user.id).await {
            tracing::warn!(error = ?err, user_id = %user.id.as_uuid(), "Failed to update last_login_at for SSO sign-in");
        }
        self.build_auth_result(&user, org_id, &role, access_token, false)
    }

    /// Request a password reset link. The response is intentionally generic so
    /// callers cannot enumerate users by email address.
    pub async fn request_password_reset(&self, email: &str) -> AppResult<()> {
        let delivery = self
            .password_reset_delivery
            .as_ref()
            .ok_or_else(UserAccountPolicy::password_reset_delivery_not_configured)?;
        let email_sender = delivery.email_sender.as_ref();
        if !email_sender.is_configured() {
            return Err(UserAccountPolicy::password_reset_smtp_not_configured().into());
        }
        let app_url = delivery
            .app_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(UserAccountPolicy::password_reset_app_url_required)?;

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
            .map_err(UserAccountPolicy::password_hashing_failed)?;
        let token_hash = token.hash();
        let updated = self.repo.reset_password_with_token(&token_hash, &hash).await?;
        if !updated {
            return Err(UserAccountPolicy::invalid_or_expired_reset_token().into());
        }
        Ok(())
    }

    /// Get a user by ID (tenant-scoped).
    pub async fn get(&self, scope: &TenantScope, id: UserId) -> AppResult<User> {
        self.repo.find_by_id(scope, id).await
    }

    /// Read the caller's GLOBAL platform-admin flag (`users.is_admin`).
    ///
    /// The JWT does NOT carry `is_admin`, so `/me` looks it up here — mirroring
    /// how `AdminService::require_platform_admin` reads the same column — to
    /// expose it as the `/me` `isAdmin` field. Pass the authenticated user's own
    /// id.
    pub(crate) async fn is_platform_admin(&self, user_id: UserId) -> AppResult<bool> {
        self.repo.find_is_admin_by_id(user_id).await
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
        UserAccessPolicy::ensure_self_profile(scope.user_id(), input.target_user_id)?;

        self.repo.update_profile(scope, input.target_user_id, input.display_name.as_deref()).await
    }

    /// Read the authenticated user's UI preferences document.
    ///
    /// Preferences are per-user (keyed by `scope.user_id()`), not org-scoped:
    /// they belong to the account and follow it across organizations.
    pub(crate) async fn get_preferences(&self, scope: &TenantScope) -> AppResult<serde_json::Value> {
        self.repo.get_preferences(scope.user_id()).await
    }

    /// Validate a preferences patch and shallow-merge it into the
    /// authenticated user's stored preferences, returning the merged document.
    pub(crate) async fn update_preferences(
        &self,
        scope: &TenantScope,
        body: &serde_json::Value,
    ) -> AppResult<serde_json::Value> {
        let patch = UserPreferencesPatch::parse(body)?;
        self.repo.merge_preferences(scope.user_id(), patch.as_value()).await
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

    pub(crate) async fn refresh_session(&self, refresh_token: &str) -> AppResult<RefreshedAccessToken> {
        let claims = self.jwt.verify_token(refresh_token).map_err(|_| UserAccountPolicy::invalid_refresh_token())?;
        // F004: durable session invalidation. A refresh token issued before the
        // account's session floor (password reset or operator force-reset) is
        // rejected even after the password hash is no longer the sentinel, so a
        // copied/stale refresh token inside its multi-day lifetime cannot mint
        // fresh access tokens.
        let floor = self.repo.session_floor(UserId::from(claims.sub)).await?;
        if agentforge_auth::session_token_revoked(claims.iat, floor.map(|f| f.timestamp())) {
            return Err(UserAccountPolicy::invalid_refresh_token().into());
        }
        // #889/F002: re-read the LIVE org-membership role instead of echoing the
        // token's `role` claim. A demoted admin is re-minted at their current
        // role; a user whose membership was revoked can no longer refresh.
        let live_role = self
            .repo
            .find_membership_role(UserId::from(claims.sub), claims.org)
            .await?
            .ok_or_else(UserAccountPolicy::invalid_refresh_token)?;
        let access_token = self
            .jwt
            .create_token_with_axes(
                claims.sub,
                claims.org,
                &live_role,
                claims.workspace_id,
                claims.team_id,
                claims.project_id,
            )
            .map_err(UserAccountPolicy::access_token_refresh_failed)?;

        Ok(RefreshedAccessToken::new(access_token, self.jwt.expiry_seconds()))
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
            .map_err(UserAccountPolicy::refresh_token_creation_failed)?;

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

fn setup_token_matches(expected: &str, candidate: Option<&str>) -> bool {
    let Some(candidate) = candidate else { return false };
    const CONTEXT: &[u8] = b"agentforge first administrator setup";

    let mut candidate_mac = Hmac::<Sha256>::new_from_slice(candidate.as_bytes()).expect("HMAC accepts any key length");
    candidate_mac.update(CONTEXT);
    let candidate_tag = candidate_mac.finalize().into_bytes();

    let mut expected_mac = Hmac::<Sha256>::new_from_slice(expected.as_bytes()).expect("HMAC accepts any key length");
    expected_mac.update(CONTEXT);
    expected_mac.verify_slice(&candidate_tag).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::user::UserRepository;
    use sqlx::PgPool;
    use uuid::Uuid;

    const TEST_SECRET: &str = "refresh-session-live-role-test-secret-32bytes!!";

    #[test]
    fn setup_token_comparison_requires_an_exact_match() {
        assert!(setup_token_matches("a-very-long-deployment-setup-token", Some("a-very-long-deployment-setup-token")));
        assert!(!setup_token_matches("a-very-long-deployment-setup-token", Some("wrong-token")));
        assert!(!setup_token_matches("a-very-long-deployment-setup-token", None));
    }

    #[tokio::test]
    async fn unprotected_admin_bootstrap_requires_explicit_local_mode() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/agentforge_test")
            .expect("lazy test pool");
        let jwt = Arc::new(JwtManager::new(TEST_SECRET, 3600));

        for (environment, opted_in, allowed) in [
            ("development", false, false),
            ("development", true, true),
            ("test", true, true),
            ("production", true, false),
            ("prod", true, false),
            ("staging", true, false),
            ("Production", true, false),
        ] {
            let mut config = crate::test_support::test_app_config("postgres://localhost/agentforge_test");
            config.environment = environment.to_string();
            config.allow_unprotected_admin_bootstrap = opted_in;
            let service = UserService::from_app_config(
                pool.clone(),
                jwt.clone(),
                Arc::new(crate::services::email::DisabledEmailSender),
                &config,
            );
            assert_eq!(service.allow_unprotected_admin_bootstrap, allowed, "environment={environment}");
        }
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn production_registration_requires_setup_token_only_for_the_first_admin(pool: PgPool) {
        const SETUP_TOKEN: &str = "bootstrap-token-that-is-at-least-thirty-two-characters";
        let password = Uuid::new_v4().to_string();
        let jwt = Arc::new(JwtManager::new(TEST_SECRET, 3600));
        let mut config = crate::test_support::test_app_config("postgres://localhost/agentforge_test");
        config.environment = "production".to_string();
        config.bootstrap_admin_token = Some(secrecy::SecretString::from(SETUP_TOKEN.to_string()));
        let service = UserService::from_app_config(
            pool.clone(),
            jwt,
            Arc::new(crate::services::email::DisabledEmailSender),
            &config,
        );

        let missing =
            service.register("missing@example.com", &password, None, None).await.expect_err("missing token must fail");
        let wrong = service
            .register("wrong@example.com", &password, None, Some("wrong-token"))
            .await
            .expect_err("wrong token must fail");
        for err in [missing, wrong] {
            assert!(matches!(
                err.kind,
                agentforge_core::ErrorKind::ForbiddenWithCode { code: "SETUP_TOKEN_REQUIRED_OR_INVALID", .. }
            ));
        }
        let rejected_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&pool).await.expect("count rejected users");
        assert_eq!(rejected_count, 0);

        service
            .register("admin@example.com", &password, None, Some(SETUP_TOKEN))
            .await
            .expect("correct token creates first admin");
        service
            .register("member@example.com", &password, None, None)
            .await
            .expect("later registrations do not require the setup token");
        service
            .register("replay@example.com", &password, None, Some(SETUP_TOKEN))
            .await
            .expect("token replay after bootstrap is an ordinary registration");

        let roles: Vec<(String, bool)> = sqlx::query_as("SELECT email, is_admin FROM users ORDER BY email")
            .fetch_all(&pool)
            .await
            .expect("read registered users");
        assert_eq!(
            roles,
            vec![
                ("admin@example.com".to_string(), true),
                ("member@example.com".to_string(), false),
                ("replay@example.com".to_string(), false),
            ]
        );
    }

    /// Seed an org (+ user), optionally with an `organization_members` row.
    async fn seed_member(pool: &PgPool, role: Option<&str>) -> (Uuid, Uuid) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org_id)
            .bind(format!("Org {org_id}"))
            .bind(format!("org-{org_id}"))
            .execute(pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        if let Some(role) = role {
            sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
                .bind(org_id)
                .bind(user_id)
                .bind(role)
                .execute(pool)
                .await
                .expect("seed membership");
        }
        (org_id, user_id)
    }

    fn user_service(pool: &PgPool, jwt: &Arc<JwtManager>) -> UserService {
        UserService::new(UserRepository::new(pool.clone()), jwt.clone())
    }

    /// #889/F002: a refresh token carrying a stale `role=admin` claim is
    /// re-minted at the user's LIVE membership role (here, demoted to `member`).
    #[sqlx::test(migrations = "../db/migrations")]
    async fn refresh_session_reissues_live_membership_role(pool: PgPool) {
        let jwt = Arc::new(JwtManager::new(TEST_SECRET, 3600));
        let (org_id, user_id) = seed_member(&pool, Some("member")).await;
        // Stale elevated refresh token: role=admin baked in at an earlier issuance.
        let refresh = jwt.create_token(user_id, org_id, "admin").expect("mint refresh");

        let session = user_service(&pool, &jwt).refresh_session(&refresh).await.expect("refresh ok");
        let claims = jwt.verify_token(session.access_token()).expect("decode new access");
        assert_eq!(claims.role, "member", "new access token must carry the LIVE role, not the stale admin claim");
    }

    /// A user whose org membership was revoked can no longer refresh.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn refresh_session_rejects_revoked_membership(pool: PgPool) {
        let jwt = Arc::new(JwtManager::new(TEST_SECRET, 3600));
        let (org_id, user_id) = seed_member(&pool, None).await; // no membership row
        let refresh = jwt.create_token(user_id, org_id, "admin").expect("mint refresh");

        let err = user_service(&pool, &jwt).refresh_session(&refresh).await.expect_err("revoked must fail");
        assert!(matches!(err.kind, agentforge_core::ErrorKind::Unauthorized), "got: {:?}", err.kind);
    }

    async fn set_session_floor(pool: &PgPool, user_id: Uuid, floor: chrono::DateTime<chrono::Utc>) {
        sqlx::query("UPDATE users SET sessions_invalid_before = $2 WHERE id = $1")
            .bind(user_id)
            .bind(floor)
            .execute(pool)
            .await
            .expect("set session floor");
    }

    /// F004: a refresh token issued BEFORE the account's session floor is
    /// rejected, even though the hash is a normal Argon2 hash (not the sentinel).
    /// This is the durable replacement for the transient sentinel-hash gate.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn refresh_session_rejects_token_issued_before_session_floor(pool: PgPool) {
        let jwt = Arc::new(JwtManager::new(TEST_SECRET, 3600));
        let (org_id, user_id) = seed_member(&pool, Some("member")).await;
        let refresh = jwt.create_token(user_id, org_id, "member").expect("mint refresh");
        // Floor in the future: the just-minted token's `iat` predates it.
        set_session_floor(&pool, user_id, chrono::Utc::now() + chrono::Duration::days(1)).await;

        let err =
            user_service(&pool, &jwt).refresh_session(&refresh).await.expect_err("pre-floor token must be rejected");
        assert!(matches!(err.kind, agentforge_core::ErrorKind::Unauthorized), "got: {:?}", err.kind);
    }

    /// A token issued AFTER the floor is still accepted (a normal post-reset login).
    #[sqlx::test(migrations = "../db/migrations")]
    async fn refresh_session_accepts_token_issued_after_session_floor(pool: PgPool) {
        let jwt = Arc::new(JwtManager::new(TEST_SECRET, 3600));
        let (org_id, user_id) = seed_member(&pool, Some("member")).await;
        // Floor in the past: a freshly minted token is newer and accepted.
        set_session_floor(&pool, user_id, chrono::Utc::now() - chrono::Duration::days(1)).await;
        let refresh = jwt.create_token(user_id, org_id, "member").expect("mint refresh");

        let session = user_service(&pool, &jwt).refresh_session(&refresh).await.expect("post-floor token must pass");
        assert!(!session.access_token().is_empty());
    }

    /// F004: force-reset replaces a legacy 64-hex SHA-256 hash with the sentinel
    /// AND stamps the session floor, and is idempotent.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn force_reset_legacy_sha256_resets_hash_and_stamps_floor(pool: PgPool) {
        let (_org_id, user_id) = seed_member(&pool, Some("member")).await;
        sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
            .bind(user_id)
            .bind("a".repeat(64))
            .execute(&pool)
            .await
            .expect("seed legacy sha256 hash");

        let repo = UserRepository::new(pool.clone());
        assert_eq!(repo.force_reset_legacy_sha256_hashes().await.expect("force reset"), 1);

        let (hash, floor): (String, Option<chrono::DateTime<chrono::Utc>>) =
            sqlx::query_as("SELECT password_hash, sessions_invalid_before FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .expect("read back");
        assert_eq!(hash, agentforge_auth::password::LEGACY_PASSWORD_RESET_SENTINEL);
        assert!(floor.is_some(), "force-reset must stamp the session floor");

        // Idempotent: no 64-hex rows remain after the first run.
        assert_eq!(repo.force_reset_legacy_sha256_hashes().await.expect("idempotent re-run"), 0);
    }
}
