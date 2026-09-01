//! Admin endpoints (nested under `/api/v1`).
//!
//! - `GET    /api/v1/admin/users`              — paginated user list (`{ ok, users, total, page, limit, totalPages }`)
//! - `PUT    /api/v1/admin/users/:id`          — change a user's global access level (`{ ok, user }`)
//! - `DELETE /api/v1/admin/users/:id`          — remove (soft-delete) a user account
//! - `GET    /api/v1/admin/organizations`      — org list with member/team counts (`{ ok, organizations, total }`)
//! - `GET    /api/v1/admin/agents`             — list agents across all tenants
//! - `GET    /api/v1/admin/agents/:id`         — agent detail with recent events
//! - `DELETE /api/v1/admin/agents/:id`         — hard-delete a single agent
//! - `DELETE /api/v1/admin/agents`             — bulk-delete agents
//! - `POST   /api/v1/admin/impersonate`        — start impersonation
//! - `POST   /api/v1/admin/impersonate/end`    — end impersonation
//! - `GET    /api/v1/admin/impersonation-log`  — list impersonation history
//! - `GET    /api/v1/admin/stats`              — system stats
//! - `GET    /api/v1/admin/dead-events`        — platform-admin-only cross-org dead-letter list (`{ ok, data }`)
//! - `GET    /api/v1/admin/cli-images`         — CLI agent-image updater status
//! - `POST   /api/v1/admin/cli-images/:tool/roll` — roll running agents of a tool
//! - `POST   /api/v1/admin/cli-images/:tool/build` — build the claude image locally

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AgentId, AppResult};

use crate::domain::admin::{AdminBulkDeletePolicy, BulkDeleteResult};
use crate::health::AppState;
use crate::services::admin::{
    AdminAgentListInput, AdminService, admin_agent_detail_response, admin_agent_list_response,
    admin_bulk_delete_response, admin_data_response, admin_delete_response, admin_org_list_response,
    admin_user_list_response, admin_user_role_response,
};
use crate::services::cli_image::cli_image_status_response;
use crate::services::cli_image_build::{LocalBuildToolPolicy, cli_image_build_response};
use crate::services::cli_image_roll::{RollToolPolicy, cli_image_roll_response};

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
    state.admin_service()
}

/// Query parameters for `GET /admin/users`: 1-based `page`, `limit`
/// (clamped to 1..=100 by the service), and an optional email/display-name
/// `search` term.
#[derive(Debug, Deserialize)]
pub struct AdminUsersQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_users_limit")]
    pub limit: i64,
    #[serde(default)]
    pub search: Option<String>,
}

fn default_users_limit() -> i64 {
    25
}

/// `GET /api/v1/admin/users` — paginated, cross-org user list.
///
/// PLATFORM-ADMIN-ONLY: gated on the server-side `users.is_admin` column, NOT
/// the per-org JWT membership role. The list spans every organization, so a
/// self-registered user who is `owner` of their personal org must not reach it
/// (closes #881). All sibling cross-org admin endpoints use the same gate.
async fn list_users(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<AdminUsersQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    let page = service.list_user_page(query.page, query.limit, query.search.as_deref()).await?;
    Ok(Json(admin_user_list_response(page)))
}

/// Body for `PUT /admin/users/:id` — the new access level
/// (`"admin" | "member"`).
#[derive(Debug, Deserialize)]
pub struct UpdateUserRoleRequest {
    pub role: String,
}

/// `PUT /api/v1/admin/users/:id` — change a user's global access level.
/// Answers `{ ok, user }` with the updated user projection.
///
/// PLATFORM-ADMIN-ONLY (`users.is_admin`). This sets the global `is_admin`
/// flag, so gating it on the self-assignable per-org role would be a direct
/// privilege-escalation path (any registered user is `owner` of their own org);
/// see #881. The per-handler owner/last-admin/self guards are unchanged.
async fn update_admin_user_role(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateUserRoleRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    let user = service.update_user_role(&auth.scope, id, &body.role).await?;
    Ok(Json(admin_user_role_response(user)))
}

