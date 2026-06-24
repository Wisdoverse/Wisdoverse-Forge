//! Tile repository — database queries for the tiles table.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::Tile;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::configuration::ConfigurationRepositoryPolicy;

/// Database access layer for dashboard tiles.
pub struct TileRepository {
    pool: PgPool,
}

impl TileRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List tiles for the authenticated user.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<Tile>> {
        let tiles = sqlx::query_as::<_, Tile>(
            r#"SELECT * FROM tiles
               WHERE organization_id = $1 AND user_id = $2
               ORDER BY position_y ASC, position_x ASC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(tiles)
    }

    /// Create a new tile.
    pub async fn create(
        &self,
        scope: &TenantScope,
        tile_type: &str,
        config: &serde_json::Value,
        position_x: i32,
        position_y: i32,
        width: i32,
        height: i32,
    ) -> AppResult<Tile> {
        sqlx::query_as::<_, Tile>(
            r#"INSERT INTO tiles (organization_id, user_id, tile_type, config, position_x, position_y, width, height)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(tile_type)
        .bind(config)
        .bind(position_x)
        .bind(position_y)
        .bind(width)
        .bind(height)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Update a tile's position/config.
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: Uuid,
        config: Option<&serde_json::Value>,
        position_x: Option<i32>,
        position_y: Option<i32>,
        width: Option<i32>,
        height: Option<i32>,
    ) -> AppResult<Tile> {
        sqlx::query_as::<_, Tile>(
            r#"UPDATE tiles
               SET config = COALESCE($3, config),
                   position_x = COALESCE($4, position_x),
                   position_y = COALESCE($5, position_y),
                   width = COALESCE($6, width),
                   height = COALESCE($7, height),
                   updated_at = now()
               WHERE id = $1 AND organization_id = $2 AND user_id = $8
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(config)
        .bind(position_x)
        .bind(position_y)
        .bind(width)
        .bind(height)
        .bind(scope.user_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ConfigurationRepositoryPolicy::tile_not_found(id))
    }

    /// Delete a tile by ID.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let result = sqlx::query(
            r#"DELETE FROM tiles
               WHERE id = $1 AND organization_id = $2 AND user_id = $3"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ConfigurationRepositoryPolicy::tile_not_found(id));
        }
        Ok(())
    }

    /// Bulk update layout positions.
    pub async fn bulk_update_layout(
        &self,
        scope: &TenantScope,
        tiles: &[(Uuid, i32, i32, i32, i32)],
    ) -> AppResult<Vec<Tile>> {
        let mut updated = Vec::with_capacity(tiles.len());
        for &(id, px, py, w, h) in tiles {
            let tile = sqlx::query_as::<_, Tile>(
                r#"UPDATE tiles
                   SET position_x = $3, position_y = $4, width = $5, height = $6, updated_at = now()
                   WHERE id = $1 AND organization_id = $2 AND user_id = $7
                   RETURNING *"#,
            )
            .bind(id)
            .bind(scope.org_id().as_uuid())
            .bind(px)
            .bind(py)
            .bind(w)
            .bind(h)
            .bind(scope.user_id().as_uuid())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ConfigurationRepositoryPolicy::tile_not_found(id))?;
            updated.push(tile);
        }
        Ok(updated)
    }
}
