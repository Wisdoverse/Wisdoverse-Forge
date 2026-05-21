//! User profile endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/users`     — list users in org (paginated)
//! - `GET    /api/v1/users/:id` — get user profile
//! - `PATCH  /api/v1/users/:id` — update own profile

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, UserId};

use crate::health::AppState;
use crate::services::user::{UpdateUserProfileInput, UserService, user_data_response, user_members_response};

/// Query parameters for the list endpoint.
#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_search_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    20
}

fn default_search_limit() -> i64 {
    20
}

/// Request body for updating user profile.
#[derive(Deserialize)]
pub struct UpdateProfileRequest {
    pub display_name: Option<String>,
}

/// Build a UserService from shared state.
fn make_service(state: &AppState) -> UserService {
    state.user_service()
}

/// `GET /api/users` — list users in the authenticated org.
async fn list_users(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let users = service.list(&auth.scope, query.limit, query.offset).await?;
    Ok(Json(user_data_response(users)))
}

/// `GET /api/users/search` — search users in the authenticated org.
async fn search_users(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<SearchQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let members = service.search_org_members(&auth.scope, &query.q, query.limit).await?;
    Ok(Json(user_members_response(members)))
}

/// `GET /api/users/:id` — get a user by ID (tenant-scoped).
async fn get_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let user = service.get(&auth.scope, UserId::from(id)).await?;
    Ok(Json(user_data_response(user)))
}

/// `PATCH /api/users/:id` — update own profile.
///
/// Users can only update their own profile. Attempting to update another user
/// returns 403 Forbidden.
async fn update_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProfileRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let user = service
        .update_own_profile(
            &auth.scope,
            UpdateUserProfileInput { target_user_id: UserId::from(id), display_name: req.display_name },
        )
        .await?;
    Ok(Json(user_data_response(user)))
}

/// Build user routes sub-router.
pub fn user_routes() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users))
        .route("/users/search", get(search_users))
        .route("/users/{id}", get(get_user).patch(update_user))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_query_defaults() {
        let query: ListQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.limit, 20);
        assert_eq!(query.offset, 0);
    }

    #[test]
    fn list_query_custom_values() {
        let query: ListQuery = serde_json::from_str(r#"{"limit": 50, "offset": 10}"#).unwrap();
        assert_eq!(query.limit, 50);
        assert_eq!(query.offset, 10);
    }

    #[test]
    fn search_query_defaults() {
        let query: SearchQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.q, "");
        assert_eq!(query.limit, 20);
    }

    #[test]
    fn update_profile_request_deserialization() {
        let req: UpdateProfileRequest = serde_json::from_str(r#"{"display_name": "New Name"}"#).unwrap();
        assert_eq!(req.display_name.as_deref(), Some("New Name"));
    }

    #[test]
    fn update_profile_request_null_display_name() {
        let req: UpdateProfileRequest = serde_json::from_str(r#"{}"#).unwrap();
        assert!(req.display_name.is_none());
    }
}