/// `DELETE /api/v1/admin/users/:id` — remove (soft-delete) a user account
/// (admin only).
async fn delete_admin_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    service.delete_user(&auth.scope, id).await?;
    Ok(Json(admin_delete_response()))
}

/// `GET /api/v1/admin/organizations` — list all organizations with member and
/// team counts (admin only).
async fn list_organizations(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    let (organizations, total) = service.list_org_page(query.limit, query.offset).await?;
    Ok(Json(admin_org_list_response(organizations, total)))
}

/// `POST /api/v1/admin/impersonate` — start impersonation.
///
/// PLATFORM-ADMIN-ONLY (`users.is_admin`): impersonation mints a session as any
/// target user across any org, so it must never be reachable via the per-org
/// role (#881).
async fn start_impersonation(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<ImpersonateRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    let log = service.start_impersonation(&auth.scope, req.target_user_id, req.reason.as_deref()).await?;
    Ok(Json(admin_data_response(log)))
}

/// `POST /api/v1/admin/impersonate/end` — end impersonation.
async fn end_impersonation(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    let log = service.end_impersonation(&auth.scope).await?;
    Ok(Json(admin_data_response(log)))
}

/// `GET /api/v1/admin/impersonation-log` — list impersonation history.
async fn list_impersonation_log(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    let logs = service.list_impersonation_log(&auth.scope, query.limit, query.offset).await?;
    Ok(Json(admin_data_response(logs)))
}

/// `GET /api/v1/admin/stats` — system-wide statistics.
async fn get_stats(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    let stats = service.stats().await?;
    Ok(Json(admin_data_response(stats)))
}

/// `GET /api/v1/admin/control-plane` — org-scoped orchestration control-plane
/// snapshot — the "is a loop wedged" signals the metrics worker emits as
/// Prometheus gauges, readable without a Prometheus stack. Scoped to the
/// caller's org. `job_queue` depth is platform-global (no org column) and
/// intentionally not included.
///
/// ORG-SCOPED on purpose: this keeps the per-org `require_admin` gate (NOT the
/// platform-admin gate the cross-org endpoints use). The data never leaves the
/// caller's own org, so a legitimate org admin/owner must be able to read their
/// own org's health — flipping it to platform-admin would wrongly 403 them.
async fn get_control_plane(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let stale_after_secs = agentforge_jobs::PARTICIPANT_DEFAULT_STALE_AFTER.as_secs() as i64;
    let snapshot = service.org_control_plane_snapshot(&auth.scope, stale_after_secs).await?;
    Ok(Json(admin_data_response(snapshot)))
}

// ============================================================================
// Admin agent listing / detail / deletion
// ============================================================================

/// Query parameters for `GET /admin/agents`. Matches the camelCase keys that
/// the admin agents view sends: `search`, `status`, `runtimeKind`, `userId`,
/// `projectId`, `page`, `limit`, `sortBy`, `sortOrder`.
///
/// `status` is accepted as a free-form string rather than `AgentStatus` so
/// that the frontend can send values like `"waiting"` / `"attention"` (present
/// in the UI enum but not yet in the Rust `agent_status` DB type) without the
/// entire request failing to deserialize.
///
/// `runtimeKind` is likewise accepted as a free-form string here; the service
/// strictly validates it against `RuntimeKind` and returns HTTP 422 for an
/// unknown value (canonical slugs: `container | cli | api`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminAgentsQuery {
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub runtime_kind: Option<String>,
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
            runtime_kind: self.runtime_kind.as_deref(),
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
    let service = make_service(&state);
    service.require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    let page = service.list_agent_page(query.as_service_input()).await?;
    Ok(Json(admin_agent_list_response(page)))
}

/// `GET /api/v1/admin/agents/:id` — agent detail including recent events.
async fn get_admin_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    let detail = service.get_agent_detail(id).await?;
    Ok(Json(admin_agent_detail_response(detail)))
}

/// `DELETE /api/v1/admin/agents/:id` — hard-delete a single agent.
async fn delete_admin_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let authority = service.require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    state.agent_container_control_service().delete_as_platform_admin(&authority, AgentId::from(id)).await?;
    Ok(Json(admin_delete_response()))
}

