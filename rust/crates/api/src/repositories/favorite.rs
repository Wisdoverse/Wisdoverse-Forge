//! Favorite repository — database queries for the favorites table.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::Favorite;
use sqlx::PgPool;
use uuid::Uuid;

/// Database access layer for favorites.
pub struct FavoriteRepository {
    pool: PgPool,
}

impl FavoriteRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List favorites for the authenticated user.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<Favorite>> {
        let favs = sqlx::query_as::<_, Favorite>(
            r#"SELECT * FROM favorites
               WHERE organization_id = $1 AND user_id = $2
               ORDER BY created_at DESC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(favs)
    }

    /// Add a favorite (tenant-scoped).
    pub async fn create(&self, scope: &TenantScope, target_type: &str, target_id: Uuid) -> AppResult<Favorite> {
        sqlx::query_as::<_, Favorite>(
            r#"INSERT INTO favorites (organization_id, user_id, target_type, target_id)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (user_id, target_type, target_id) DO NOTHING
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(target_type)
        .bind(target_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::Validation("favorite already exists".into()).into())
    }

    /// Delete a favorite by ID (tenant-scoped).
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let result = sqlx::query(
            r#"DELETE FROM favorites
               WHERE id = $1 AND organization_id = $2 AND user_id = $3"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ErrorKind::NotFound(format!("favorite {id}")).into());
        }
        Ok(())
    }
}
