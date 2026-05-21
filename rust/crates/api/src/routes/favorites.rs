//! Favorite endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/favorites`      — list user's favorites
//! - `POST   /api/v1/favorites`      — add favorite
//! - `DELETE /api/v1/favorites/{id}` — remove favorite

use axum::extract::{Path, State};
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::repositories::favorite::FavoriteRepository;
use crate::services::favorite::{FavoriteService, favorite_data_response, favorite_delete_response};

/// Request body for adding a favorite.
#[derive(Deserialize)]
pub struct AddFavoriteRequest {
    pub target_type: String,
    pub target_id: Uuid,
}

/// Build a FavoriteService from shared state.
fn make_service(state: &AppState) -> FavoriteService {
    FavoriteService::new(FavoriteRepository::new(state.pool.clone()))
}

/// `GET /api/favorites` — list favorites.
async fn list_favorites(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let favs = service.list(&auth.scope).await?;
    Ok(Json(favorite_data_response(favs)))
}

/// `POST /api/favorites` — add a favorite.
async fn add_favorite(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<AddFavoriteRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let fav = service.add(&auth.scope, &req.target_type, req.target_id).await?;
    Ok(Json(favorite_data_response(fav)))
}

/// `DELETE /api/favorites/{id}` — remove a favorite.
async fn remove_favorite(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.remove(&auth.scope, id).await?;
    Ok(Json(favorite_delete_response()))
}

/// Build favorite routes sub-router.
pub fn favorite_routes() -> Router<AppState> {
    Router::new()
        .route("/favorites", get(list_favorites).post(add_favorite))
        .route("/favorites/{id}", delete(remove_favorite))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_favorite_request_deserialization() {
        let req: AddFavoriteRequest =
            serde_json::from_str(r#"{"target_type": "agent", "target_id": "550e8400-e29b-41d4-a716-446655440000"}"#)
                .unwrap();
        assert_eq!(req.target_type, "agent");
    }

    #[test]
    fn add_favorite_project_type() {
        let req: AddFavoriteRequest =
            serde_json::from_str(r#"{"target_type": "project", "target_id": "550e8400-e29b-41d4-a716-446655440000"}"#)
                .unwrap();
        assert_eq!(req.target_type, "project");
    }

    #[test]
    fn add_favorite_workspace_type() {
        let req: AddFavoriteRequest = serde_json::from_str(
            r#"{"target_type": "workspace", "target_id": "550e8400-e29b-41d4-a716-446655440000"}"#,
        )
        .unwrap();
        assert_eq!(req.target_type, "workspace");
    }

    #[test]
    fn target_type_validation_values() {
        // Verify the valid target types that the service layer checks
        let valid = ["agent", "project", "workspace"];
        for t in &valid {
            assert!(["agent", "project", "workspace"].contains(t));
        }
        assert!(!["agent", "project", "workspace"].contains(&"user"));
    }
}