/// `DELETE /api/v1/admin/agents` — bulk delete via a JSON `{ ids: [...] }` body.
async fn bulk_delete_admin_agents(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<BulkDeleteRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    let authority = service.require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    AdminBulkDeletePolicy::require_ids(&body.ids)?;
    let control = state.agent_container_control_service();
    let mut results = Vec::with_capacity(body.ids.len());
    for id in body.ids {
        match control.delete_as_platform_admin(&authority, AgentId::from(id)).await {
            Ok(()) => results.push(BulkDeleteResult { id, ok: true, error: None }),
            Err(err) => results.push(BulkDeleteResult {
                id,
                ok: false,
                error: Some(AdminBulkDeletePolicy::error_message(&err)),
            }),
        }
    }
    Ok(Json(admin_bulk_delete_response(results)))
}

/// Query parameters for `GET /admin/dead-events`: 1-based `page`, `limit`
/// (clamped to 1..=100 by the service), and an optional exact `reason` filter.
#[derive(Debug, Deserialize)]
pub struct DeadEventsQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_agents_limit")]
    pub limit: i64,
    #[serde(default)]
    pub reason: Option<String>,
}

/// `GET /api/v1/admin/dead-events` — paginated, cross-org list of captured
/// dead-letter rows (permanently-dropped NATS envelopes), newest first.
///
/// PLATFORM-ADMIN-ONLY: gated on the server-side `users.is_admin` column, NOT
/// the per-org membership role. The view is cross-org by design, so a
/// self-registered user who is `owner` of their personal org must not reach it —
/// a non-admin gets 403. `payload_excerpt` is UNTRUSTED (stored-XSS-capable, may
/// contain other orgs' task output); any UI must render it escaped.
async fn list_dead_events(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<DeadEventsQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let service = make_service(&state);
    service.require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    let page = service.list_dead_events(query.page, query.limit, query.reason.as_deref()).await?;
    Ok(Json(admin_data_response(page)))
}

/// `GET /api/v1/admin/cli-images` — read-only status of the CLI agent-image
/// auto-updater: per-tool image state, local/remote digests, last check/error,
/// and a rough per-tool live-container count. Deployment-global (no tenant
/// scope) since image state is per host; admin-gated.
async fn list_cli_image_status(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    make_service(&state).require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    let report = state.cli_image_service().status_report().await?;
    Ok(Json(cli_image_status_response(report)))
}

/// `POST /api/v1/admin/cli-images/{tool}/roll` — drain + respawn the running
/// container agents of one tool onto the freshly re-tagged image. DESTRUCTIVE:
/// it interrupts running agents (in-flight work surfaces as `agent_lost`).
/// Admin-gated, operator-initiated, never `claude`. Returns a per-agent report;
/// a tool that is unknown/claude → 422, a concurrent roll of the same tool →
/// 409. Cross-org by design, through sealed platform-admin lifecycle authority.
async fn roll_cli_image(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tool): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let authority = make_service(&state).require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    // Defense-in-depth: reject claude/unknown at the route too (the service
    // re-checks). 422, not 404 — the path matched; the tool is just not rollable.
    RollToolPolicy::ensure_rollable(&tool)?;
    let report = state.cli_image_roll_service().roll(&authority, &tool).await?;
    Ok(Json(cli_image_roll_response(report)))
}

/// `POST /api/v1/admin/cli-images/{tool}/build` — build the `claude` agent
/// image locally on this server (claude has no public registry image; its
/// license requires a self-build). NON-destructive to agents: image-level
/// only, running agents are untouched — the NEXT spawn picks up the new image.
/// Answers `202 { ok, started, targetVersion }` and runs the docker build in
/// the background; progress + outcome land in the status report and the admin
/// toast. Admin-gated. A non-claude/unknown tool → 422, a build already in
/// flight → 409, container runtime or npm registry unavailable → 503 (nothing
/// started).
async fn build_cli_image(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tool): Path<String>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    make_service(&state).require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    // Defense-in-depth: reject non-claude at the route too (the service
    // re-checks). 422, not 404 — the path matched; the tool is just not built
    // locally.
    LocalBuildToolPolicy::ensure_local_buildable(&tool)?;
    let target_version = state.cli_image_build_service().start_build(&tool).await?;
    Ok((StatusCode::ACCEPTED, Json(cli_image_build_response(&target_version))))
}

