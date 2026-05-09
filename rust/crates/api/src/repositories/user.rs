//! User repository — database queries for the users table.
//!
//! `find_by_email` is NOT tenant-scoped because login happens before org context
//! is established. Other methods enforce tenant isolation via `TenantScope`.

use agentforge_core::{AppResult, ErrorKind, TenantScope, UserId};
use agentforge_db::entities::User;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct OrgUserSearchResult {
    pub user_id: uuid::Uuid,
    pub email: String,
    pub username: String,
    pub role: String,
}

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
        .ok_or_else(|| ErrorKind::NotFound(format!("user {id}")).into())
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
    pub async fn create(&self, email: &str, password_hash: &str, display_name: Option<&str>) -> AppResult<User> {
        let mut tx = self.pool.begin().await?;

        let user = sqlx::query_as::<_, User>(
            r#"INSERT INTO users (email, password_hash, display_name)
               VALUES ($1, $2, $3)
               RETURNING *"#,
        )
        .bind(email)
        .bind(password_hash)
        .bind(display_name)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| -> agentforge_core::AppError {
            match &e {
                sqlx::Error::Database(db_err) if db_err.constraint() == Some("users_email_key") => {
                    ErrorKind::Conflict("email already registered".into()).into()
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
            return Err(ErrorKind::NotFound(format!("user {id}")).into());
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
        .ok_or_else(|| ErrorKind::NotFound(format!("user {id}")).into())
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

        sqlx::query("UPDATE users SET password_hash = $2, updated_at = NOW() WHERE id = $1 AND deleted_at IS NULL")
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

    Err(ErrorKind::Internal(anyhow::anyhow!("failed to allocate unique personal organization slug")).into())
}

#[cfg(test)]
mod tests {
    use super::email_domain;

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
