//! User profile endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/users`                — list users in org (paginated)
//! - `GET    /api/v1/users/me/preferences` — read own UI preferences
//! - `PATCH  /api/v1/users/me/preferences` — shallow-merge own UI preferences
//! - `GET    /api/v1/users/:id`            — get user profile
//! - `PATCH  /api/v1/users/:id`            — update own profile

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, UserId};

use crate::health::AppState;
use crate::services::user::{
    UpdateUserProfileInput, UserService, user_data_response, user_members_response, user_preferences_response,
};

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

/// `GET /api/v1/users/me/preferences` — read the authenticated user's UI
/// preferences document.
async fn get_my_preferences(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let preferences = service.get_preferences(&auth.scope).await?;
    Ok(Json(user_preferences_response(&preferences)))
}

/// `PATCH /api/v1/users/me/preferences` — validate and shallow-merge a
/// preferences patch for the authenticated user, returning the merged document.
async fn update_my_preferences(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let preferences = service.update_preferences(&auth.scope, &body).await?;
    Ok(Json(user_preferences_response(&preferences)))
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
        .route("/users/me/preferences", get(get_my_preferences).patch(update_my_preferences))
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

    #[sqlx::test(migrations = "../db/migrations")]
    async fn preferences_routes_roundtrip_behind_auth(pool: sqlx::PgPool) {
        use axum::body::{Body, to_bytes};
        use axum::http::{Request, StatusCode, header};
        use tower::ServiceExt;

        let seed = crate::test_support::seed_provider_agent(&pool, "openai", "gpt-5.5").await;
        let app = crate::test_support::test_app_with_mock_provider(pool, "openai", "ok").await;

        // Unauthenticated requests are rejected by the auth middleware.
        let anonymous =
            Request::builder().method("GET").uri("/api/v1/users/me/preferences").body(Body::empty()).unwrap();
        let response = app.clone().oneshot(anonymous).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // Fresh accounts start with an empty preferences document.
        let get = Request::builder()
            .method("GET")
            .uri("/api/v1/users/me/preferences")
            .header(header::AUTHORIZATION, format!("Bearer {}", seed.jwt))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(get).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["preferences"], serde_json::json!({}));

        // PATCH validates through the domain type and returns the merged doc.
        let patch = Request::builder()
            .method("PATCH")
            .uri("/api/v1/users/me/preferences")
            .header(header::AUTHORIZATION, format!("Bearer {}", seed.jwt))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"gettingStartedDismissed":true}"#))
            .unwrap();
        let response = app.clone().oneshot(patch).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["preferences"]["gettingStartedDismissed"], true);

        // Unknown keys are rejected with a validation error.
        let invalid = Request::builder()
            .method("PATCH")
            .uri("/api/v1/users/me/preferences")
            .header(header::AUTHORIZATION, format!("Bearer {}", seed.jwt))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"theme":"dark"}"#))
            .unwrap();
        let response = app.clone().oneshot(invalid).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // The merged value persists for subsequent reads.
        let get_again = Request::builder()
            .method("GET")
            .uri("/api/v1/users/me/preferences")
            .header(header::AUTHORIZATION, format!("Bearer {}", seed.jwt))
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(get_again).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["preferences"], serde_json::json!({ "gettingStartedDismissed": true }));
    }
}
