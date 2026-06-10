//! Admin service — business logic for admin-only operations.

use std::sync::Arc;

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::ImpersonationLog;
use sqlx::PgPool;
use uuid::Uuid;

pub use crate::domain::admin::BulkDeleteResult;
use crate::domain::admin::{
    AdminAgentDetailProjection, AdminAgentEventProjection, AdminAgentFilterPolicy, AdminAgentFilterQuery,
    AdminAgentListProjection, AdminAgentProjection, AdminAgentTokens, AdminBulkDeletePolicy, AdminImpersonationPolicy,
    AdminListPage, AdminOrgProjection, AdminRolePolicy, AdminUserListProjection, AdminUserProjection,
};
pub(crate) use crate::domain::admin::{
    admin_agent_detail_response, admin_agent_list_response, admin_bulk_delete_response, admin_data_response,
    admin_delete_response, admin_org_list_response, admin_user_list_response,
};
use crate::repositories::admin::{
    AdminAgentEventRow, AdminAgentFilters, AdminAgentRow, AdminOrgRow, AdminRepository, AdminStats,
};
use crate::services::auth_callout::AuthCalloutService;

/// Service input for the admin agent list endpoint. This is intentionally
/// independent of the HTTP query DTO so the route only performs extraction.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AdminAgentListInput<'a> {
    pub(crate) search: Option<&'a str>,
    pub(crate) status: Option<&'a str>,
    pub(crate) runtime_kind: Option<&'a str>,
    pub(crate) user_id: Option<Uuid>,
    pub(crate) project_id: Option<Uuid>,
    pub(crate) page: i64,
    pub(crate) limit: i64,
    pub(crate) sort_by: Option<&'a str>,
    pub(crate) sort_order: Option<&'a str>,
}

impl From<AdminAgentRow> for AdminAgentProjection {
    fn from(row: AdminAgentRow) -> Self {
        Self {
            id: row.id,
            name: row.name.unwrap_or_default(),
            status: row.status,
            cwd: row.cwd.unwrap_or_default(),
            current_tool: row.current_tool,
            cli_tool: row.cli_tool,
            tokens: AdminAgentTokens::new(row.tokens_current, row.tokens_cumulative),
            git_branch: row.git_status,
            owner_username: row.owner_username,
            owner_email: row.owner_email,
            project_name: row.project_name,
            created_at: row.created_at.timestamp_millis(),
            last_activity: row.last_activity.timestamp_millis(),
            runtime_id: row.runtime_id.unwrap_or_default(),
            runtime_kind: row.runtime_kind,
            container_id: row.container_id,
            events_count: row.events_count,
        }
    }
}

impl From<AdminAgentEventRow> for AdminAgentEventProjection {
    fn from(row: AdminAgentEventRow) -> Self {
        Self { id: row.id, event_type: row.event_type, tool_name: None, created_at: row.created_at.timestamp_millis() }
    }
}

impl From<AdminOrgRow> for AdminOrgProjection {
    fn from(row: AdminOrgRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            slug: row.slug,
            created_at: row.created_at,
            members_count: row.members_count,
            teams_count: row.teams_count,
        }
    }
}

/// Business logic layer for admin operations.
pub struct AdminService {
    repo: AdminRepository,
    auth_callout: Option<Arc<AuthCalloutService>>,
}

impl AdminService {
    pub fn new(repo: AdminRepository) -> Self {
        Self { repo, auth_callout: None }
    }

    pub fn from_runtime(pool: PgPool, auth_callout: Option<Arc<AuthCalloutService>>) -> Self {
        Self::new(AdminRepository::new(pool)).with_auth_callout(auth_callout)
    }

    pub(crate) fn with_auth_callout(mut self, auth_callout: Option<Arc<AuthCalloutService>>) -> Self {
        self.auth_callout = auth_callout;
        self
    }

