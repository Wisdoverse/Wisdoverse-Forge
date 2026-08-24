//! SCIM 2.0 user endpoints (User resource subset, Webhook-authenticated).
//!
//! - GET    /auth/sso/scim/Users[?startIndex=&count=]  paged user list
//! - GET    /auth/sso/scim/Users/{id}                 single user
//! - POST   /auth/sso/scim/Users                      ensure account (+ org memberships)
//! - DELETE /auth/sso/scim/Users/{id}                 strip memberships + deactivate account

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::scim::{
    SCIM_LIST_SCHEMA, ScimListResponse, ScimPagePolicy, ScimUser, scim_bad_request, scim_error, scim_not_found,
    scim_unauthorized,
};
use crate::domain::sso::{DeprovisionGuardState, SsoPolicy};
use crate::health::AppState;
use agentforge_core::UserId;

type ScimError = (StatusCode, Json<serde_json::Value>);
type ScimResult<T> = Result<T, ScimError>;

/// Webhook guard shared by all SCIM routes (same secret as provisioning).
fn guard(state: &AppState, headers: &axum::http::HeaderMap) -> Result<(), ScimError> {
    let provided = headers.get("x-forge-deprovision-token").map(|v| v.as_bytes()).unwrap_or(&[]);
    if let Err(err) = state.sso_deprovision_guard(provided) {
        let (status, body) = match SsoPolicy::deprovision_guard_state(&err) {
            DeprovisionGuardState::Unconfigured => (
                StatusCode::NOT_FOUND,
                scim_not_found("SCIM webhooks are not configured (set AUTH_SSO__DEPROVISION_TOKEN)."),
            ),
            DeprovisionGuardState::Unauthorized => (StatusCode::UNAUTHORIZED, scim_unauthorized("Unauthorized.")),
        };
        return Err((status, Json(body)));
    }
    Ok(())
}

fn internal_scim(err: impl std::fmt::Display) -> ScimError {
    let detail = format!("Internal error: {err}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(scim_error("500", &detail)))
}

/// Query params: 1-based startIndex, count clamped to 1..=100.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimUsersQuery {
    pub start_index: Option<i64>,
    pub count: Option<i64>,
}

/// Body for POST (CamelCase SCIM fields).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimCreateUserRequest {
    pub user_name: String,
    pub display_name: Option<String>,
    pub active: Option<bool>,
    #[serde(default)]
    pub groups: Vec<ScimGroupRef>,
}

#[derive(Deserialize)]
pub struct ScimGroupRef {
    pub value: String,
}

/// GET /auth/sso/scim/Users - paged list.
pub async fn scim_list_users(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(query): Query<ScimUsersQuery>,
) -> ScimResult<Json<serde_json::Value>> {
    guard(&state, &headers)?;
    let (count, start_index) = ScimPagePolicy::normalize(query.count, query.start_index);
    let service = state.user_service();
    let rows = service.scim_page(count, start_index - 1).await.map_err(internal_scim)?;
    let total = service.scim_total().await.map_err(internal_scim)?;
    let resources: Vec<ScimUser> = rows
        .into_iter()
        .map(|(id, email, display_name, created)| ScimUser::new(id, email, display_name, created))
        .collect();
    let response = ScimListResponse {
        schemas: vec![SCIM_LIST_SCHEMA.to_string()],
        total_results: total,
        start_index,
        items_per_page: count,
        resources,
    };
    let value = serde_json::to_value(response).map_err(internal_scim)?;
    Ok(Json(value))
}

/// GET /auth/sso/scim/Users/{id} - single user.
pub async fn scim_get_user(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
) -> ScimResult<Json<serde_json::Value>> {
    guard(&state, &headers)?;
    let user = state.user_service().scim_user_by_id(UserId::from(id)).await.map_err(internal_scim)?;
    let Some(user) = user else {
        return Err((StatusCode::NOT_FOUND, Json(scim_not_found("User not found."))));
    };
    let view = ScimUser::new(user.id.as_uuid(), user.email, user.display_name, user.created_at);
    let value = serde_json::to_value(view).map_err(internal_scim)?;
    Ok(Json(value))
}

/// POST /auth/sso/scim/Users - ensure account + memberships.
pub async fn scim_create_user(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ScimCreateUserRequest>,
) -> ScimResult<(StatusCode, Json<serde_json::Value>)> {
    guard(&state, &headers)?;
    if req.user_name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(scim_bad_request("userName is required."))));
    }
    let slugs: Vec<String> = req.groups.into_iter().map(|g| g.value).collect();
    let roles: Vec<String> = Vec::new();
    let user = state
        .user_service()
        .provision_user(&req.user_name, req.display_name.as_deref(), &slugs, &roles)
        .await
        .map_err(internal_scim)?;
    let view = ScimUser::new(user.id.as_uuid(), user.email, user.display_name, user.created_at);
    let value = serde_json::to_value(view).map_err(internal_scim)?;
    Ok((StatusCode::CREATED, Json(value)))
}

/// DELETE /auth/sso/scim/Users/{id} - deprovision.
pub async fn scim_delete_user(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<Uuid>,
) -> ScimResult<StatusCode> {
    guard(&state, &headers)?;
    let removed = state.user_service().scim_delete_user(UserId::from(id)).await.map_err(internal_scim)?;
    if !removed.0 {
        return Err((StatusCode::NOT_FOUND, Json(scim_not_found("User not found."))));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// SCIM sub-router.
pub fn scim_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/sso/scim/Users", axum::routing::get(scim_list_users).post(scim_create_user))
        .route("/auth/sso/scim/Users/{id}", axum::routing::get(scim_get_user).delete(scim_delete_user))
}
