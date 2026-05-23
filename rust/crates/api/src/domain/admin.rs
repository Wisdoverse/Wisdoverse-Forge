//! Admin domain rules.
//!
//! This module owns pure admin-console policies that are independent of
//! repositories and HTTP route DTOs.

use agentforge_core::{AgentStatus, AppError, AppResult, ErrorKind};
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

pub(crate) fn admin_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
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
    pub page: i64,
    pub limit: i64,
    pub sort_by: Option<&'a str>,
    pub sort_order: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdminAgentFilterDecision {
    pub search: Option<String>,
    pub status: Option<AgentStatus>,
    pub page: i64,
    pub limit: i64,
    pub offset: i64,
    pub sort_by: AdminAgentSort,
    pub sort_order: SortOrder,
}

/// Admin agent list parsing and pagination policy.
pub(crate) struct AdminAgentFilterPolicy;

impl AdminAgentFilterPolicy {
    pub(crate) fn from_query(query: AdminAgentFilterQuery<'_>) -> AdminAgentFilterDecision {
        let page = query.page.max(1);
        let limit = query.limit.clamp(1, 100);
        AdminAgentFilterDecision {
            search: query.search.and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }),
            status: Self::parse_status_filter(query.status),
            page,
            limit,
            offset: (page - 1) * limit,
            sort_by: Self::parse_sort_by(query.sort_by),
            sort_order: Self::parse_sort_order(query.sort_order),
        }
    }

    fn parse_status_filter(raw: Option<&str>) -> Option<AgentStatus> {
        match raw?.to_ascii_lowercase().as_str() {
            "working" => Some(AgentStatus::Working),
            "idle" => Some(AgentStatus::Idle),
            "offline" => Some(AgentStatus::Offline),
            _ => None,
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
            _ => Err(ErrorKind::Forbidden.into()),
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
            ErrorKind::Unprocessable(msg) => format!("unprocessable entity: {msg}"),
            ErrorKind::Conflict(msg) => format!("conflict: {msg}"),
            ErrorKind::Unauthorized => "unauthorized".to_string(),
            ErrorKind::Forbidden => "forbidden".to_string(),
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

    #[test]
    fn admin_agent_filter_accepts_supported_statuses() {
        assert_eq!(
            AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
                search: None,
                status: Some("working"),
                page: 1,
                limit: 25,
                sort_by: None,
                sort_order: None,
            })
            .status,
            Some(AgentStatus::Working)
        );
        assert_eq!(
            AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
                search: None,
                status: Some("WORKING"),
                page: 1,
                limit: 25,
                sort_by: None,
                sort_order: None,
            })
            .status,
            Some(AgentStatus::Working)
        );
    }

    #[test]
    fn admin_agent_filter_drops_unsupported_statuses() {
        for status in [Some("waiting"), Some("attention"), Some("bogus"), None] {
            let decision = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
                search: None,
                status,
                page: 1,
                limit: 25,
                sort_by: None,
                sort_order: None,
            });
            assert_eq!(decision.status, None);
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
                page: 1,
                limit: 25,
                sort_by,
                sort_order: Some("ASC"),
            });
            assert_eq!(decision.sort_by, expected);
            assert_eq!(decision.sort_order, SortOrder::Asc);
        }
    }

    #[test]
    fn admin_agent_filter_paginates_clamps_and_drops_blank_search() {
        let decision = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
            search: Some("  "),
            status: None,
            page: 4,
            limit: 10,
            sort_by: Some("name"),
            sort_order: Some("asc"),
        });

        assert_eq!(decision.search, None);
        assert_eq!(decision.page, 4);
        assert_eq!(decision.limit, 10);
        assert_eq!(decision.offset, 30);
        assert_eq!(decision.sort_by, AdminAgentSort::Name);
        assert_eq!(decision.sort_order, SortOrder::Asc);

        let clamped = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
            search: Some(" user@example.com "),
            status: None,
            page: -4,
            limit: 500,
            sort_by: None,
            sort_order: Some("nope"),
        });
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
        assert_eq!(value["currentTool"], "Edit");
        assert_eq!(value["cliTool"], "claude");
        assert_eq!(value["gitBranch"], "+3 -1");
        assert_eq!(value["tokens"]["current"], 1234);
        assert_eq!(value["tokens"]["cumulative"], 56789);
        assert_eq!(value["eventsCount"], 42);
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