    /// List users as the admin-console paginated projection (admin only).
    /// `page` is 1-based with a floor of 1; the limit is clamped to 1..=100 by
    /// [`AdminListPage`].
    pub(crate) async fn list_user_page(
        &self,
        page: i64,
        limit: i64,
        search: Option<&str>,
    ) -> AppResult<AdminUserListProjection> {
        let page = page.max(1);
        let list_page = AdminListPage::new(limit, (page - 1).saturating_mul(limit));
        let users = self.repo.list_all_users(list_page.limit(), list_page.offset(), search).await?;
        let total = self.repo.count_users(search).await?;
        let users = users.into_iter().map(AdminUserProjection::from).collect();
        Ok(AdminUserListProjection::new(users, total, page, list_page.limit()))
    }

    /// List organizations with member/team counts plus the total org count
    /// (admin only). Limit capped at 100.
    pub(crate) async fn list_org_page(&self, limit: i64, offset: i64) -> AppResult<(Vec<AdminOrgProjection>, i64)> {
        let page = AdminListPage::new(limit, offset);
        let orgs = self.repo.list_all_organizations_with_counts(page.limit(), page.offset()).await?;
        let total = self.repo.count_organizations().await?;
        Ok((orgs.into_iter().map(AdminOrgProjection::from).collect(), total))
    }

    /// Start impersonation of a target user.
    pub async fn start_impersonation(
        &self,
        scope: &TenantScope,
        target_user_id: Uuid,
        reason: Option<&str>,
    ) -> AppResult<ImpersonationLog> {
        AdminImpersonationPolicy::ensure_not_self(scope.user_id().as_uuid(), target_user_id)?;
        self.repo.start_impersonation(scope, target_user_id, reason).await
    }

    /// End the current impersonation session.
    pub async fn end_impersonation(&self, scope: &TenantScope) -> AppResult<ImpersonationLog> {
        self.repo.end_impersonation(scope).await
    }

