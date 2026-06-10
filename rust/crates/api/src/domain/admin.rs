//! Admin domain rules.
//!
//! This module owns pure admin-console policies that are independent of
//! repositories and HTTP route DTOs.

use agentforge_core::{AgentStatus, AppError, AppResult, ErrorKind, RuntimeKind};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

/// Per-ID outcome from a bulk-delete call, serialised in admin API responses.
#[derive(Debug, Clone, Serialize)]
pub struct BulkDeleteResult {
    pub id: Uuid,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Agent token counters exposed to the admin console.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AdminAgentTokens {
    pub(crate) current: i64,
    pub(crate) cumulative: i64,
}

impl AdminAgentTokens {
    pub(crate) fn new(current: i64, cumulative: i64) -> Self {
        Self { current, cumulative }
    }
}

/// Admin-console agent projection consumed by the legacy React admin table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminAgentProjection {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) status: AgentStatus,
    pub(crate) cwd: String,
    pub(crate) current_tool: Option<String>,
    pub(crate) cli_tool: Option<String>,
    pub(crate) tokens: AdminAgentTokens,
    pub(crate) git_branch: Option<String>,
    pub(crate) owner_username: Option<String>,
    pub(crate) owner_email: Option<String>,
    pub(crate) project_name: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) last_activity: i64,
    pub(crate) runtime_id: String,
    /// Execution surface discriminator (`"container" | "cli" | "api"`). Lets the
    /// admin console filter and badge agents by runtime kind.
    pub(crate) runtime_kind: RuntimeKind,
    pub(crate) container_id: Option<String>,
    pub(crate) events_count: i64,
}

/// Recent event projection for the admin agent detail panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminAgentEventProjection {
    pub(crate) id: Uuid,
    #[serde(rename = "type")]
    pub(crate) event_type: String,
    pub(crate) tool_name: Option<String>,
    pub(crate) created_at: i64,
}

/// Detail response projection for a single admin agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdminAgentDetailProjection {
    pub(crate) agent: AdminAgentProjection,
    pub(crate) user_id: Uuid,
    pub(crate) organization_id: Uuid,
    pub(crate) project_id: Option<Uuid>,
    pub(crate) cli_session_id: Option<String>,
    pub(crate) recent_events: Vec<AdminAgentEventProjection>,
}

/// Paginated admin-console agent list projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdminAgentListProjection {
    pub(crate) agents: Vec<AdminAgentProjection>,
    pub(crate) total: i64,
    pub(crate) page: i64,
    pub(crate) limit: i64,
}

impl AdminAgentListProjection {
    pub(crate) fn new(agents: Vec<AdminAgentProjection>, total: i64, page: i64, limit: i64) -> Self {
        Self { agents, total, page, limit }
    }

    fn total_pages(&self) -> i64 {
        if self.limit > 0 { (self.total + self.limit - 1) / self.limit } else { 0 }
    }
}

/// Admin-console user projection consumed by the React admin "User access"
/// table. `role` is derived from `users.is_admin` (`"admin" | "member"`);
/// `status` is always `"active"` because the list query filters soft-deleted
/// rows — the field stays so the response contract is explicit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminUserProjection {
    pub(crate) id: Uuid,
    pub(crate) email: String,
    pub(crate) display_name: String,
    pub(crate) role: String,
    pub(crate) status: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) last_login_at: Option<DateTime<Utc>>,
}

impl From<agentforge_db::entities::User> for AdminUserProjection {
    fn from(user: agentforge_db::entities::User) -> Self {
        let display_name = match user.display_name.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() => name.to_string(),
            // Fall back to the local part of the email so the admin table
            // never renders a blank "Person" cell.
            _ => user.email.split('@').next().unwrap_or(user.email.as_str()).to_string(),
        };
        Self {
            id: user.id.as_uuid(),
            display_name,
            role: if user.is_admin { "admin".to_string() } else { "member".to_string() },
            status: "active".to_string(),
            created_at: user.created_at,
            last_login_at: user.last_login_at,
            email: user.email,
        }
    }
}

