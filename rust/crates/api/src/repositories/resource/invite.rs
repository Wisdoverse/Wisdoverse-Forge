//! Team invite repository — pending invites for people without an account yet.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::TeamInvite;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::resource::ResourceRepositoryPolicy;

/// Database access layer for `team_invites`.
pub struct TeamInviteRepository {
    pool: PgPool,
}

impl TeamInviteRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a pending invite (idempotent per team + email: re-inviting a
    /// pending person refreshes the token and expiry).
    pub async fn upsert_pending(
        &self,
        scope: &TenantScope,
        team_id: Uuid,
        email: &str,
        role: &str,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> AppResult<TeamInvite> {
        sqlx::query_as::<_, TeamInvite>(
            r#"INSERT INTO team_invites (organization_id, team_id, email, role, token_hash, created_by, expires_at, accepted_at)
               SELECT $1, t.id, $3, $4, $5, $6, $7, NULL
                 FROM teams t
                WHERE t.id = $2 AND t.organization_id = $1 AND t.deleted_at IS NULL
               ON CONFLICT (team_id, email)
               DO UPDATE SET role = EXCLUDED.role, token_hash = EXCLUDED.token_hash,
                             created_by = EXCLUDED.created_by, expires_at = EXCLUDED.expires_at,
                             accepted_at = NULL
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(team_id)
        .bind(email)
        .bind(role)
        .bind(token_hash)
        .bind(scope.user_id().as_uuid())
        .bind(expires_at)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::team_not_found(team_id.into()))
    }

    /// Find a still-pending invite by token hash (expired or accepted = None).
    pub async fn find_active_by_token_hash(&self, token_hash: &str) -> AppResult<Option<TeamInvite>> {
        sqlx::query_as::<_, TeamInvite>(
            "SELECT * FROM team_invites WHERE token_hash = $1 AND accepted_at IS NULL AND expires_at > NOW()",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(agentforge_core::AppError::from)
    }

    /// Grant org + team memberships for a redeemed invite (idempotent — the
    /// redeemer may already be an org member from another flow).
    pub async fn grant_memberships(&self, org_id: Uuid, team_id: Uuid, user_id: Uuid, role: &str) -> AppResult<()> {
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member') ON CONFLICT DO NOTHING")
            .bind(org_id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(agentforge_core::AppError::from)?;
        sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, $3) ON CONFLICT (team_id, user_id) DO UPDATE SET role = EXCLUDED.role")
            .bind(team_id)
            .bind(user_id)
            .bind(role)
            .execute(&self.pool)
            .await
            .map_err(agentforge_core::AppError::from)?;
        Ok(())
    }

    /// Mark a pending invite as accepted.
    pub async fn mark_accepted(&self, invite_id: Uuid) -> AppResult<()> {
        sqlx::query("UPDATE team_invites SET accepted_at = NOW() WHERE id = $1 AND accepted_at IS NULL")
            .bind(invite_id)
            .execute(&self.pool)
            .await
            .map_err(agentforge_core::AppError::from)?;
        Ok(())
    }
}
