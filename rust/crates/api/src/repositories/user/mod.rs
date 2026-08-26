//! User aggregate — database queries for the users table and per-user LLM
//! provider configurations.
//!
//! `find_by_email` is NOT tenant-scoped because login happens before org context
//! is known. Per-user LLM configs are tenant-scoped via the user FK.

pub mod llm_config;

pub use llm_config::{UserLlmConfigRepository, UserLlmConfigSecret};

use agentforge_core::{AppError, AppResult, TenantScope, UserId};
use agentforge_db::entities::User;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};

use crate::domain::user::UserRepositoryPolicy;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct OrgUserSearchResult {
    pub user_id: uuid::Uuid,
    pub email: String,
    pub username: String,
    pub role: String,
}

/// Fixed advisory-lock key that serializes the first-user platform-admin
/// bootstrap in [`UserRepository::create`]. Any stable constant works; `881`
/// references the issue (#881) that introduced the platform-admin gate, so the
/// intent is greppable. Taken with `pg_advisory_xact_lock` inside the
/// registration transaction so it auto-releases at commit/rollback.
const BOOTSTRAP_ADVISORY_LOCK_KEY: i64 = 881;

/// Database access layer for users.
pub struct UserRepository {
    pool: PgPool,
}

impl UserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find a user by email (for login — NOT tenant-scoped, email is global).
    pub async fn find_by_email(&self, email: &str) -> AppResult<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1 AND deleted_at IS NULL")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        Ok(user)
    }

    /// Find a user by ID (tenant-scoped for API access).
    /// Find a user by ID without tenant scoping — used by the SSO exchange
    /// (the user is only identifiable by the signed code at that point, the
    /// same no-org-ctx reasoning as `find_by_email`).
    pub async fn find_by_user_id(&self, id: UserId) -> AppResult<Option<User>> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1 AND deleted_at IS NULL")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await?;
        Ok(user)
    }

    pub async fn find_by_id(&self, scope: &TenantScope, id: UserId) -> AppResult<User> {
        sqlx::query_as::<_, User>(
            r#"SELECT u.* FROM users u
               INNER JOIN organization_members om ON om.user_id = u.id
               WHERE u.id = $1 AND om.organization_id = $2 AND u.deleted_at IS NULL"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| UserRepositoryPolicy::user_not_found(id))
    }

    /// Read the caller's GLOBAL platform-admin flag (`users.is_admin`) by id.
    ///
    /// Per-user, NOT org-scoped: `is_admin` is a deployment-wide flag that
    /// follows the account across organizations, so this keys off the user id
    /// only (same pattern as `get_preferences`). Callers must pass the
    /// authenticated user's own id (`scope.user_id()`). Backs the `/me`
    /// `isAdmin` field so the frontend can gate the admin console exactly as the
    /// backend platform-admin gate does. A soft-deleted/unknown user is a 404.
    pub async fn find_is_admin_by_id(&self, user_id: UserId) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>("SELECT is_admin FROM users WHERE id = $1 AND deleted_at IS NULL")
            .bind(user_id.as_uuid())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| UserRepositoryPolicy::user_not_found(user_id))
    }

    /// Create a new user (registration).
    ///
    /// Registration is fail-closed for email-domain organizations: an unverified
    /// self-signup must not join an existing canonical org or claim a new
    /// canonical domain org. It always creates a personal organization with
    /// `email_domain = NULL`; a verified invite/admin domain-claim flow can move
    /// the user later.
    ///
    /// All steps run in one transaction so login works immediately after.
    ///
    /// Bootstrap (no-lockout): on a fresh deployment the very first authorized
    /// user is promoted to platform admin (`users.is_admin = true`). The
    /// platform-admin gate (`AdminService::require_platform_admin`, #881) now
    /// guards every cross-org `/admin/*` endpoint, and `is_admin` is only
    /// settable by an existing admin — so without this, a brand-new install
    /// would have no one able to administer it. A transaction-scoped advisory
    /// lock (see `BOOTSTRAP_ADVISORY_LOCK_KEY`) serializes the decision before
    /// insert, so only one request can claim the first administrator. Migration
    /// 072 covers deployments that pre-date this code.
    ///
    /// `password_hash` is `None` for SSO-provisioned accounts, which can only
    /// sign in through the configured identity provider.
    pub async fn create(
        &self,
        email: &str,
        password_hash: Option<&str>,
        display_name: Option<&str>,
        admin_bootstrap_authorized: bool,
    ) -> AppResult<User> {
        let mut tx = self.pool.begin().await?;

        // ponytail: registration volume is low; keep one global lock until it
        // becomes measurable, then narrow locking to the empty-admin path.
        // Serialize the no-admin decision before inserting a user. An
        // unauthorized first request must leave no account behind, while two
        // authorized requests racing may create two users but only one admin.
        sqlx::query("SELECT pg_advisory_xact_lock($1)").bind(BOOTSTRAP_ADVISORY_LOCK_KEY).execute(&mut *tx).await?;
        let admin_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE is_admin AND deleted_at IS NULL)")
                .fetch_one(&mut *tx)
                .await?;
        let promote_to_admin = !admin_exists;
        if promote_to_admin && !admin_bootstrap_authorized {
            return Err(UserRepositoryPolicy::setup_token_required_or_invalid());
        }

        let user = sqlx::query_as::<_, User>(
            r#"INSERT INTO users (email, password_hash, display_name, is_admin)
               VALUES ($1, $2, $3, $4)
               RETURNING *"#,
        )
        .bind(email)
        .bind(password_hash)
        .bind(display_name)
        .bind(promote_to_admin)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| -> AppError {
            match &e {
                sqlx::Error::Database(db_err) if db_err.constraint() == Some("users_email_key") => {
                    UserRepositoryPolicy::email_already_registered()
                }
                _ => e.into(),
            }
        })?;

        let slug_base = email
            .split('@')
            .next()
            .unwrap_or("user")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>()
            .to_lowercase();
        let slug_base = if slug_base.is_empty() { "user".to_string() } else { slug_base };
        let org_name = display_name.unwrap_or(&slug_base).to_string();
        let org_id = insert_personal_org(&mut tx, &org_name, &slug_base).await?;

        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
            .bind(org_id)
            .bind(user.id.as_uuid())
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        tracing::info!(user_id = %user.id, org_id = %org_id, "User registered with new organization");
        Ok(user)
    }

    pub(crate) async fn has_active_platform_admin(&self) -> AppResult<bool> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE is_admin AND deleted_at IS NULL)")
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    /// Domain membership backfill is intentionally disabled for unverified
    /// accounts. Keeping the method as a no-op preserves the login call site
    /// while avoiding a security bug where any self-signup for a corporate
    /// domain could join that tenant without email verification or approval.
    pub async fn ensure_domain_membership(&self, _user_id: UserId, _email: &str) -> AppResult<()> {
        Ok(())
    }

    /// Update a user's display name (tenant-scoped).
    pub async fn update_profile(&self, scope: &TenantScope, id: UserId, display_name: Option<&str>) -> AppResult<User> {
        // Verify user belongs to the org first
        let membership: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM organization_members WHERE user_id = $1 AND organization_id = $2",
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_one(&self.pool)
        .await?;

        if membership == 0 {
            return Err(UserRepositoryPolicy::user_not_found(id));
        }

        sqlx::query_as::<_, User>(
            r#"UPDATE users SET display_name = COALESCE($2, display_name), updated_at = NOW()
               WHERE id = $1 AND deleted_at IS NULL
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(display_name)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| UserRepositoryPolicy::user_not_found(id))
    }

    /// Read the user's UI preferences document.
    ///
    /// Per-user, not org-scoped: preferences belong to the account itself and
    /// follow the user across organizations. Callers must pass the
    /// authenticated user's own id (`scope.user_id()`).
    pub async fn get_preferences(&self, user_id: UserId) -> AppResult<serde_json::Value> {
        sqlx::query_scalar::<_, serde_json::Value>("SELECT preferences FROM users WHERE id = $1 AND deleted_at IS NULL")
            .bind(user_id.as_uuid())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| UserRepositoryPolicy::user_not_found(user_id))
    }

    /// Shallow-merge a validated patch into the user's preferences document
    /// and return the merged result. `||` is PostgreSQL's top-level JSONB
    /// merge, so keys absent from the patch keep their stored values.
    pub async fn merge_preferences(&self, user_id: UserId, patch: &serde_json::Value) -> AppResult<serde_json::Value> {
        sqlx::query_scalar::<_, serde_json::Value>(
            r#"UPDATE users SET preferences = preferences || $2, updated_at = NOW()
               WHERE id = $1 AND deleted_at IS NULL
               RETURNING preferences"#,
        )
        .bind(user_id.as_uuid())
        .bind(patch)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| UserRepositoryPolicy::user_not_found(user_id))
    }

    /// Find the user's default organization and role.
    ///
    /// Three-tier preference:
    ///   1. Canonical org whose `email_domain` matches the user's own email
    ///      domain (the tenant they belong to).
    ///   2. Any other canonical org (`email_domain IS NOT NULL`) — legacy
    ///      cross-company memberships preserved by migration 009.
    ///   3. Personal Space (`email_domain IS NULL`) — auto-created fallback.
    ///
    /// Ties broken by earliest `created_at`.
    ///
    /// Fixes the !529 regression where users with memberships in multiple
    /// canonical orgs (e.g., old cross-company invite + their new domain org)
    /// were assigned whichever they joined first, not their own tenant.
    pub async fn find_default_org(&self, user_id: UserId) -> AppResult<Option<(uuid::Uuid, String)>> {
        let result = sqlx::query_as::<_, (uuid::Uuid, String)>(
            r#"SELECT om.organization_id, om.role
               FROM organization_members om
               JOIN organizations o ON o.id = om.organization_id
               JOIN users u ON u.id = om.user_id
               WHERE om.user_id = $1 AND o.deleted_at IS NULL
               ORDER BY
                 (o.email_domain IS DISTINCT FROM lower(split_part(u.email, '@', 2))),
                 (o.email_domain IS NULL),
                 om.created_at ASC
               LIMIT 1"#,
        )
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;
        Ok(result)
    }

    /// Upgrade one membership row (member → admin) for the SSO role mapping.
    /// The `owner` guard mirrors the admin repo's `set_member_role`: SSO may
    /// elevate members, never demote or take over an owner's org.
    pub async fn set_membership_role(&self, org_id: uuid::Uuid, user_id: UserId, role: &str) -> AppResult<bool> {
        let result = sqlx::query(
            "UPDATE organization_members SET role = $3 WHERE organization_id = $1 AND user_id = $2 AND role <> 'owner' AND role <> $3",
        )
        .bind(org_id)
        .bind(user_id.as_uuid())
        .bind(role)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Add an org membership (SSO org provisioning). Returns `false` when the
    /// user is already a member — no role change happens here, so an existing
    /// member is never silently demoted by a re-provision.
    pub async fn add_membership(&self, org_id: uuid::Uuid, user_id: UserId, role: &str) -> AppResult<bool> {
        let result = sqlx::query(
            "INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)\n             ON CONFLICT (organization_id, user_id) DO NOTHING",
        )
        .bind(org_id)
        .bind(user_id.as_uuid())
        .bind(role)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Remove a non-owner membership (SSO deprovisioning). Owners are never
    /// removed.
    pub async fn remove_membership_if_member(&self, org_id: uuid::Uuid, user_id: UserId) -> AppResult<bool> {
        let result = sqlx::query(
            "DELETE FROM organization_members WHERE organization_id = $1 AND user_id = $2 AND role <> 'owner'",
        )
        .bind(org_id)
        .bind(user_id.as_uuid())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Number of orgs the user belongs to (SSO deprovisioning safety gate).
    pub async fn membership_count(&self, user_id: UserId) -> AppResult<i64> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM organization_members WHERE user_id = $1")
            .bind(user_id.as_uuid())
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    /// Find an org id by slug (SSO provisioning map lookup — pre-login, no user
    /// context yet).
    pub async fn find_org_id_by_slug(&self, slug: &str) -> AppResult<Option<uuid::Uuid>> {
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM organizations WHERE slug = $1 AND deleted_at IS NULL")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await
            .map_err(Into::into)
    }

    /// Find a live team id by name inside an org (SSO team mapping lookup).
    pub async fn find_team_id_by_name(&self, org_id: uuid::Uuid, name: &str) -> AppResult<Option<uuid::Uuid>> {
        sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT id FROM teams WHERE organization_id = $1 AND name = $2 AND deleted_at IS NULL LIMIT 1",
        )
        .bind(org_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Add or authoritatively sync an SSO team membership. Team owners are
    /// never overwritten. Returns whether a row was inserted or changed.
    pub async fn add_team_membership(&self, team_id: uuid::Uuid, user_id: UserId, role: &str) -> AppResult<bool> {
        let result = sqlx::query(
            r#"INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, $3)
               ON CONFLICT (team_id, user_id) DO UPDATE SET role = EXCLUDED.role
               WHERE team_members.role <> 'owner' AND team_members.role <> EXCLUDED.role"#,
        )
        .bind(team_id)
        .bind(user_id.as_uuid())
        .bind(role)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Remove a team membership (SSO deprovisioning).
    pub async fn remove_team_membership(&self, team_id: uuid::Uuid, user_id: UserId) -> AppResult<bool> {
        let result = sqlx::query("DELETE FROM team_members WHERE team_id = $1 AND user_id = $2 AND role <> 'owner'")
            .bind(team_id)
            .bind(user_id.as_uuid())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn find_membership_role(&self, user_id: UserId, org_id: uuid::Uuid) -> AppResult<Option<String>> {
        sqlx::query_scalar::<_, String>(
            r#"SELECT om.role
               FROM organization_members om
               JOIN organizations o ON o.id = om.organization_id
              WHERE om.organization_id = $1
                AND om.user_id = $2
                AND o.deleted_at IS NULL
              LIMIT 1"#,
        )
        .bind(org_id)
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Paged user listing for SCIM (id, email, display_name, created_at).
    pub async fn list_users_paged(
        &self,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<(uuid::Uuid, String, Option<String>, DateTime<Utc>)>> {
        let rows: Vec<(uuid::Uuid, String, Option<String>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, email, display_name, created_at FROM users WHERE deleted_at IS NULL ORDER BY created_at ASC, email ASC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// SCIM delete: deactivate an account (soft-delete, idempotent).
    pub async fn deactivate_user(&self, user_id: UserId) -> AppResult<()> {
        sqlx::query("UPDATE users SET deleted_at = NOW(), updated_at = NOW() WHERE id = $1 AND deleted_at IS NULL")
            .bind(user_id.as_uuid())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Total registered accounts (SCIM totalResults).
    pub async fn count_users(&self) -> AppResult<i64> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE deleted_at IS NULL")
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    /// All (org, role) memberships for a user (instant-off deprovisioning sweep).
    pub async fn memberships_of(&self, user_id: UserId) -> AppResult<Vec<(uuid::Uuid, String)>> {
        let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
            "SELECT organization_id, role FROM organization_members WHERE user_id = $1 ORDER BY created_at ASC",
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// The active user's session-invalidation floor (F004): refresh/switch
    /// tokens whose `iat` predates this instant must be rejected. The outer
    /// `Option` distinguishes an active user from a missing/deactivated one; the
    /// inner `Option` is NULL when the account was never invalidated. Set by a password reset
    /// and by the operator-gated legacy SHA-256 force-reset. Unlike a sentinel
    /// hash check, this stays in force after the user resets their password, so a
    /// copied refresh token inside its multi-day lifetime cannot revive a session.
    pub async fn active_session_floor(&self, user_id: UserId) -> AppResult<Option<Option<DateTime<Utc>>>> {
        sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            r#"SELECT sessions_invalid_before FROM users WHERE id = $1 AND deleted_at IS NULL"#,
        )
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn invalidate_sessions(&self, user_id: UserId) -> AppResult<()> {
        sqlx::query(
            "UPDATE users SET sessions_invalid_before = NOW(), updated_at = NOW() WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(user_id.as_uuid())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Force-reset every remaining legacy unsalted SHA-256 hash (F004): replace
    /// it with the reset sentinel (so login fails and no brute-forceable digest
    /// is left at rest) AND stamp `sessions_invalid_before = NOW()` so any live
    /// session for that account is invalidated. Returns the number of rows reset.
    ///
    /// Idempotent: after the first run no 64-hex hashes remain, so re-running
    /// affects zero rows. The caller MUST gate this on a configured reset path —
    /// see the server startup routine.
    pub async fn force_reset_legacy_sha256_hashes(&self) -> AppResult<u64> {
        let result = sqlx::query(
            r#"UPDATE users
                  SET password_hash = $1,
                      sessions_invalid_before = NOW(),
                      updated_at = NOW()
                WHERE password_hash ~ '^[0-9a-fA-F]{64}$'"#,
        )
        .bind(agentforge_auth::password::LEGACY_PASSWORD_RESET_SENTINEL)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn workspace_exists_in_org(&self, org_id: uuid::Uuid, workspace_id: uuid::Uuid) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1 FROM workspaces
                    WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL
               )"#,
        )
        .bind(workspace_id)
        .bind(org_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn user_can_read_team(
        &self,
        user_id: UserId,
        org_id: uuid::Uuid,
        team_id: uuid::Uuid,
    ) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>(
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
        .bind(user_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn user_can_read_project(
        &self,
        user_id: UserId,
        org_id: uuid::Uuid,
        project_id: uuid::Uuid,
        workspace_id: uuid::Uuid,
    ) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>(
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
        .bind(user_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Update the user's last_login_at timestamp.
    pub async fn update_last_login(&self, id: UserId) -> AppResult<()> {
        sqlx::query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Upgrade a legacy password hash to the current Argon2 format.
    pub async fn update_password_hash(&self, id: UserId, password_hash: &str) -> AppResult<()> {
        sqlx::query("UPDATE users SET password_hash = $2, updated_at = NOW() WHERE id = $1")
            .bind(id.as_uuid())
            .bind(password_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Store a one-time password reset token hash. Existing active tokens for
    /// the user are invalidated so only the newest email link can be used.
    pub async fn store_password_reset_token(
        &self,
        user_id: UserId,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"UPDATE password_reset_tokens
                  SET used_at = NOW()
                WHERE user_id = $1
                  AND used_at IS NULL"#,
        )
        .bind(user_id.as_uuid())
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"INSERT INTO password_reset_tokens (user_id, token_hash, expires_at)
               VALUES ($1, $2, $3)"#,
        )
        .bind(user_id.as_uuid())
        .bind(token_hash)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Remove a token hash after a downstream mail-send failure. This prevents
    /// undelivered reset links from lingering as valid credentials.
    pub async fn delete_password_reset_token(&self, token_hash: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM password_reset_tokens WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Consume a reset token and update the user's password atomically.
    /// Returns `false` for unknown, expired, or already-used tokens.
    pub async fn reset_password_with_token(&self, token_hash: &str, password_hash: &str) -> AppResult<bool> {
        let mut tx = self.pool.begin().await?;
        let user_id = sqlx::query_scalar::<_, uuid::Uuid>(
            r#"UPDATE password_reset_tokens
                  SET used_at = NOW()
                WHERE token_hash = $1
                  AND used_at IS NULL
                  AND expires_at > NOW()
              RETURNING user_id"#,
        )
        .bind(token_hash)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(user_id) = user_id else {
            tx.rollback().await?;
            return Ok(false);
        };

        // F004: a successful reset stamps the session floor so the long-lived
        // token-minting paths (`/auth/refresh`, `/auth/switch-context`) reject
        // every refresh/switch token issued at or before this instant — closing
        // the window where a copied pre-reset refresh token could revive the
        // session after the hash is no longer the sentinel.
        //
        // ponytail: pre-reset *access* tokens are NOT checked per-request (the
        // AuthUser extractor stays stateless — no DB read on the hot path), so
        // they remain usable until they expire on their own. That residual is
        // bounded by the short access TTL (`jwt_expiry_seconds`, 900s by
        // default), the standard stateless-JWT trade-off. Upgrade path if instant
        // access revocation is ever required: have the auth middleware consult
        // `session_floor` per request (one indexed PK read per call).
        sqlx::query(
            "UPDATE users SET password_hash = $2, sessions_invalid_before = NOW(), updated_at = NOW() \
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(user_id)
        .bind(password_hash)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"UPDATE password_reset_tokens
                  SET used_at = NOW()
                WHERE user_id = $1
                  AND used_at IS NULL"#,
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(true)
    }

    /// List users in the current org (admin).
    pub async fn list_by_org(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<User>> {
        let users = sqlx::query_as::<_, User>(
            r#"SELECT u.* FROM users u
               INNER JOIN organization_members om ON om.user_id = u.id
               WHERE om.organization_id = $1 AND u.deleted_at IS NULL
               ORDER BY u.created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(users)
    }

    pub async fn search_org_members(
        &self,
        scope: &TenantScope,
        query: &str,
        limit: i64,
    ) -> AppResult<Vec<OrgUserSearchResult>> {
        let pattern = format!("%{}%", query.trim().to_ascii_lowercase());
        let users = sqlx::query_as::<_, OrgUserSearchResult>(
            r#"SELECT
                   u.id AS user_id,
                   u.email,
                   COALESCE(NULLIF(u.display_name, ''), split_part(u.email, '@', 1)) AS username,
                   om.role
               FROM users u
               JOIN organization_members om
                 ON om.user_id = u.id
              WHERE om.organization_id = $1
                AND u.deleted_at IS NULL
                AND (
                     $2 = '%%'
                     OR lower(u.email) LIKE $2
                     OR lower(COALESCE(u.display_name, '')) LIKE $2
                )
              ORDER BY u.created_at DESC
              LIMIT $3"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(pattern)
        .bind(limit.clamp(1, 50))
        .fetch_all(&self.pool)
        .await?;
        Ok(users)
    }
}

#[cfg(test)]
/// Lowercase domain part of an email, or `None` if the address has no `@`.
pub(crate) fn email_domain(email: &str) -> Option<String> {
    let domain = email.split('@').nth(1)?.trim().to_ascii_lowercase();
    if domain.is_empty() { None } else { Some(domain) }
}

async fn insert_personal_org(
    tx: &mut Transaction<'_, Postgres>,
    org_name: &str,
    slug_base: &str,
) -> AppResult<uuid::Uuid> {
    for attempt in 0..8 {
        let slug = if attempt == 0 {
            slug_base.to_string()
        } else {
            format!("{slug_base}-{}", &uuid::Uuid::new_v4().to_string()[..8])
        };

        let org_id: Option<uuid::Uuid> = sqlx::query_scalar(
            r#"INSERT INTO organizations (name, slug, email_domain)
               VALUES ($1, $2, NULL)
               ON CONFLICT (slug) DO NOTHING
               RETURNING id"#,
        )
        .bind(org_name)
        .bind(&slug)
        .fetch_optional(&mut **tx)
        .await?;

        if let Some(org_id) = org_id {
            return Ok(org_id);
        }
    }

    Err(UserRepositoryPolicy::personal_org_slug_allocation_failed())
}

#[cfg(test)]
mod tests {
    use super::email_domain;
    use super::*;
    use serde_json::json;

    /// Seed a bare user row. Preferences tests are user-scoped, so no org or
    /// membership is required.
    async fn seed_user(pool: &sqlx::PgPool) -> UserId {
        let user_uuid = uuid::Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_uuid)
            .bind(format!("u-{user_uuid}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        UserId::from(user_uuid)
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn preferences_default_to_empty_object(pool: sqlx::PgPool) {
        let repo = UserRepository::new(pool.clone());
        let user_id = seed_user(&pool).await;

        let preferences = repo.get_preferences(user_id).await.expect("get preferences");
        assert_eq!(preferences, json!({}));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn merge_preferences_is_shallow_and_preserves_other_keys(pool: sqlx::PgPool) {
        let repo = UserRepository::new(pool.clone());
        let user_id = seed_user(&pool).await;

        let first = repo.merge_preferences(user_id, &json!({ "defaultCliTool": "codex" })).await.expect("first patch");
        assert_eq!(first, json!({ "defaultCliTool": "codex" }));

        // A second patch must keep the first patch's keys (shallow JSONB merge).
        let second =
            repo.merge_preferences(user_id, &json!({ "gettingStartedDismissed": true })).await.expect("second patch");
        assert_eq!(second, json!({ "defaultCliTool": "codex", "gettingStartedDismissed": true }));

        // Re-patching an existing key overwrites only that key.
        let third =
            repo.merge_preferences(user_id, &json!({ "gettingStartedDismissed": false })).await.expect("third patch");
        assert_eq!(third, json!({ "defaultCliTool": "codex", "gettingStartedDismissed": false }));

        let stored = repo.get_preferences(user_id).await.expect("get preferences");
        assert_eq!(stored, third);
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn preferences_reject_unknown_user(pool: sqlx::PgPool) {
        let repo = UserRepository::new(pool);
        let missing = UserId::new();

        let get_err = repo.get_preferences(missing).await.expect_err("get must fail");
        assert!(matches!(get_err.kind, agentforge_core::ErrorKind::NotFound(_)));

        let merge_err = repo
            .merge_preferences(missing, &json!({ "gettingStartedDismissed": true }))
            .await
            .expect_err("merge must fail");
        assert!(matches!(merge_err.kind, agentforge_core::ErrorKind::NotFound(_)));
    }

    /// First-user bootstrap (#881): the very first registered account is
    /// promoted to platform admin, but the second is not. This keeps a fresh
    /// deployment from locking itself out of the platform-admin-gated `/admin/*`
    /// surface while never minting extra admins.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn create_promotes_only_the_first_user_to_platform_admin(pool: sqlx::PgPool) {
        let repo = UserRepository::new(pool.clone());

        let first =
            repo.create("first@example.com", Some("hash"), Some("First"), true).await.expect("create first user");
        assert!(first.is_admin, "the first registered user becomes the platform admin");

        let second =
            repo.create("second@example.com", Some("hash"), Some("Second"), false).await.expect("create second user");
        assert!(!second.is_admin, "the second registered user is NOT promoted");

        // The returned entities must match the persisted rows (no second SELECT).
        let stored_admin: bool = sqlx::query_scalar("SELECT is_admin FROM users WHERE email = $1")
            .bind("first@example.com")
            .fetch_one(&pool)
            .await
            .expect("read first user is_admin");
        assert!(stored_admin, "first user's promotion is persisted");
        let stored_member: bool = sqlx::query_scalar("SELECT is_admin FROM users WHERE email = $1")
            .bind("second@example.com")
            .fetch_one(&pool)
            .await
            .expect("read second user is_admin");
        assert!(!stored_member, "second user stays a non-admin");

        // Exactly one platform admin exists deployment-wide.
        let admin_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin AND deleted_at IS NULL")
            .fetch_one(&pool)
            .await
            .expect("count admins");
        assert_eq!(admin_count, 1, "exactly one platform admin after two registrations");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn create_rejects_an_untrusted_first_admin_without_persisting_a_user(pool: sqlx::PgPool) {
        let repo = UserRepository::new(pool.clone());

        let err = repo
            .create("attacker@example.com", Some("hash"), Some("Attacker"), false)
            .await
            .expect_err("an untrusted request must not claim the first admin account");
        assert!(matches!(
            err.kind,
            agentforge_core::ErrorKind::ForbiddenWithCode { code: "SETUP_TOKEN_REQUIRED_OR_INVALID", .. }
        ));

        let user_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&pool).await.expect("count users");
        assert_eq!(user_count, 0, "rejected bootstrap must roll back the user insert");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn concurrent_authorized_registrations_create_exactly_one_admin(pool: sqlx::PgPool) {
        let repo = UserRepository::new(pool.clone());

        let (left, right) = tokio::join!(
            repo.create("left@example.com", Some("hash"), Some("Left"), true),
            repo.create("right@example.com", Some("hash"), Some("Right"), true),
        );
        let left = left.expect("create left user");
        let right = right.expect("create right user");
        assert_ne!(left.is_admin, right.is_admin, "only one racing request may claim platform admin");

        let admin_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin AND deleted_at IS NULL")
            .fetch_one(&pool)
            .await
            .expect("count admins");
        assert_eq!(admin_count, 1);
    }

    /// `find_is_admin_by_id` reads the global flag for the `/me` `isAdmin` field
    /// and 404s for unknown/soft-deleted users.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn find_is_admin_by_id_reads_global_flag(pool: sqlx::PgPool) {
        let repo = UserRepository::new(pool.clone());

        let admin_id = uuid::Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, email, is_admin) VALUES ($1, $2, true)")
            .bind(admin_id)
            .bind(format!("admin-{admin_id}@example.com"))
            .execute(&pool)
            .await
            .expect("seed admin");
        let member_id = uuid::Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, email, is_admin) VALUES ($1, $2, false)")
            .bind(member_id)
            .bind(format!("member-{member_id}@example.com"))
            .execute(&pool)
            .await
            .expect("seed member");

        assert!(repo.find_is_admin_by_id(UserId::from(admin_id)).await.expect("admin lookup"));
        assert!(!repo.find_is_admin_by_id(UserId::from(member_id)).await.expect("member lookup"));

        let missing = repo.find_is_admin_by_id(UserId::new()).await.expect_err("unknown user is 404");
        assert!(matches!(missing.kind, agentforge_core::ErrorKind::NotFound(_)));
    }

    #[test]
    fn extracts_lowercase_domain() {
        assert_eq!(email_domain("Xiang.Chen@EXAMPLE.COM").as_deref(), Some("example.com"));
    }

    #[test]
    fn missing_at_returns_none() {
        assert!(email_domain("not-an-email").is_none());
    }

    #[test]
    fn empty_domain_returns_none() {
        assert!(email_domain("user@").is_none());
    }
}
