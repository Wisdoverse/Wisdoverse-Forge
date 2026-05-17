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
use serde_json::json;
use uuid::Uuid;

use agentforge_auth::AuthUser;
use agentforge_core::{AppResult, ErrorKind};

use crate::domain::admin::{AdminAgentFilterPolicy, AdminAgentFilterQuery};
use crate::health::AppState;
use crate::repositories::admin::{AdminAgentEventRow, AdminAgentFilters, AdminAgentRow, AdminRepository};
use crate::services::admin::AdminService;

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
    AdminService::new(AdminRepository::new(state.pool.clone()))
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
    Ok(Json(serde_json::json!({ "ok": true, "data": users })))
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
    Ok(Json(serde_json::json!({ "ok": true, "data": orgs })))
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
    Ok(Json(serde_json::json!({ "ok": true, "data": log })))
}

/// `POST /api/v1/admin/impersonate/end` — end impersonation.
async fn end_impersonation(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let log = service.end_impersonation(&auth.scope).await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": log })))
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
    Ok(Json(serde_json::json!({ "ok": true, "data": logs })))
}

/// `GET /api/v1/admin/stats` — system-wide statistics.
async fn get_stats(State(state): State<AppState>, auth: AuthUser) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let stats = service.stats().await?;
    Ok(Json(serde_json::json!({ "ok": true, "data": stats })))
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

/// Build the `AdminAgentFilters` struct passed down to the repository.
fn filters_from_query(query: &AdminAgentsQuery) -> AdminAgentFilters {
    let decision = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
        search: query.search.as_deref(),
        status: query.status.as_deref(),
        page: query.page,
        limit: query.limit,
        sort_by: query.sort_by.as_deref(),
        sort_order: query.sort_order.as_deref(),
    });

    AdminAgentFilters {
        search: decision.search,
        status: decision.status,
        user_id: query.user_id,
        project_id: query.project_id,
        sort_by: decision.sort_by,
        sort_order: decision.sort_order,
        limit: decision.limit,
        offset: decision.offset,
    }
}

fn page_from_query(query: &AdminAgentsQuery) -> i64 {
    AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
        search: query.search.as_deref(),
        status: query.status.as_deref(),
        page: query.page,
        limit: query.limit,
        sort_by: query.sort_by.as_deref(),
        sort_order: query.sort_order.as_deref(),
    })
    .page
}

/// Shape a DB row into the JSON object the frontend `AdminAgent` interface
/// expects. Columns that do not yet exist in the Rust `agents` schema
/// (`cwd`, `current_tool`, `tokens_*`, `git_status`, `runtime_id`) are
/// emitted as null / zero / empty string so the UI still renders gracefully.
///
fn admin_agent_row_to_json(row: &AdminAgentRow) -> serde_json::Value {
    json!({
        "id": row.id,
        "name": row.name.clone().unwrap_or_default(),
        "status": row.status,
        "cwd": row.cwd.clone().unwrap_or_default(),
        "currentTool": row.current_tool,
        "cliTool": row.cli_tool,
        "tokens": { "current": row.tokens_current, "cumulative": row.tokens_cumulative },
        "gitBranch": row.git_status,
        "ownerUsername": row.owner_username,
        "ownerEmail": row.owner_email,
        "projectName": row.project_name,
        "createdAt": row.created_at.timestamp_millis(),
        "lastActivity": row.last_activity.timestamp_millis(),
        "runtimeId": row.runtime_id.clone().unwrap_or_default(),
        "containerId": row.container_id,
        "eventsCount": row.events_count,
    })
}

/// Shape a recent-events row for the admin agent detail panel.
fn admin_event_row_to_json(row: &AdminAgentEventRow) -> serde_json::Value {
    json!({
        "id": row.id,
        "type": row.event_type,
        "toolName": null,
        "createdAt": row.created_at.timestamp_millis(),
    })
}