/// Build admin routes sub-router.
pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(list_users))
        .route("/admin/users/{id}", put(update_admin_user_role).delete(delete_admin_user))
        .route("/admin/organizations", get(list_organizations))
        .route("/admin/agents", get(list_admin_agents).delete(bulk_delete_admin_agents))
        .route("/admin/agents/{id}", get(get_admin_agent).delete(delete_admin_agent))
        .route("/admin/impersonate", post(start_impersonation))
        .route("/admin/impersonate/end", post(end_impersonation))
        .route("/admin/impersonation-log", get(list_impersonation_log))
        .route("/admin/stats", get(get_stats))
        .route("/admin/control-plane", get(get_control_plane))
        .route("/admin/dead-events", get(list_dead_events))
        .route("/admin/cli-images", get(list_cli_image_status))
        .route("/admin/cli-images/{tool}/roll", post(roll_cli_image))
        .route("/admin/cli-images/{tool}/build", post(build_cli_image))
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
    fn admin_users_query_defaults() {
        let query: AdminUsersQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.page, 1);
        assert_eq!(query.limit, 25);
        assert!(query.search.is_none());
    }

    #[test]
    fn admin_users_query_custom_values() {
        let query: AdminUsersQuery = serde_json::from_str(r#"{"page": 3, "limit": 50, "search": "alice"}"#).unwrap();
        assert_eq!(query.page, 3);
        assert_eq!(query.limit, 50);
        assert_eq!(query.search.as_deref(), Some("alice"));
    }

    #[test]
    fn update_user_role_request_deserialization() {
        let req: UpdateUserRoleRequest = serde_json::from_str(r#"{"role": "admin"}"#).unwrap();
        assert_eq!(req.role, "admin");
        let req: UpdateUserRoleRequest = serde_json::from_str(r#"{"role": "member"}"#).unwrap();
        assert_eq!(req.role, "member");
    }

    #[test]
    fn update_user_role_request_requires_role() {
        // The wire value is validated by the domain (`AdminRoleChange::parse`);
        // the body shape itself only requires the `role` key to exist.
        assert!(serde_json::from_str::<UpdateUserRoleRequest>(r#"{}"#).is_err());
        assert!(serde_json::from_str::<UpdateUserRoleRequest>(r#"{"role": 1}"#).is_err());
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

    // The dead-letter reader gate is now keyed off `users.is_admin`
    // (`AdminService::require_platform_admin`), which loads the caller's user
    // row — see the `#[sqlx::test]` `require_platform_admin_keys_off_is_admin_column`
    // in services::admin and the pure-policy test in domain::admin.

    #[test]
    fn dead_events_query_defaults_and_custom() {
        let query: DeadEventsQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.page, 1);
        assert_eq!(query.limit, 25);
        assert!(query.reason.is_none());

        let query: DeadEventsQuery =
            serde_json::from_str(r#"{"page": 2, "limit": 50, "reason": "signature_mismatch"}"#).unwrap();
        assert_eq!(query.page, 2);
        assert_eq!(query.limit, 50);
        assert_eq!(query.reason.as_deref(), Some("signature_mismatch"));
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
        assert!(query.runtime_kind.is_none());
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
            "runtimeKind": "cli",
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
        assert_eq!(query.runtime_kind.as_deref(), Some("cli"));
        assert!(query.user_id.is_some());
        assert!(query.project_id.is_some());
        assert_eq!(query.page, 3);
        assert_eq!(query.limit, 50);
        assert_eq!(query.sort_by.as_deref(), Some("lastActivity"));
        assert_eq!(query.sort_order.as_deref(), Some("asc"));
    }

    #[test]
    fn admin_agents_query_carries_runtime_kind_into_service_input() {
        let query: AdminAgentsQuery = serde_json::from_str(r#"{"runtimeKind": "container"}"#).unwrap();
        assert_eq!(query.as_service_input().runtime_kind, Some("container"));
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
