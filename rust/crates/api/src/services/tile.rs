//! Tile service — dashboard layout management.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::Tile;
use uuid::Uuid;

use crate::domain::configuration::{TileLayoutPolicy, TileType};
use crate::repositories::tile::TileRepository;

/// Business logic layer for tile operations.
pub struct TileService {
    repo: TileRepository,
}

impl TileService {
    pub fn new(repo: TileRepository) -> Self {
        Self { repo }
    }

    /// List tiles for the authenticated user.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<Tile>> {
        self.repo.list(scope).await
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
        let tile_type = TileType::parse(tile_type)?;
        TileLayoutPolicy::validate_dimensions(width, height)?;
        self.repo.create(scope, tile_type.value(), config, position_x, position_y, width, height).await
    }

    /// Update a tile.
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
        if let Some(width) = width {
            TileLayoutPolicy::validate_width(width)?;
        }
        if let Some(height) = height {
            TileLayoutPolicy::validate_height(height)?;
        }
        self.repo.update(scope, id, config, position_x, position_y, width, height).await
    }

    /// Delete a tile.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }

    /// Bulk update layout positions.
    pub async fn bulk_update_layout(
        &self,
        scope: &TenantScope,
        tiles: &[(Uuid, i32, i32, i32, i32)],
    ) -> AppResult<Vec<Tile>> {
        TileLayoutPolicy::validate_bulk_layout(tiles)?;
        self.repo.bulk_update_layout(scope, tiles).await
    }
}