    /// List impersonation log (admin only). Limit capped at 100.
    pub async fn list_impersonation_log(
        &self,
        scope: &TenantScope,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<ImpersonationLog>> {
        let page = AdminListPage::new(limit, offset);
        self.repo.list_impersonation_log(scope, page.limit(), page.offset()).await
    }

    /// Get system-wide statistics.
    pub async fn stats(&self) -> AppResult<AdminStats> {
        self.repo.stats().await
    }

    /// List agents across every organization for the admin dashboard. Applies
    /// the same limit clamping as other admin list endpoints.
    pub async fn list_agents(&self, mut filters: AdminAgentFilters) -> AppResult<(Vec<AdminAgentRow>, i64)> {
        let page = AdminListPage::new(filters.limit, filters.offset);
        filters.limit = page.limit();
        filters.offset = page.offset();
        self.repo.list_agents(&filters).await
    }

    /// List agents as the admin-console response projection.
    pub(crate) async fn list_agent_page(&self, input: AdminAgentListInput<'_>) -> AppResult<AdminAgentListProjection> {
        let (filters, page) = filters_from_agent_list_input(input)?;
        let limit = filters.limit;
        let (rows, total) = self.list_agents(filters).await?;
        let agents = rows.into_iter().map(AdminAgentProjection::from).collect();
        Ok(AdminAgentListProjection::new(agents, total, page, limit))
    }

    /// Fetch a single agent by ID (admin only) along with its most recent events.
    /// Returns `(row, recent_events)`; callers assemble the final JSON response.
    pub async fn get_agent(&self, agent_id: Uuid) -> AppResult<(AdminAgentRow, Vec<AdminAgentEventRow>)> {
        let row = self.repo.find_agent_by_id(agent_id).await?;
        let events = self.repo.recent_events_for_agent(agent_id, 20).await?;
        Ok((row, events))
    }

    /// Fetch a single agent as the admin-console detail response projection.
    pub(crate) async fn get_agent_detail(&self, agent_id: Uuid) -> AppResult<AdminAgentDetailProjection> {
        let (row, events) = self.get_agent(agent_id).await?;
        Ok(AdminAgentDetailProjection {
            agent: row.clone().into(),
            user_id: row.user_id,
            organization_id: row.organization_id,
            project_id: row.project_id,
            cli_session_id: row.cli_session_id,
            recent_events: events.into_iter().map(AdminAgentEventProjection::from).collect(),
        })
    }

    /// Hard-delete a single agent (admin only).
    pub async fn delete_agent(&self, agent_id: Uuid) -> AppResult<()> {
        self.revoke_agent_connection(agent_id, "admin delete_agent").await;
        self.repo.delete_agent(agent_id).await
    }

    pub async fn bulk_delete_agents_checked(&self, agent_ids: &[Uuid]) -> AppResult<Vec<BulkDeleteResult>> {
        AdminBulkDeletePolicy::require_ids(agent_ids)?;
        Ok(self.bulk_delete_agents(agent_ids).await)
    }

    /// Delete multiple agents, collecting per-ID success/failure results so the
    /// frontend can show which IDs were handled. Error messages are derived
    /// from `ErrorKind` (which implements `Display`) to avoid leaking internal
    /// error details in the response.
    pub async fn bulk_delete_agents(&self, agent_ids: &[Uuid]) -> Vec<BulkDeleteResult> {
        self.revoke_agent_connections(agent_ids, "admin bulk_delete_agents").await;
        let mut results = Vec::with_capacity(agent_ids.len());
        for id in agent_ids {
            match self.repo.delete_agent(*id).await {
                Ok(()) => results.push(BulkDeleteResult { id: *id, ok: true, error: None }),
                Err(err) => results.push(BulkDeleteResult {
                    id: *id,
                    ok: false,
                    error: Some(AdminBulkDeletePolicy::error_message(&err)),
                }),
            }
        }
        results
    }

    async fn revoke_agent_connections(&self, agent_ids: &[Uuid], operation: &'static str) {
        match self.auth_callout.as_ref() {
            Some(callout) => {
                for id in agent_ids {
                    callout.revoke(*id).await;
                }
            }
            None => tracing::info!(
                count = agent_ids.len(),
                operation,
                "auth callout disabled — revocation falls back to JWT TTL"
            ),
        }
    }

    async fn revoke_agent_connection(&self, agent_id: Uuid, operation: &'static str) {
        self.revoke_agent_connections(&[agent_id], operation).await;
    }

    /// Check if the user has admin privileges. Returns an error if not.
    pub fn require_admin(auth_role: &str) -> AppResult<()> {
        AdminRolePolicy::require_admin(auth_role)
    }
}

/// Build repository filters from the service-level admin list input.
///
/// Returns an error (HTTP 422 via `ErrorKind::Unprocessable`) when the caller
/// supplies an unknown `runtimeKind` value.
fn filters_from_agent_list_input(input: AdminAgentListInput<'_>) -> AppResult<(AdminAgentFilters, i64)> {
    let decision = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
        search: input.search,
        status: input.status,
        runtime_kind: input.runtime_kind,
        page: input.page,
        limit: input.limit,
        sort_by: input.sort_by,
        sort_order: input.sort_order,
    })?;

    Ok((
        AdminAgentFilters {
            search: decision.search,
            status: decision.status,
            runtime_kind: decision.runtime_kind,
            user_id: input.user_id,
            project_id: input.project_id,
            sort_by: decision.sort_by,
            sort_order: decision.sort_order,
            limit: decision.limit,
            offset: decision.offset,
        },
        decision.page,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::admin::{AdminAgentSort, SortOrder};
    use agentforge_core::{AgentStatus, ErrorKind, RuntimeKind};

    #[test]
    fn admin_role_check_owner() {
        assert!(AdminService::require_admin("owner").is_ok());
    }

    #[test]
    fn admin_role_check_admin() {
        assert!(AdminService::require_admin("admin").is_ok());
    }

    #[test]
    fn admin_role_check_member_rejected() {
        assert!(AdminService::require_admin("member").is_err());
    }

    #[test]
    fn admin_role_check_viewer_rejected() {
        assert!(AdminService::require_admin("viewer").is_err());
    }

    #[test]
    fn admin_role_check_empty_rejected() {
        assert!(AdminService::require_admin("").is_err());
    }

    #[test]
    fn admin_agent_list_input_paginates_and_clamps() {
        let (filters, page) = filters_from_agent_list_input(AdminAgentListInput {
            search: Some("  "),
            status: None,
            runtime_kind: None,
            user_id: None,
            project_id: None,
            page: 4,
            limit: 10,
            sort_by: Some("name"),
            sort_order: Some("asc"),
        })
        .unwrap();

        assert_eq!(page, 4);
        assert_eq!(filters.limit, 10);
        assert_eq!(filters.offset, 30);
        assert!(filters.search.is_none());
        assert!(filters.runtime_kind.is_none());
        assert_eq!(filters.sort_by, AdminAgentSort::Name);
        assert_eq!(filters.sort_order, SortOrder::Asc);

        let (filters, page) = filters_from_agent_list_input(AdminAgentListInput {
            search: Some(" user@example.com "),
            status: Some("WORKING"),
            runtime_kind: Some("cli"),
            user_id: None,
            project_id: None,
            page: 0,
            limit: 500,
            sort_by: None,
            sort_order: Some("nope"),
        })
        .unwrap();

        assert_eq!(page, 1);
        assert_eq!(filters.limit, 100);
        assert_eq!(filters.offset, 0);
        assert_eq!(filters.search.as_deref(), Some("user@example.com"));
        assert_eq!(filters.status, Some(AgentStatus::Working));
        assert_eq!(filters.runtime_kind, Some(RuntimeKind::Cli));
        assert_eq!(filters.sort_order, SortOrder::Desc);
    }

    #[test]
    fn admin_agent_list_input_rejects_unknown_runtime_kind() {
        let err = filters_from_agent_list_input(AdminAgentListInput {
            search: None,
            status: None,
            runtime_kind: Some("host_cli"),
            user_id: None,
            project_id: None,
            page: 1,
            limit: 25,
            sort_by: None,
            sort_order: None,
        })
        .expect_err("unknown runtimeKind must surface as an error");
        assert!(matches!(err.kind, ErrorKind::Unprocessable(_)), "expected 422 Unprocessable, got {err:?}");
    }

    /// Seed `count` users so pagination math has data to work against.
    async fn seed_users(pool: &PgPool, count: usize) {
        for index in 0..count {
            sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, $3)")
                .bind(Uuid::new_v4())
                .bind(format!("user-{index}@example.com"))
                .bind(format!("User {index}"))
                .execute(pool)
                .await
                .expect("seed user");
        }
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn list_user_page_returns_correct_total_pages(pool: PgPool) {
        let service = AdminService::new(AdminRepository::new(pool.clone()));
        seed_users(&pool, 5).await;

        let page = service.list_user_page(1, 2, None).await.expect("first page");
        assert_eq!(page.total, 5);
        assert_eq!(page.page, 1);
        assert_eq!(page.limit, 2);
        assert_eq!(page.total_pages, 3, "5 users at limit 2 span 3 pages");
        assert_eq!(page.users.len(), 2);

        let last = service.list_user_page(3, 2, None).await.expect("last page");
        assert_eq!(last.page, 3);
        assert_eq!(last.users.len(), 1, "last page carries the remainder");

        // Page floor: page 0 behaves like page 1 instead of a negative offset.
        let floored = service.list_user_page(0, 2, None).await.expect("floored page");
        assert_eq!(floored.page, 1);
        assert_eq!(floored.users.len(), 2);

        // Search threads through to the projection and its pagination metadata.
        let searched = service.list_user_page(1, 2, Some("user-3")).await.expect("searched page");
        assert_eq!(searched.total, 1);
        assert_eq!(searched.total_pages, 1);
        assert_eq!(searched.users[0].email, "user-3@example.com");
        assert_eq!(searched.users[0].display_name, "User 3");
        assert_eq!(searched.users[0].role, "member");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn list_org_page_projects_counts_and_total(pool: PgPool) {
        let service = AdminService::new(AdminRepository::new(pool.clone()));
        let org_id = Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Acme', $2)")
            .bind(org_id)
            .bind(format!("org-{org_id}"))
            .execute(&pool)
            .await
            .expect("seed organization");

        let (orgs, total) = service.list_org_page(50, 0).await.expect("org page");
        assert_eq!(total, 1);
        assert_eq!(orgs.len(), 1);
        assert_eq!(orgs[0].name, "Acme");
        assert_eq!(orgs[0].members_count, 0);
        assert_eq!(orgs[0].teams_count, 0);
    }

    #[test]
    fn admin_agent_row_projection_uses_camel_case_and_epoch_ms() {
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
            runtime_kind: RuntimeKind::Container,
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
        let value = serde_json::to_value(AdminAgentProjection::from(row)).unwrap();

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
}