/// `GET /api/v1/admin/agents` — paginated list of agents across all tenants.
async fn list_admin_agents(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<AdminAgentsQuery>,
) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);

    let filters = filters_from_query(&query);
    let page = page_from_query(&query);
    let limit = filters.limit;
    let (rows, total) = service.list_agents(filters).await?;

    let agents: Vec<serde_json::Value> = rows.iter().map(admin_agent_row_to_json).collect();
    let total_pages = if limit > 0 { (total + limit - 1) / limit } else { 0 };

    Ok(Json(json!({
        "ok": true,
        "agents": agents,
        "total": total,
        "page": page,
        "limit": limit,
        "totalPages": total_pages,
    })))
}

/// `GET /api/v1/admin/agents/:id` — agent detail including recent events.
async fn get_admin_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    let service = make_service(&state);
    let (row, events) = service.get_agent(id).await?;

    let mut agent = admin_agent_row_to_json(&row);
    // Augment the list shape with the extra fields the detail panel expects.
    if let Some(obj) = agent.as_object_mut() {
        obj.insert("userId".into(), json!(row.user_id));
        obj.insert("orgId".into(), json!(row.organization_id));
        obj.insert("projectId".into(), json!(row.project_id));
        obj.insert("cliSessionId".into(), json!(row.cli_session_id));
        obj.insert("claudeFlags".into(), serde_json::Value::Null);
        obj.insert("groupId".into(), serde_json::Value::Null);
        obj.insert("gitStatus".into(), serde_json::Value::Null);
        obj.insert("recentEvents".into(), json!(events.iter().map(admin_event_row_to_json).collect::<Vec<_>>()));
    }

    Ok(Json(json!({ "ok": true, "agent": agent })))
}