/// Paginated admin-console user list projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdminUserListProjection {
    pub(crate) users: Vec<AdminUserProjection>,
    pub(crate) total: i64,
    pub(crate) page: i64,
    pub(crate) limit: i64,
    pub(crate) total_pages: i64,
}

impl AdminUserListProjection {
    pub(crate) fn new(users: Vec<AdminUserProjection>, total: i64, page: i64, limit: i64) -> Self {
        let total_pages = if limit > 0 { (total + limit - 1) / limit } else { 0 };
        Self { users, total, page, limit, total_pages }
    }
}

/// Admin-console organization projection. There is intentionally NO `plan`
/// field: the organizations table has no plan column, so the contract must
/// not pretend one exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdminOrgProjection {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) slug: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) members_count: i64,
    pub(crate) teams_count: i64,
}

pub(crate) fn admin_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

pub(crate) fn admin_user_list_response(projection: AdminUserListProjection) -> Value {
    json!({
        "ok": true,
        "users": projection.users,
        "total": projection.total,
        "page": projection.page,
        "limit": projection.limit,
        "totalPages": projection.total_pages,
    })
}

pub(crate) fn admin_org_list_response(organizations: Vec<AdminOrgProjection>, total: i64) -> Value {
    json!({ "ok": true, "organizations": organizations, "total": total })
}

pub(crate) fn admin_delete_response() -> Value {
    json!({ "ok": true })
}

pub(crate) fn admin_bulk_delete_response(results: Vec<BulkDeleteResult>) -> Value {
    json!({ "ok": true, "results": results })
}

pub(crate) fn admin_agent_list_response(projection: AdminAgentListProjection) -> Value {
    let total_pages = projection.total_pages();
    json!({
        "ok": true,
        "agents": projection.agents,
        "total": projection.total,
        "page": projection.page,
        "limit": projection.limit,
        "totalPages": total_pages,
    })
}

pub(crate) fn admin_agent_detail_response(detail: AdminAgentDetailProjection) -> Value {
    let AdminAgentDetailProjection { agent, user_id, organization_id, project_id, cli_session_id, recent_events } =
        detail;
    let mut agent = json!(agent);
    if let Some(obj) = agent.as_object_mut() {
        obj.insert("userId".into(), json!(user_id));
        obj.insert("orgId".into(), json!(organization_id));
        obj.insert("projectId".into(), json!(project_id));
        obj.insert("cliSessionId".into(), json!(cli_session_id));
        obj.insert("claudeFlags".into(), Value::Null);
        obj.insert("groupId".into(), Value::Null);
        obj.insert("gitStatus".into(), Value::Null);
        obj.insert("recentEvents".into(), json!(recent_events));
    }

    json!({ "ok": true, "agent": agent })
}

/// Admin repository lookup policy.
pub(crate) struct AdminRepositoryPolicy;

impl AdminRepositoryPolicy {
    pub(crate) fn active_impersonation_not_found() -> AppError {
        ErrorKind::NotFound("no active impersonation session".into()).into()
    }

    pub(crate) fn agent_not_found(agent_id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("agent {agent_id}")).into()
    }
}

/// Validated pagination request for admin list endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AdminListPage {
    limit: i64,
    offset: i64,
}

