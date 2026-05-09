//! Organization CRUD endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/organizations`      — list user's organizations
//! - `POST   /api/v1/organizations`      — create organization
//! - `GET    /api/v1/organizations/{id}` — get organization by ID
//! - `PATCH  /api/v1/organizations/{id}` — update organization

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, OrgId};

use crate::health::AppState;
use crate::repositories::organization::OrganizationRepository;
use crate::services::organization::OrganizationService;

/// Request body for creating an organization.
#[derive(Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub slug: String,
}

/// Request body for updating an organization.
#[derive(Deserialize)]
pub struct UpdateOrganizationRequest {
    pub name: String,
}

/// Build a service instance from shared state.
fn make_service(state: &AppState) -> OrganizationService {
    OrganizationService::new(OrganizationRepository::new(state.pool.clone()))
}

/// `GET /api/organizations` — list organizations the user belongs to.
async fn list_organizations(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let orgs = service.list(&auth.scope).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": orgs })))
}

/// `GET /api/organizations/{id}` — get a single organization.
async fn get_organization(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let org = service.get(&auth.scope, OrgId::from(id)).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": org })))
}

/// `POST /api/organizations` — create a new organization.
async fn create_organization(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<CreateOrganizationRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let org = service.create(&auth.scope, &req.name, &req.slug).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": org })))
}

/// `PATCH /api/organizations/{id}` — update an organization.
async fn update_organization(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateOrganizationRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let org = service.update(&auth.scope, OrgId::from(id), &req.name).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": org })))
}

/// Build organization routes sub-router.
pub fn organization_routes() -> Router<AppState> {
    Router::new()
        .route("/organizations", get(list_organizations).post(create_organization))
        .route("/organizations/{id}", get(get_organization).patch(update_organization))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_request_deserialization() {
        let req: CreateOrganizationRequest =
            serde_json::from_str(r#"{"name": "Test Org", "slug": "test-org"}"#).unwrap();
        assert_eq!(req.name, "Test Org");
        assert_eq!(req.slug, "test-org");
    }

    #[test]
    fn create_request_missing_name_fails() {
        let result = serde_json::from_str::<CreateOrganizationRequest>(r#"{"slug": "test-org"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn create_request_missing_slug_fails() {
        let result = serde_json::from_str::<CreateOrganizationRequest>(r#"{"name": "Test Org"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn update_request_deserialization() {
        let req: UpdateOrganizationRequest = serde_json::from_str(r#"{"name": "Updated Org"}"#).unwrap();
        assert_eq!(req.name, "Updated Org");
    }

    #[test]
    fn update_request_missing_name_fails() {
        let result = serde_json::from_str::<UpdateOrganizationRequest>(r#"{}"#);
        assert!(result.is_err());
    }
}