/// `DELETE /api/v1/admin/agents/:id` — hard-delete a single agent.
async fn delete_admin_agent(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    // Revoke the live NATS connection BEFORE the DB row vanishes — once the
    // row is gone, the callout handler would deny any reconnect anyway, but
    // publishing the KICK while the tracker entry is still keyed by this
    // agent ID yields a clean ≤2s cutoff instead of the 15-min JWT ceiling.
    match state.auth_callout.as_ref() {
        Some(callout) => callout.revoke(id).await,
        None => tracing::info!(
            %id,
            "admin delete_agent: auth callout disabled — revocation falls back to JWT TTL"
        ),
    }
    let service = make_service(&state);
    service.delete_agent(id).await?;
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /api/v1/admin/agents` — bulk delete via a JSON `{ ids: [...] }` body.
async fn bulk_delete_admin_agents(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<BulkDeleteRequest>,
) -> AppResult<Json<serde_json::Value>> {
    AdminService::require_admin(&auth.role)?;
    if body.ids.is_empty() {
        return Err(ErrorKind::Validation("ids array required".into()).into());
    }
    // Revoke each agent's live NATS connection before bulk deletion. The
    // revoke() method is best-effort and logs internally, so a single
    // loop that ignores failures is the right shape — a partial KICK
    // fleet-out does not block the DB delete, and `bulk_delete_agents`
    // below reports per-id success/failure for the DB step.
    match state.auth_callout.as_ref() {
        Some(callout) => {
            for id in &body.ids {
                callout.revoke(*id).await;
            }
        }
        None => tracing::info!(
            count = body.ids.len(),
            "admin bulk_delete_agents: auth callout disabled — revocation falls back to JWT TTL"
        ),
    }
    let service = make_service(&state);
    let results = service.bulk_delete_agents(&body.ids).await;
    Ok(Json(json!({ "ok": true, "results": results })))
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
    use crate::domain::admin::{AdminAgentSort, SortOrder};
    use agentforge_core::AgentStatus;

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
    fn filters_from_query_paginates_and_clamps() {
        let query: AdminAgentsQuery =
            serde_json::from_str(r#"{"page": 4, "limit": 10, "search": "  ", "sortBy": "name", "sortOrder": "asc"}"#)
                .unwrap();
        let filters = filters_from_query(&query);
        assert_eq!(filters.limit, 10);
        assert_eq!(filters.offset, 30); // (page 4 - 1) * 10
        // Blank-only search strings are dropped so they don't hit SQL.
        assert!(filters.search.is_none());
        assert_eq!(filters.sort_by, AdminAgentSort::Name);
        assert_eq!(filters.sort_order, SortOrder::Asc);
    }

    #[test]
    fn filters_from_query_clamps_limit_to_100() {
        let query: AdminAgentsQuery = serde_json::from_str(r#"{"page": 1, "limit": 500}"#).unwrap();
        let filters = filters_from_query(&query);
        assert_eq!(filters.limit, 100);
        assert_eq!(filters.offset, 0);
    }

    #[test]
    fn filters_from_query_floor_page_to_one() {
        let query: AdminAgentsQuery = serde_json::from_str(r#"{"page": 0, "limit": 25}"#).unwrap();
        let filters = filters_from_query(&query);
        assert_eq!(filters.offset, 0);
    }

    #[test]
    fn admin_agent_row_to_json_uses_camel_case_and_epoch_ms() {
        use chrono::{TimeZone, Utc};
        let row = AdminAgentRow {
            id: Uuid::nil(),
            name: Some("worker".into()),
            status: AgentStatus::Working,
            model: Some("claude".into()),
            provider: Some("anthropic".into()),
            container_id: Some("abc123".into()),
            cli_session_id: None,
            cwd: Some("/workspace/agentforge".into()),
            current_tool: Some("Edit".into()),
            cli_tool: Some("claude".into()),
            tokens_current: 1234,
            tokens_cumulative: 56789,
            git_status: Some("+3 -1".into()),
            runtime_id: Some("af-deadbeef".into()),
            organization_id: Uuid::nil(),
            project_id: None,
            user_id: Uuid::nil(),
            owner_username: Some("alice".into()),
            owner_email: Some("alice@example.com".into()),
            project_name: Some("P".into()),
            created_at: Utc.timestamp_millis_opt(1_700_000_000_000).unwrap(),
            updated_at: Utc.timestamp_millis_opt(1_700_000_100_000).unwrap(),
            last_activity: Utc.timestamp_millis_opt(1_700_000_200_000).unwrap(),
            events_count: 42,
        };
        let value = admin_agent_row_to_json(&row);

        // Owner fields are surfaced at the expected camelCase keys.
        assert_eq!(value["ownerUsername"], "alice");
        assert_eq!(value["ownerEmail"], "alice@example.com");
        assert_eq!(value["projectName"], "P");

        // Timestamps are epoch milliseconds (numeric), not ISO strings.
        assert_eq!(value["createdAt"], 1_700_000_000_000_i64);
        assert_eq!(value["lastActivity"], 1_700_000_200_000_i64);

        // Migration 013 surfaces real runtime data instead of placeholders.
        assert_eq!(value["cwd"], "/workspace/agentforge");
        assert_eq!(value["runtimeId"], "af-deadbeef");
        assert_eq!(value["currentTool"], "Edit");
        assert_eq!(value["gitBranch"], "+3 -1");
        assert_eq!(value["tokens"]["current"], 1234);
        assert_eq!(value["tokens"]["cumulative"], 56789);

        assert_eq!(value["eventsCount"], 42);
    }

    #[test]
    fn admin_agent_row_to_json_emits_cli_tool() {
        use chrono::Utc;
        let row = AdminAgentRow {
            id: uuid::Uuid::nil(),
            name: Some("t".into()),
            status: AgentStatus::Idle,
            model: None,
            provider: None,
            container_id: None,
            cli_session_id: None,
            cwd: None,
            current_tool: None,
            cli_tool: Some("claude".into()),
            tokens_current: 0,
            tokens_cumulative: 0,
            git_status: None,
            runtime_id: None,
            organization_id: uuid::Uuid::nil(),
            project_id: None,
            user_id: uuid::Uuid::nil(),
            owner_username: None,
            owner_email: None,
            project_name: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_activity: Utc::now(),
            events_count: 0,
        };
        let v = admin_agent_row_to_json(&row);
        assert_eq!(v["cliTool"], serde_json::json!("claude"));
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