impl AdminListPage {
    pub(crate) fn new(limit: i64, offset: i64) -> Self {
        Self { limit: limit.clamp(1, 100), offset: offset.max(0) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn offset(self) -> i64 {
        self.offset
    }
}

/// Columns the admin agent list can be sorted by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AdminAgentSort {
    Name,
    Status,
    #[default]
    LastActivity,
    CreatedAt,
    OwnerUsername,
}

/// Sort direction for admin list queries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    #[default]
    Desc,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AdminAgentFilterQuery<'a> {
    pub search: Option<&'a str>,
    pub status: Option<&'a str>,
    pub runtime_kind: Option<&'a str>,
    pub page: i64,
    pub limit: i64,
    pub sort_by: Option<&'a str>,
    pub sort_order: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdminAgentFilterDecision {
    pub search: Option<String>,
    pub status: Option<AgentStatus>,
    pub runtime_kind: Option<RuntimeKind>,
    pub page: i64,
    pub limit: i64,
    pub offset: i64,
    pub sort_by: AdminAgentSort,
    pub sort_order: SortOrder,
}

/// Admin agent list parsing and pagination policy.
pub(crate) struct AdminAgentFilterPolicy;

impl AdminAgentFilterPolicy {
    /// Parse the validated admin-agent filter decision from raw query input.
    ///
    /// `status` is intentionally lenient (unsupported values are dropped) so the
    /// frontend's wider status enum never fails the whole request. `runtime_kind`
    /// is strict: an unknown value is rejected with an `Unprocessable` (HTTP 422)
    /// error so operators get a clear signal instead of a silently empty list.
    pub(crate) fn from_query(query: AdminAgentFilterQuery<'_>) -> AppResult<AdminAgentFilterDecision> {
        let page = query.page.max(1);
        let limit = query.limit.clamp(1, 100);
        Ok(AdminAgentFilterDecision {
            search: query.search.and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }),
            status: Self::parse_status_filter(query.status),
            runtime_kind: Self::parse_runtime_kind_filter(query.runtime_kind)?,
            page,
            limit,
            offset: (page - 1) * limit,
            sort_by: Self::parse_sort_by(query.sort_by),
            sort_order: Self::parse_sort_order(query.sort_order),
        })
    }

    fn parse_status_filter(raw: Option<&str>) -> Option<AgentStatus> {
        match raw?.to_ascii_lowercase().as_str() {
            "working" => Some(AgentStatus::Working),
            "idle" => Some(AgentStatus::Idle),
            "offline" => Some(AgentStatus::Offline),
            _ => None,
        }
    }

    /// Strictly parse the optional `runtimeKind` filter. A blank value behaves
    /// like "no filter"; any other unknown value is rejected with HTTP 422.
    fn parse_runtime_kind_filter(raw: Option<&str>) -> AppResult<Option<RuntimeKind>> {
        let Some(value) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
            return Ok(None);
        };
        match RuntimeKind::parse_legacy(value) {
            Ok(kind) => Ok(Some(kind)),
            Err(_) => Err(ErrorKind::Unprocessable(format!(
                "unknown runtimeKind '{value}'; expected one of {}",
                RuntimeKind::SUPPORTED_SLUGS
            ))
            .into()),
        }
    }

    fn parse_sort_by(raw: Option<&str>) -> AdminAgentSort {
        match raw.unwrap_or("") {
            "name" => AdminAgentSort::Name,
            "status" => AdminAgentSort::Status,
            "createdAt" => AdminAgentSort::CreatedAt,
            "ownerUsername" => AdminAgentSort::OwnerUsername,
            _ => AdminAgentSort::LastActivity,
        }
    }

    fn parse_sort_order(raw: Option<&str>) -> SortOrder {
        match raw.unwrap_or("").to_ascii_lowercase().as_str() {
            "asc" => SortOrder::Asc,
            _ => SortOrder::Desc,
        }
    }
}

/// Admin role authorization policy.
pub(crate) struct AdminRolePolicy;

impl AdminRolePolicy {
    pub(crate) fn require_admin(auth_role: &str) -> AppResult<()> {
        match auth_role {
            "owner" | "admin" => Ok(()),
            _ => Err(ErrorKind::Forbidden("forbidden".into()).into()),
        }
    }
}

/// Admin impersonation policy.
pub(crate) struct AdminImpersonationPolicy;

impl AdminImpersonationPolicy {
    pub(crate) fn ensure_not_self(current_user_id: Uuid, target_user_id: Uuid) -> AppResult<()> {
        if current_user_id == target_user_id {
            return Err(ErrorKind::Validation("cannot impersonate yourself".into()).into());
        }
        Ok(())
    }
}

/// Admin bulk-delete request and error-projection policy.
pub(crate) struct AdminBulkDeletePolicy;

impl AdminBulkDeletePolicy {
    pub(crate) fn require_ids(agent_ids: &[Uuid]) -> AppResult<()> {
        if agent_ids.is_empty() {
            return Err(ErrorKind::Validation("ids array required".into()).into());
        }
        Ok(())
    }

