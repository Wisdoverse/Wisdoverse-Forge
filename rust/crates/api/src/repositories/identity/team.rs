//! Team repository — tenant-scoped database queries for teams.

use agentforge_core::{AppResult, TeamId, TenantScope};
use agentforge_db::entities::Team;
use sqlx::PgPool;

use crate::domain::resource::ResourceRepositoryPolicy;

/// Database access layer for teams.
pub struct TeamRepository {
    pool: PgPool,
}

impl TeamRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List teams for the current tenant, ordered by most recent first.
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<Team>> {
        let teams = sqlx::query_as::<_, Team>(
            r#"SELECT * FROM teams
               WHERE organization_id = $1 AND deleted_at IS NULL
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(teams)
    }

    /// Get a single team by ID (tenant-scoped).
    pub async fn find_by_id(&self, scope: &TenantScope, id: TeamId) -> AppResult<Team> {
        sqlx::query_as::<_, Team>("SELECT * FROM teams WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL")
            .bind(id.as_uuid())
            .bind(scope.org_id().as_uuid())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ResourceRepositoryPolicy::team_not_found(id))
    }

    /// Create a new team with the domain-resolved slug so migration 026's
    /// `teams.slug NOT NULL` constraint holds for every new row.
    pub async fn create(&self, scope: &TenantScope, name: &str, slug: &str) -> AppResult<Team> {
        sqlx::query_as::<_, Team>(
            r#"INSERT INTO teams (organization_id, name, slug)
               VALUES ($1, $2, $3)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(name)
        .bind(slug)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Update a team's name (tenant-scoped).
    pub async fn update(&self, scope: &TenantScope, id: TeamId, name: &str) -> AppResult<Team> {
        sqlx::query_as::<_, Team>(
            r#"UPDATE teams SET name = $3, updated_at = NOW()
               WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(name)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::team_not_found(id))
    }

    /// Soft-delete a team (set deleted_at).
    pub async fn delete(&self, scope: &TenantScope, id: TeamId) -> AppResult<()> {
        let result = sqlx::query(
            r#"UPDATE teams SET deleted_at = NOW()
               WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ResourceRepositoryPolicy::team_not_found(id));
        }
        Ok(())
    }
}
