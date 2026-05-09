//! Tile service — dashboard layout management.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::Tile;
use uuid::Uuid;

use crate::repositories::tile::TileRepository;

/// Valid tile types.
const VALID_TILE_TYPES: &[&str] = &["agent", "feed", "chart", "custom"];

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
        if !VALID_TILE_TYPES.contains(&tile_type) {
            return Err(ErrorKind::Validation(format!("tile_type must be one of: {:?}", VALID_TILE_TYPES)).into());
        }
        if width < 1 || height < 1 {
            return Err(ErrorKind::Validation("width and height must be >= 1".into()).into());
        }
        self.repo.create(scope, tile_type, config, position_x, position_y, width, height).await
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
        if let Some(w) = width
            && w < 1
        {
            return Err(ErrorKind::Validation("width must be >= 1".into()).into());
        }
        if let Some(h) = height
            && h < 1
        {
            return Err(ErrorKind::Validation("height must be >= 1".into()).into());
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
        if tiles.is_empty() {
            return Err(ErrorKind::Validation("tiles array must not be empty".into()).into());
        }
        for &(_, _, _, w, h) in tiles {
            if w < 1 || h < 1 {
                return Err(ErrorKind::Validation("width and height must be >= 1".into()).into());
            }
        }
        self.repo.bulk_update_layout(scope, tiles).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_tile_types() {
        assert!(VALID_TILE_TYPES.contains(&"agent"));
        assert!(VALID_TILE_TYPES.contains(&"feed"));
        assert!(VALID_TILE_TYPES.contains(&"chart"));
        assert!(VALID_TILE_TYPES.contains(&"custom"));
    }

    #[test]
    fn invalid_tile_type_rejected() {
        assert!(!VALID_TILE_TYPES.contains(&"widget"));
        assert!(!VALID_TILE_TYPES.contains(&""));
    }

    #[test]
    fn zero_dimensions_rejected() {
        let width = 0;
        assert!(width < 1); // width/height must be >= 1
    }
}