    pub(crate) fn error_message(err: &AppError) -> String {
        match &err.kind {
            ErrorKind::NotFound(_) => "agent not found".to_string(),
            ErrorKind::Validation(msg) => format!("validation error: {msg}"),
            ErrorKind::ValidationWithCode { message, .. } => format!("validation error: {message}"),
            ErrorKind::Unprocessable(msg) => format!("unprocessable entity: {msg}"),
            ErrorKind::Conflict(msg) => format!("conflict: {msg}"),
            ErrorKind::Unauthorized => "unauthorized".to_string(),
            ErrorKind::Forbidden(_) => "forbidden".to_string(),
            ErrorKind::ForbiddenWithCode { .. } => "forbidden".to_string(),
            ErrorKind::Unavailable(msg) => format!("service unavailable: {msg}"),
            ErrorKind::Internal(_) => "delete failed".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_list_page_clamps_limit_and_offset() {
        assert_eq!(AdminListPage::new(0, -10).limit(), 1);
        assert_eq!(AdminListPage::new(200, 10).limit(), 100);
        assert_eq!(AdminListPage::new(50, -10).offset(), 0);
        assert_eq!(AdminListPage::new(50, 10).offset(), 10);
    }

    #[test]
    fn admin_role_policy_accepts_owner_and_admin() {
        assert!(AdminRolePolicy::require_admin("owner").is_ok());
        assert!(AdminRolePolicy::require_admin("admin").is_ok());
    }

    #[test]
    fn admin_role_policy_rejects_non_admin_roles() {
        assert!(AdminRolePolicy::require_admin("member").is_err());
        assert!(AdminRolePolicy::require_admin("viewer").is_err());
        assert!(AdminRolePolicy::require_admin("").is_err());
    }

    #[test]
    fn admin_impersonation_policy_rejects_self_target() {
        let user_id = Uuid::now_v7();
        assert!(AdminImpersonationPolicy::ensure_not_self(user_id, user_id).is_err());
        assert!(AdminImpersonationPolicy::ensure_not_self(user_id, Uuid::now_v7()).is_ok());
    }

    #[test]
    fn admin_bulk_delete_policy_owns_ids_and_error_projection() {
        assert!(AdminBulkDeletePolicy::require_ids(&[Uuid::now_v7()]).is_ok());
        assert!(AdminBulkDeletePolicy::require_ids(&[]).is_err());

        let validation: AppError = ErrorKind::Validation("bad id".into()).into();
        let internal: AppError = ErrorKind::Internal(anyhow::anyhow!("db failed")).into();
        assert_eq!(AdminBulkDeletePolicy::error_message(&validation), "validation error: bad id");
        assert_eq!(AdminBulkDeletePolicy::error_message(&internal), "delete failed");
    }

    #[test]
    fn admin_repository_policy_owns_lookup_errors() {
        let agent_id = Uuid::new_v4();

        assert!(matches!(
            AdminRepositoryPolicy::active_impersonation_not_found().kind,
            ErrorKind::NotFound(message) if message == "no active impersonation session"
        ));
        assert!(matches!(
            AdminRepositoryPolicy::agent_not_found(agent_id).kind,
            ErrorKind::NotFound(message) if message == format!("agent {agent_id}")
        ));
    }

    /// Build a query with the given status; all other fields are unset.
    fn status_query(status: Option<&str>) -> AdminAgentFilterQuery<'_> {
        AdminAgentFilterQuery {
            search: None,
            status,
            runtime_kind: None,
            page: 1,
            limit: 25,
            sort_by: None,
            sort_order: None,
        }
    }

    #[test]
    fn admin_agent_filter_accepts_supported_statuses() {
        assert_eq!(
            AdminAgentFilterPolicy::from_query(status_query(Some("working"))).unwrap().status,
            Some(AgentStatus::Working)
        );
        assert_eq!(
            AdminAgentFilterPolicy::from_query(status_query(Some("WORKING"))).unwrap().status,
            Some(AgentStatus::Working)
        );
    }

    #[test]
    fn admin_agent_filter_drops_unsupported_statuses() {
        for status in [Some("waiting"), Some("attention"), Some("bogus"), None] {
            let decision = AdminAgentFilterPolicy::from_query(status_query(status)).unwrap();
            assert_eq!(decision.status, None);
        }
    }

    #[test]
    fn admin_agent_filter_accepts_canonical_runtime_kinds() {
        let cases = [
            (Some("container"), Some(RuntimeKind::Container)),
            (Some(" CLI "), Some(RuntimeKind::Cli)),
            (Some("Api"), Some(RuntimeKind::Api)),
            (Some("   "), None),
            (None, None),
        ];
        for (raw, expected) in cases {
            let decision = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
                search: None,
                status: None,
                runtime_kind: raw,
                page: 1,
                limit: 25,
                sort_by: None,
                sort_order: None,
            })
            .expect("valid runtime kind filter");
            assert_eq!(decision.runtime_kind, expected);
        }
    }

    #[test]
    fn admin_agent_filter_rejects_unknown_runtime_kind_with_422() {
        for raw in ["host_cli", "docker", "lambda", "bogus"] {
            let err = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
                search: None,
                status: None,
                runtime_kind: Some(raw),
                page: 1,
                limit: 25,
                sort_by: None,
                sort_order: None,
            })
            .expect_err("unknown runtimeKind must be rejected");
            assert!(
                matches!(err.kind, ErrorKind::Unprocessable(ref msg) if msg.contains(raw)),
                "expected Unprocessable carrying the bad value, got {err:?}"
            );
        }
    }

    #[test]
    fn admin_agent_filter_maps_sorting_and_defaults() {
        let cases = [
            (Some("name"), AdminAgentSort::Name),
            (Some("status"), AdminAgentSort::Status),
            (Some("createdAt"), AdminAgentSort::CreatedAt),
            (Some("ownerUsername"), AdminAgentSort::OwnerUsername),
            (Some("lastActivity"), AdminAgentSort::LastActivity),
            (Some("tokens"), AdminAgentSort::LastActivity),
            (Some("cwd"), AdminAgentSort::LastActivity),
            (None, AdminAgentSort::LastActivity),
        ];

        for (sort_by, expected) in cases {
            let decision = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
                search: None,
                status: None,
                runtime_kind: None,
                page: 1,
                limit: 25,
                sort_by,
                sort_order: Some("ASC"),
            })
            .unwrap();
            assert_eq!(decision.sort_by, expected);
            assert_eq!(decision.sort_order, SortOrder::Asc);
        }
    }

    #[test]
    fn admin_agent_filter_paginates_clamps_and_drops_blank_search() {
        let decision = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
            search: Some("  "),
            status: None,
            runtime_kind: None,
            page: 4,
            limit: 10,
            sort_by: Some("name"),
            sort_order: Some("asc"),
        })
        .unwrap();

        assert_eq!(decision.search, None);
        assert_eq!(decision.page, 4);
        assert_eq!(decision.limit, 10);
        assert_eq!(decision.offset, 30);
        assert_eq!(decision.sort_by, AdminAgentSort::Name);
        assert_eq!(decision.sort_order, SortOrder::Asc);

        let clamped = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
            search: Some(" user@example.com "),
            status: None,
            runtime_kind: None,
            page: -4,
            limit: 500,
            sort_by: None,
            sort_order: Some("nope"),
        })
        .unwrap();
        assert_eq!(clamped.search.as_deref(), Some("user@example.com"));
        assert_eq!(clamped.page, 1);
        assert_eq!(clamped.limit, 100);
        assert_eq!(clamped.offset, 0);
        assert_eq!(clamped.sort_order, SortOrder::Desc);
    }

    #[test]
    fn admin_agent_projection_serializes_admin_table_contract() {
        let value = serde_json::to_value(AdminAgentProjection {
            id: Uuid::nil(),
            name: "worker".into(),
            status: AgentStatus::Working,
            cwd: "/workspace/agentforge".into(),
            current_tool: Some("Edit".into()),
            cli_tool: Some("claude".into()),
            tokens: AdminAgentTokens::new(1234, 56789),
            git_branch: Some("+3 -1".into()),
            owner_username: Some("alice".into()),
            owner_email: Some("alice@example.com".into()),
            project_name: Some("P".into()),
            created_at: 1_700_000_000_000,
            last_activity: 1_700_000_200_000,
            runtime_id: "af-deadbeef".into(),
            runtime_kind: RuntimeKind::Container,
            container_id: Some("abc123".into()),
            events_count: 42,
        })
        .unwrap();

        assert_eq!(value["ownerUsername"], "alice");
        assert_eq!(value["ownerEmail"], "alice@example.com");
        assert_eq!(value["projectName"], "P");
        assert_eq!(value["createdAt"], 1_700_000_000_000_i64);
        assert_eq!(value["lastActivity"], 1_700_000_200_000_i64);
        assert_eq!(value["cwd"], "/workspace/agentforge");
        assert_eq!(value["runtimeId"], "af-deadbeef");
        assert_eq!(value["runtimeKind"], "container");
        assert_eq!(value["currentTool"], "Edit");
        assert_eq!(value["cliTool"], "claude");
        assert_eq!(value["gitBranch"], "+3 -1");
        assert_eq!(value["tokens"]["current"], 1234);
        assert_eq!(value["tokens"]["cumulative"], 56789);
        assert_eq!(value["eventsCount"], 42);
    }

    /// Build a DB user entity for projection tests.
    fn db_user(email: &str, display_name: Option<&str>, is_admin: bool) -> agentforge_db::entities::User {
        use chrono::TimeZone;
        agentforge_db::entities::User {
            id: Uuid::nil().into(),
            email: email.to_string(),
            password_hash: None,
            display_name: display_name.map(str::to_string),
            is_admin,
            last_login_at: Some(Utc.timestamp_millis_opt(1_700_000_200_000).unwrap()),
            created_at: Utc.timestamp_millis_opt(1_700_000_000_000).unwrap(),
            updated_at: Utc.timestamp_millis_opt(1_700_000_100_000).unwrap(),
            deleted_at: None,
        }
    }

    #[test]
    fn admin_user_projection_serializes_camel_case_contract() {
        let value =
            serde_json::to_value(AdminUserProjection::from(db_user("alice@example.com", Some("Alice"), true))).unwrap();

        assert_eq!(value["id"], json!(Uuid::nil()));
        assert_eq!(value["email"], "alice@example.com");
        assert_eq!(value["displayName"], "Alice");
        assert_eq!(value["role"], "admin");
        assert_eq!(value["status"], "active");
        assert!(value["createdAt"].as_str().expect("createdAt RFC3339 string").starts_with("2023-11-14T"));
        assert!(value["lastLoginAt"].as_str().expect("lastLoginAt RFC3339 string").starts_with("2023-11-14T"));
        // snake_case keys must not leak.
        assert!(value.get("display_name").is_none());
        assert!(value.get("created_at").is_none());
        assert!(value.get("last_login_at").is_none());
    }

    #[test]
    fn admin_user_projection_falls_back_to_email_local_part() {
        for missing in [None, Some(""), Some("   ")] {
            let projection = AdminUserProjection::from(db_user("bob@example.com", missing, false));
            assert_eq!(projection.display_name, "bob", "display_name {missing:?} must fall back to email local part");
        }
        let kept = AdminUserProjection::from(db_user("bob@example.com", Some("Bob B."), false));
        assert_eq!(kept.display_name, "Bob B.");
    }

    #[test]
    fn admin_user_projection_derives_role_from_is_admin() {
        assert_eq!(AdminUserProjection::from(db_user("a@example.com", None, true)).role, "admin");
        assert_eq!(AdminUserProjection::from(db_user("b@example.com", None, false)).role, "member");
    }

    #[test]
    fn admin_user_list_projection_computes_total_pages() {
        let page = AdminUserListProjection::new(vec![], 51, 2, 25);
        assert_eq!(page.total_pages, 3);
        assert_eq!(AdminUserListProjection::new(vec![], 50, 1, 25).total_pages, 2);
        assert_eq!(AdminUserListProjection::new(vec![], 0, 1, 25).total_pages, 0);
        assert_eq!(AdminUserListProjection::new(vec![], 10, 1, 0).total_pages, 0);
    }

    #[test]
    fn admin_user_list_response_uses_camel_case_keys() {
        let response = admin_user_list_response(AdminUserListProjection::new(
            vec![AdminUserProjection::from(db_user("alice@example.com", Some("Alice"), true))],
            26,
            2,
            25,
        ));

        assert_eq!(response["ok"], true);
        assert_eq!(response["total"], 26);
        assert_eq!(response["page"], 2);
        assert_eq!(response["limit"], 25);
        assert_eq!(response["totalPages"], 2);
        assert_eq!(response["users"][0]["displayName"], "Alice");
        assert_eq!(response["users"][0]["role"], "admin");
    }

    #[test]
    fn admin_org_projection_serializes_camel_case_contract() {
        use chrono::TimeZone;
        let value = serde_json::to_value(AdminOrgProjection {
            id: Uuid::nil(),
            name: "Acme".into(),
            slug: "acme".into(),
            created_at: Utc.timestamp_millis_opt(1_700_000_000_000).unwrap(),
            members_count: 6,
            teams_count: 2,
        })
        .unwrap();

        assert_eq!(value["name"], "Acme");
        assert_eq!(value["slug"], "acme");
        assert_eq!(value["membersCount"], 6);
        assert_eq!(value["teamsCount"], 2);
        assert!(value["createdAt"].as_str().expect("createdAt RFC3339 string").starts_with("2023-11-14T"));
        // The organizations table has no plan column — the contract must not invent one.
        assert!(value.get("plan").is_none());
        assert!(value.get("members_count").is_none());
    }

    #[test]
    fn admin_org_list_response_wraps_organizations_and_total() {
        use chrono::TimeZone;
        let response = admin_org_list_response(
            vec![AdminOrgProjection {
                id: Uuid::nil(),
                name: "Acme".into(),
                slug: "acme".into(),
                created_at: Utc.timestamp_millis_opt(1_700_000_000_000).unwrap(),
                members_count: 1,
                teams_count: 0,
            }],
            7,
        );

        assert_eq!(response["ok"], true);
        assert_eq!(response["total"], 7);
        assert_eq!(response["organizations"][0]["slug"], "acme");
        assert_eq!(response["organizations"][0]["teamsCount"], 0);
    }

    #[test]
    fn admin_agent_detail_response_adds_detail_fields() {
        let event_id = Uuid::now_v7();
        let response = admin_agent_detail_response(AdminAgentDetailProjection {
            agent: AdminAgentProjection {
                id: Uuid::nil(),
                name: String::new(),
                status: AgentStatus::Idle,
                cwd: String::new(),
                current_tool: None,
                cli_tool: None,
                tokens: AdminAgentTokens::new(0, 0),
                git_branch: None,
                owner_username: None,
                owner_email: None,
                project_name: None,
                created_at: 1,
                last_activity: 2,
                runtime_id: String::new(),
                runtime_kind: RuntimeKind::Api,
                container_id: None,
                events_count: 1,
            },
            user_id: Uuid::nil(),
            organization_id: Uuid::nil(),
            project_id: None,
            cli_session_id: Some("session-1".into()),
            recent_events: vec![AdminAgentEventProjection {
                id: event_id,
                event_type: "tool_call".into(),
                tool_name: None,
                created_at: 3,
            }],
        });

        assert_eq!(response["ok"], true);
        assert_eq!(response["agent"]["userId"], json!(Uuid::nil()));
        assert_eq!(response["agent"]["orgId"], json!(Uuid::nil()));
        assert_eq!(response["agent"]["cliSessionId"], "session-1");
        assert!(response["agent"]["gitStatus"].is_null());
        assert_eq!(response["agent"]["recentEvents"][0]["id"], json!(event_id));
        assert_eq!(response["agent"]["recentEvents"][0]["type"], "tool_call");
    }
}
