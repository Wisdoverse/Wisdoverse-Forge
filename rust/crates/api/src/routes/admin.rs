//! Admin endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/admin/users`              — list all users (admin only)
//! - `GET    /api/v1/admin/organizations`      — list all orgs (admin only)
//! - `GET    /api/v1/admin/agents`             — list agents across all tenants
//! - `GET    /api/v1/admin/agents/:id`         — agent detail with recent events
//! - `DELETE /api/v1/admin/agents/:id`         — hard-delete a single agent
//! - `DELETE /api/v1/admin/agents`             — bulk-delete agents
//! - `POST   /api/v1/admin/impersonate`        — start impersonation
//! - `POST   /api/v1/admin/impersonate/end`    — end impersonation
//! - `GET    /api/v1/admin/impersonation-log`  — list impersonation history
//! - `GET    /api/v1/admin/stats`              — system stats

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::admin::{
    AdminAgentListInput, AdminService, admin_bulk_delete_response, admin_data_response, admin_delete_response,
};

/// Query parameters for paginated admin endpoints.
#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

/// Request body for starting impersonation.
#[derive(Deserialize)]
pub struct ImpersonateRequest {
    pub target_user_id: Uuid,
    pub reason: Option<String>,
}

/// Build a service instance from shared state.
fn make_service(state: &AppState) -> AdminService {
    AdminService::from_runtime(state.pool.clone(), state.auth_callout.clone())
}

/// `GET /api/v1/admin/users` — list all users (admin only).
async fn list_users(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let users = service.list_all_users(query.limit, query.offset).await?;
    Ok(Json(admin_data_response(users)))
}

/// `GET /api/v1/admin/organizations` — list all organizations (admin only).
async fn list_organizations(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let orgs = service.list_all_organizations(query.limit, query.offset).await?;
    Ok(Json(admin_data_response(orgs)))
}

/// `POST /api/v1/admin/impersonate` — start impersonation.
async fn start_impersonation(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<ImpersonateRequest>,
) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let log = service.start_impersonation(&auth.scope, req.target_user_id, req.reason.as_deref()).await?;
    Ok(Json(admin_data_response(log)))
}

/// `POST /api/v1/admin/impersonate/end` — end impersonation.
async fn end_impersonation(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let log = service.end_impersonation(&auth.scope).await?;
    Ok(Json(admin_data_response(log)))
}

/// `GET /api/v1/admin/impersonation-log` — list impersonation history.
async fn list_impersonation_log(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let logs = service.list_impersonation_log(&auth.scope, query.limit, query.offset).await?;
    Ok(Json(admin_data_response(logs)))
}

/// `GET /api/v1/admin/stats` — system-wide statistics.
async fn get_stats(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let stats = service.stats().await?;
    Ok(Json(admin_data_response(stats)))
}

// ============================================================================
// Admin agent listing / detail / deletion
// ============================================================================

/// Query parameters for `GET /admin/agents`. Matches the camelCase keys that
/// `AgentsTable.ts` sends: `search`, `status`, `userId`, `projectId`, `page`,
/// `limit`, `sortBy`, `sortOrder`.
///
/// `status` is accepted as a free-form string rather than `AgentStatus` so
/// that the frontend can send values like `"waiting"` / `"attention"` (present
/// in the UI enum but not yet in the Rust `agent_status` DB type) without the
/// entire request failing to deserialize.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminAgentsQuery {
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub user_id: Option<Uuid>,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_agents_limit")]
    pub limit: i64,
    #[serde(default)]
    pub sort_by: Option<String>,
    #[serde(default)]
    pub sort_order: Option<String>,
}

impl AdminAgentsQuery {
    fn as_service_input(&self) -> AdminAgentListInput<'_> {
        AdminAgentListInput {
            search: self.search.as_deref(),
            status: self.status.as_deref(),
            user_id: self.user_id,
            project_id: self.project_id,
            page: self.page,
            limit: self.limit,
            sort_by: self.sort_by.as_deref(),
            sort_order: self.sort_order.as_deref(),
        }
    }
}

fn default_page() -> i64 {
    1
}

fn default_agents_limit() -> i64 {
    25
}

/// Body shape for `DELETE /admin/agents` — a list of agent IDs to remove.
#[derive(Debug, Deserialize)]
pub struct BulkDeleteRequest {
    pub ids: Vec<Uuid>,
}

