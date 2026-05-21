//! Tile endpoints (nested under `/api/v1`).
//!
//! - `GET    /tiles`        — list user's tiles
//! - `POST   /tiles`        — create tile
//! - `PATCH  /tiles/{id}`   — update position/config
//! - `DELETE /tiles/{id}`   — remove tile
//! - `PUT    /tiles/layout` — bulk update layout

use axum::extract::{Path, State};
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::tile::{TileService, tile_data_response, tile_delete_response};

/// Request body for creating a tile.
#[derive(Deserialize)]
pub struct CreateTileRequest {
    pub tile_type: String,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub position_x: i32,
    #[serde(default)]
    pub position_y: i32,
    #[serde(default = "default_one")]
    pub width: i32,
    #[serde(default = "default_one")]
    pub height: i32,
}

fn default_one() -> i32 {
    1
}

/// Request body for updating a tile.
#[derive(Deserialize)]
pub struct UpdateTileRequest {
    pub config: Option<serde_json::Value>,
    pub position_x: Option<i32>,
    pub position_y: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

/// A single tile layout entry in a bulk update.
#[derive(Deserialize)]
pub struct TileLayoutEntry {
    pub id: Uuid,
    pub position_x: i32,
    pub position_y: i32,
    pub width: i32,
    pub height: i32,
}

/// Request body for bulk layout update.
#[derive(Deserialize)]
pub struct BulkLayoutRequest {
    pub tiles: Vec<TileLayoutEntry>,
}

/// Build a TileService from shared state.
fn make_service(state: &AppState) -> TileService {
    TileService::from_pool(state.pool.clone())
}

/// `GET /tiles` — list tiles.
async fn list_tiles(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let tiles = service.list(&auth.scope).await?;
    Ok(Json(tile_data_response(tiles)))
}

/// `POST /tiles` — create a tile.
async fn create_tile(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateTileRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let tile = service
        .create(&auth.scope, &req.tile_type, &req.config, req.position_x, req.position_y, req.width, req.height)
        .await?;
    Ok(Json(tile_data_response(tile)))
}

/// `PATCH /tiles/{id}` — update a tile.
async fn update_tile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateTileRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let tile = service
        .update(&auth.scope, id, req.config.as_ref(), req.position_x, req.position_y, req.width, req.height)
        .await?;
    Ok(Json(tile_data_response(tile)))
}

/// `DELETE /tiles/{id}` — remove a tile.
async fn delete_tile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.delete(&auth.scope, id).await?;
    Ok(Json(tile_delete_response()))
}

/// `PUT /tiles/layout` — bulk update layout.
async fn bulk_update_layout(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<BulkLayoutRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let entries: Vec<(Uuid, i32, i32, i32, i32)> =
        req.tiles.iter().map(|t| (t.id, t.position_x, t.position_y, t.width, t.height)).collect();
    let tiles = service.bulk_update_layout(&auth.scope, &entries).await?;
    Ok(Json(tile_data_response(tiles)))
}

/// Build tile routes sub-router.
pub fn tile_routes() -> Router<AppState> {
    Router::new()
        .route("/tiles", get(list_tiles).post(create_tile))
        .route("/tiles/layout", put(bulk_update_layout))
        .route("/tiles/{id}", get(update_tile).patch(update_tile).delete(delete_tile))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_tile_request_deserialization() {
        let req: CreateTileRequest =
            serde_json::from_str(r#"{"tile_type": "agent", "position_x": 0, "position_y": 0}"#).unwrap();
        assert_eq!(req.tile_type, "agent");
        assert_eq!(req.width, 1);
        assert_eq!(req.height, 1);
    }

    #[test]
    fn create_tile_request_full() {
        let req: CreateTileRequest = serde_json::from_str(
            r#"{"tile_type": "chart", "config": {"chart": "line"}, "position_x": 2, "position_y": 1, "width": 3, "height": 2}"#,
        )
        .unwrap();
        assert_eq!(req.tile_type, "chart");
        assert_eq!(req.width, 3);
        assert_eq!(req.height, 2);
    }

    #[test]
    fn bulk_layout_request_deserialization() {
        let req: BulkLayoutRequest = serde_json::from_str(
            r#"{"tiles": [{"id": "550e8400-e29b-41d4-a716-446655440000", "position_x": 0, "position_y": 0, "width": 1, "height": 1}]}"#,
        )
        .unwrap();
        assert_eq!(req.tiles.len(), 1);
        assert_eq!(req.tiles[0].width, 1);
    }

    #[test]
    fn update_tile_request_partial() {
        let req: UpdateTileRequest = serde_json::from_str(r#"{"position_x": 5}"#).unwrap();
        assert_eq!(req.position_x, Some(5));
        assert!(req.config.is_none());
        assert!(req.width.is_none());
    }
}