/// `GET /api/v1/admin/agents` — paginated list of agents across all tenants.
async fn list_admin_agents(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<AdminAgentsQuery>,
) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let response = service.list_agent_page(query.as_service_input()).await?;
    Ok(Json(response))
}

/// `GET /api/v1/admin/agents/:id` — agent detail including recent events.
async fn get_admin_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let response = service.get_agent_response(id).await?;
    Ok(Json(response))
}

/// `DELETE /api/v1/admin/agents/:id` — hard-delete a single agent.
async fn delete_admin_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    service.delete_agent(id).await?;
    Ok(Json(admin_delete_response()))
}

/// `DELETE /api/v1/admin/agents` — bulk delete via a JSON `{ ids: [...] }` body.
async fn bulk_delete_admin_agents(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<BulkDeleteRequest>,
) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let results = service.bulk_delete_agents_checked(&body.ids).await?;
    Ok(Json(admin_bulk_delete_response(results)))
}

/// Build admin routes sub-router.
pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(list_users))
        .route("/admin/organizations", get(list_organizations))
        .route("/admin/agents", get(list_admin_agents).delete(bulk_delete_admin_agents))
        .route("/admin/agents/{id}", get(get_admin_agent).delete(delete_admin_agent))
        .route("/admin/impersonate", post(start_impersonation))
        .route("/admin/impersonate/end", post(end_impersonation))
        .route("/admin/impersonation-log", get(list_impersonation_log))
        .route("/admin/stats", get(get_stats))
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
    fn impersonate_request_deserialization() {
        let req: ImpersonateRequest = serde_json::from_str(
            r#"{"target_user_id": "00000000-0000-0000-0000-000000000001", "reason": "Debug user issue"}"#,
        )
        .unwrap();
        assert_eq!(req.target_user_id, Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap());
        assert_eq!(req.reason.as_deref(), Some("Debug user issue"));
    }

    #[test]
    fn impersonate_request_no_reason() {
        let req: ImpersonateRequest =
            serde_json::from_str(r#"{"target_user_id": "00000000-0000-0000-0000-000000000001"}"#).unwrap();
        assert!(req.reason.is_none());
    }

    #[test]
    fn impersonate_request_missing_target_fails() {
        let result = serde_json::from_str::<ImpersonateRequest>(r#"{}"#);
        assert!(result.is_err());
    }

    #[test]
    fn admin_role_check_via_service() {
        assert!(AdminService::require_admin("owner").is_ok());
        assert!(AdminService::require_admin("admin").is_ok());
        assert!(AdminService::require_admin("member").is_err());
        assert!(AdminService::require_admin("viewer").is_err());
    }

    // ========================================================================
    // Admin agents query + filter parsing tests
    // ========================================================================

    #[test]
    fn admin_agents_query_defaults() {
        let query: AdminAgentsQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.page, 1);
        assert_eq!(query.limit, 25);
        assert!(query.search.is_none());
        assert!(query.status.is_none());
        assert!(query.user_id.is_none());
        assert!(query.project_id.is_none());
        assert!(query.sort_by.is_none());
        assert!(query.sort_order.is_none());
    }

    #[test]
    fn admin_agents_query_accepts_camel_case_keys() {
        let raw = r#"{
            "search": "foo",
            "status": "working",
            "userId": "11111111-1111-1111-1111-111111111111",
            "projectId": "22222222-2222-2222-2222-222222222222",
            "page": 3,
            "limit": 50,
            "sortBy": "lastActivity",
            "sortOrder": "asc"
        }"#;
        let query: AdminAgentsQuery = serde_json::from_str(raw).unwrap();
        assert_eq!(query.search.as_deref(), Some("foo"));
        assert_eq!(query.status.as_deref(), Some("working"));
        assert!(query.user_id.is_some());
        assert!(query.project_id.is_some());
        assert_eq!(query.page, 3);
        assert_eq!(query.limit, 50);
        assert_eq!(query.sort_by.as_deref(), Some("lastActivity"));
        assert_eq!(query.sort_order.as_deref(), Some("asc"));
    }

    #[test]
    fn bulk_delete_request_deserialization() {
        let raw = r#"{"ids": ["11111111-1111-1111-1111-111111111111",
                             "22222222-2222-2222-2222-222222222222"]}"#;
        let req: BulkDeleteRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.ids.len(), 2);
    }

    #[test]
    fn bulk_delete_request_rejects_missing_ids() {
        let result = serde_json::from_str::<BulkDeleteRequest>(r#"{}"#);
        assert!(result.is_err());
    }
}
