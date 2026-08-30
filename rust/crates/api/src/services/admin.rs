//! Admin service — business logic for admin-only operations.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::{ImpersonationLog, User};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::admin::{
    ADMIN_USER_AUDIT_RESOURCE, ADMIN_USER_DELETED_ACTION, ADMIN_USER_ROLE_UPDATED_ACTION, AdminAgentDetailProjection,
    AdminAgentEventProjection, AdminAgentFilterPolicy, AdminAgentFilterQuery, AdminAgentListProjection,
    AdminAgentProjection, AdminAgentTokens, AdminImpersonationPolicy, AdminListPage, AdminOrgProjection,
    AdminRoleChange, AdminRolePolicy, AdminUserListProjection, AdminUserModificationPolicy, AdminUserProjection,
    DeadEventPage, DeadEventRow, OrgControlPlaneSnapshot, admin_role_label, admin_user_deleted_audit_details,
    admin_user_role_audit_details,
};
pub(crate) use crate::domain::admin::{
    admin_agent_detail_response, admin_agent_list_response, admin_bulk_delete_response, admin_data_response,
    admin_delete_response, admin_org_list_response, admin_user_list_response, admin_user_role_response,
};
use crate::repositories::admin::{
    AdminAgentEventRow, AdminAgentFilters, AdminAgentRow, AdminOrgRow, AdminRepository, AdminStats, DeadEventRecord,
};
use crate::repositories::audit::AuditRepository;

/// Persistence adapter: project a stored `User` row onto the admin-console
/// projection. Lives in the service layer so `domain/admin.rs` stays free of
/// `agentforge_db` (DDD-2); `AdminUserProjection` itself stays a pure domain type.
impl From<User> for AdminUserProjection {
    fn from(user: User) -> Self {
        let display_name = match user.display_name.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() => name.to_string(),
            // Fall back to the local part of the email so the admin table
            // never renders a blank "Person" cell.
            _ => user.email.split('@').next().unwrap_or(user.email.as_str()).to_string(),
        };
        Self {
            id: user.id.as_uuid(),
            display_name,
            role: admin_role_label(user.is_admin).to_string(),
            status: "active".to_string(),
            created_at: user.created_at,
            last_login_at: user.last_login_at,
            email: user.email,
        }
    }
}

/// Persistence adapter: project a stored `dead_events` row onto the domain wire
/// shape. Lives in the service layer so `domain/admin.rs` stays free of any
/// SQLx row coupling.
impl From<DeadEventRecord> for DeadEventRow {
    fn from(row: DeadEventRecord) -> Self {
        Self {
            id: row.id,
            source: row.source,
            reason: row.reason,
            subject: row.subject,
            detail: row.detail,
            delivery_id: row.delivery_id,
            org_id: row.org_id,
            payload_excerpt: row.payload_excerpt,
            recorded_at: row.recorded_at,
        }
    }
}

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

/// Unforgeable proof that `users.is_admin` passed in this service.
#[derive(Debug)]
pub(crate) struct PlatformAdminAuthority(());

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
    /// User-management actions (role change, removal) write an audit trail.
    /// Wired here directly — there is no shared service-layer audit facade yet;
    /// `ContextGovernanceService::emit_audit` uses the same repository-direct
    /// pattern.
    audit: AuditRepository,
}

impl AdminService {
    pub fn new(repo: AdminRepository, audit: AuditRepository) -> Self {
        Self { repo, audit }
    }

    pub fn from_runtime(pool: PgPool) -> Self {
        Self::new(AdminRepository::new(pool.clone()), AuditRepository::new(pool))
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

    /// Change a user's access level (admin only). `role` is the wire value
    /// `"admin" | "member"` and is written to BOTH `users.is_admin` (the
    /// console's display/accounting source) and the target's
    /// `organization_members.role` in this organization — the JWT `role`
    /// claim is minted from the membership row at login/context-switch, so
    /// without the dual write a "promotion" would never grant real access.
    /// The new claim takes effect on the target's next sign-in or token
    /// refresh. Cross-org memberships are untouched (future multi-tenant
    /// work).
    ///
    /// Guards: admins cannot change their own account, organization owners
    /// are not managed here, and the last remaining admin can never be
    /// demoted. Setting a role the user already has is an idempotent
    /// success. Each applied change writes an audit entry.
    pub(crate) async fn update_user_role(
        &self,
        scope: &TenantScope,
        target_user_id: Uuid,
        role: &str,
    ) -> AppResult<AdminUserProjection> {
        let make_admin = AdminRoleChange::parse(role)?;
        AdminUserModificationPolicy::ensure_not_self(scope.user_id().as_uuid(), target_user_id)?;

        let target = self.repo.find_user_by_id(target_user_id).await?;
        if self.repo.member_role(scope.org_id().as_uuid(), target_user_id).await?.as_deref() == Some("owner") {
            return Err(crate::domain::admin::owner_managed_error());
        }
        // Only a demotion of a CURRENT admin can orphan the deployment; a
        // member→member no-op or any promotion never trips the guard.
        if target.is_admin && !make_admin {
            AdminUserModificationPolicy::ensure_another_admin_remains(self.repo.count_active_admins().await?)?;
        }

        let old_role = admin_role_label(target.is_admin);
        let updated = self.repo.set_user_admin(target_user_id, make_admin).await?;
        self.repo.set_member_role(scope.org_id().as_uuid(), target_user_id, admin_role_label(make_admin)).await?;
        let projection = AdminUserProjection::from(updated);

        self.audit
            .create(
                scope.org_id(),
                Some(scope.user_id()),
                ADMIN_USER_ROLE_UPDATED_ACTION,
                ADMIN_USER_AUDIT_RESOURCE,
                Some(target_user_id),
                &admin_user_role_audit_details(&projection.email, old_role, admin_role_label(make_admin)),
                None,
            )
            .await?;

        Ok(projection)
    }

    /// Remove (soft-delete) a user account (admin only). The user disappears
    /// from the admin list and can no longer sign in; history rows stay.
    ///
    /// Guards: admins cannot delete their own account, organization owners
    /// are not removable here, and the last remaining admin can never be
    /// deleted. Each removal writes an audit entry.
    pub(crate) async fn delete_user(&self, scope: &TenantScope, target_user_id: Uuid) -> AppResult<()> {
        AdminUserModificationPolicy::ensure_not_self(scope.user_id().as_uuid(), target_user_id)?;

        let target = self.repo.find_user_by_id(target_user_id).await?;
        if self.repo.member_role(scope.org_id().as_uuid(), target_user_id).await?.as_deref() == Some("owner") {
            return Err(crate::domain::admin::owner_managed_error());
        }
        // Deleting a non-admin can never orphan the deployment.
        if target.is_admin {
            AdminUserModificationPolicy::ensure_another_admin_remains(self.repo.count_active_admins().await?)?;
        }

        self.repo.soft_delete_user(target_user_id).await?;

        self.audit
            .create(
                scope.org_id(),
                Some(scope.user_id()),
                ADMIN_USER_DELETED_ACTION,
                ADMIN_USER_AUDIT_RESOURCE,
                Some(target_user_id),
                &admin_user_deleted_audit_details(&target.email, admin_role_label(target.is_admin)),
                None,
            )
            .await?;

        Ok(())
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

    /// Org-scoped orchestration control-plane snapshot for the admin health panel.
    /// `stale_after_secs` is the participant-staleness threshold (caller supplies
    /// it so the route stays free of config reads).
    pub(crate) async fn org_control_plane_snapshot(
        &self,
        scope: &TenantScope,
        stale_after_secs: i64,
    ) -> AppResult<OrgControlPlaneSnapshot> {
        self.repo.org_control_plane_snapshot(scope, stale_after_secs).await
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

    /// Check if the user has admin privileges. Returns an error if not.
    pub fn require_admin(auth_role: &str) -> AppResult<()> {
        AdminRolePolicy::require_admin(auth_role)
    }

    /// Platform-admin-only gate, keyed off the server-side `users.is_admin`
    /// column (NOT the per-org membership role). Loads the caller's user row and
    /// defers the verdict to the domain policy so the service owns the I/O and
    /// the domain owns the error contract. Used for cross-org surfaces such as
    /// the dead-letter reader.
    pub(crate) async fn require_platform_admin(&self, user_id: Uuid) -> AppResult<PlatformAdminAuthority> {
        let user = self.repo.find_user_by_id(user_id).await?;
        AdminRolePolicy::require_platform_admin(user.is_admin)?;
        Ok(PlatformAdminAuthority(()))
    }

    /// Org-admin gate keyed on the LIVE per-org membership role
    /// (`organization_members.role`), NOT the self-issued JWT `role` claim. Used
    /// for org-scoped privileged actions (feature flags, governance audit reads,
    /// task approval) so a demoted member loses access immediately instead of at
    /// token expiry. A non-member (`None`) is rejected like a non-admin.
    pub async fn require_org_admin(&self, org_id: Uuid, user_id: Uuid) -> AppResult<()> {
        let role = self.repo.member_role(org_id, user_id).await?;
        AdminRolePolicy::require_admin(role.as_deref().unwrap_or(""))
    }

    /// List captured dead-letter rows as a paginated, cross-org projection
    /// (platform admin only — enforced at the route). `page` is 1-based with a
    /// floor of 1; `limit` is clamped to 1..=100 by [`AdminListPage`]. An optional
    /// `reason` filters exactly.
    pub(crate) async fn list_dead_events(
        &self,
        page: i64,
        limit: i64,
        reason: Option<&str>,
    ) -> AppResult<DeadEventPage> {
        let page = page.max(1);
        let list_page = AdminListPage::new(limit, (page - 1).saturating_mul(limit));
        let records = self.repo.list_dead_events(list_page.limit(), list_page.offset(), reason).await?;
        let items = records.into_iter().map(DeadEventRow::from).collect();
        let total = self.repo.count_dead_events(reason).await?;
        Ok(DeadEventPage::new(items, total, page, list_page.limit()))
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

    /// Build an [`AdminService`] with real repositories over the test pool.
    fn admin_service(pool: &PgPool) -> AdminService {
        AdminService::new(AdminRepository::new(pool.clone()), AuditRepository::new(pool.clone()))
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
        let service = admin_service(&pool);
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
        let service = admin_service(&pool);
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

    /// #889: `require_org_admin` keys off the LIVE `organization_members.role`,
    /// not the JWT claim. owner/admin pass; member/viewer/non-member are refused.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn require_org_admin_uses_live_membership_role(pool: PgPool) {
        let service = admin_service(&pool);
        let org = seed_org(&pool).await;
        for (role, allowed) in [("owner", true), ("admin", true), ("member", false), ("viewer", false)] {
            let user = seed_user(&pool, &format!("{role}-{}@example.com", Uuid::new_v4()), false).await;
            sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
                .bind(org)
                .bind(user)
                .bind(role)
                .execute(&pool)
                .await
                .expect("seed membership");
            let result = service.require_org_admin(org, user).await;
            if allowed {
                assert!(result.is_ok(), "{role} must pass the org-admin gate");
            } else {
                assert!(
                    matches!(result.expect_err("refused").kind, ErrorKind::Forbidden(_)),
                    "{role} must be Forbidden by the org-admin gate"
                );
            }
        }
        // A user with no membership row in this org is refused like a non-admin.
        let outsider = seed_user(&pool, &format!("outsider-{}@example.com", Uuid::new_v4()), false).await;
        assert!(matches!(
            service.require_org_admin(org, outsider).await.expect_err("non-member refused").kind,
            ErrorKind::Forbidden(_)
        ));
    }

    // ========================================================================
    // User role management + removal
    // ========================================================================

    /// Seed one organization row (audit entries FK onto it).
    async fn seed_org(pool: &PgPool) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Admin Org', $2)")
            .bind(id)
            .bind(format!("org-{id}"))
            .execute(pool)
            .await
            .expect("seed organization");
        id
    }

    /// Seed one live user with an explicit global admin flag.
    async fn seed_user(pool: &PgPool, email: &str, is_admin: bool) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, email, is_admin) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(email)
            .bind(is_admin)
            .execute(pool)
            .await
            .expect("seed user");
        id
    }

    /// Seed an org membership with an explicit role.
    async fn seed_membership(pool: &PgPool, org_id: Uuid, user_id: Uuid, role: &str) {
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)")
            .bind(org_id)
            .bind(user_id)
            .bind(role)
            .execute(pool)
            .await
            .expect("seed membership");
    }

    /// The membership role for (org, user), if any.
    async fn membership_role(pool: &PgPool, org_id: Uuid, user_id: Uuid) -> Option<String> {
        sqlx::query_scalar("SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2")
            .bind(org_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .expect("read membership role")
    }

    /// Tenant scope for the acting admin-console operator.
    fn scope_for(org_id: Uuid, user_id: Uuid) -> TenantScope {
        crate::test_support::tenant_scope_for_ids(org_id, user_id)
    }

    /// Read the audit trail rows written for the org, oldest first.
    async fn audit_rows(pool: &PgPool, org_id: Uuid) -> Vec<(String, Option<Uuid>, serde_json::Value)> {
        sqlx::query_as(
            "SELECT action, resource_id, details FROM audit_log WHERE organization_id = $1 ORDER BY created_at",
        )
        .bind(org_id)
        .fetch_all(pool)
        .await
        .expect("read audit rows")
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn update_user_role_promotes_and_demotes_with_audit_trail(pool: PgPool) {
        let service = admin_service(&pool);
        let org_id = seed_org(&pool).await;
        let actor = seed_user(&pool, "operator@example.com", true).await;
        let target = seed_user(&pool, "teammate@example.com", false).await;
        let scope = scope_for(org_id, actor);

        // Promote member → admin: the response role flips immediately.
        let promoted = service.update_user_role(&scope, target, "admin").await.expect("promote");
        assert_eq!(promoted.role, "admin");
        assert_eq!(promoted.email, "teammate@example.com");

        // Demote one of two admins back to member: allowed, role flips back.
        let demoted = service.update_user_role(&scope, target, "member").await.expect("demote");
        assert_eq!(demoted.role, "member");

        // Re-applying the same role is an idempotent success, not an error.
        let unchanged = service.update_user_role(&scope, target, "member").await.expect("idempotent demote");
        assert_eq!(unchanged.role, "member");

        let rows = audit_rows(&pool, org_id).await;
        assert_eq!(rows.len(), 3, "every applied change writes an audit row");
        let (action, resource_id, details) = &rows[0];
        assert_eq!(action, "admin.user.role_updated");
        assert_eq!(*resource_id, Some(target));
        assert_eq!(details["email"], "teammate@example.com");
        assert_eq!(details["oldRole"], "member");
        assert_eq!(details["newRole"], "admin");
        assert_eq!(rows[1].2["oldRole"], "admin");
        assert_eq!(rows[1].2["newRole"], "member");
    }

    /// The JWT role claim is minted from `organization_members.role`, so a
    /// role change must dual-write the membership row — and never touch an
    /// owner from this surface.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn update_user_role_dual_writes_membership_and_protects_owner(pool: PgPool) {
        let service = admin_service(&pool);
        let org_id = seed_org(&pool).await;
        let actor = seed_user(&pool, "operator@example.com", true).await;
        let target = seed_user(&pool, "teammate@example.com", false).await;
        let owner = seed_user(&pool, "owner@example.com", true).await;
        seed_membership(&pool, org_id, actor, "admin").await;
        seed_membership(&pool, org_id, target, "member").await;
        seed_membership(&pool, org_id, owner, "owner").await;
        let scope = scope_for(org_id, actor);

        // Promotion flips the membership role so the next minted JWT is admin.
        service.update_user_role(&scope, target, "admin").await.expect("promote");
        assert_eq!(membership_role(&pool, org_id, target).await.as_deref(), Some("admin"));

        // Demotion flips it back.
        service.update_user_role(&scope, target, "member").await.expect("demote");
        assert_eq!(membership_role(&pool, org_id, target).await.as_deref(), Some("member"));

        // Owners are not managed from this surface — role change and removal
        // both refuse, and the membership row is untouched.
        let err = service.update_user_role(&scope, owner, "member").await.expect_err("owner role protected");
        assert!(matches!(err.kind, ErrorKind::Unprocessable(_)), "owner change → 422, got {:?}", err.kind);
        let err = service.delete_user(&scope, owner).await.expect_err("owner removal protected");
        assert!(matches!(err.kind, ErrorKind::Unprocessable(_)), "owner delete → 422, got {:?}", err.kind);
        assert_eq!(membership_role(&pool, org_id, owner).await.as_deref(), Some("owner"));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn update_user_role_rejects_last_admin_demotion_and_self_change(pool: PgPool) {
        let service = admin_service(&pool);
        let org_id = seed_org(&pool).await;
        let actor = seed_user(&pool, "operator@example.com", false).await;
        let only_admin = seed_user(&pool, "solo-admin@example.com", true).await;
        let scope = scope_for(org_id, actor);

        // Demoting the final remaining admin is rejected with 422.
        let err = service.update_user_role(&scope, only_admin, "member").await.expect_err("last admin protected");
        assert!(matches!(err.kind, ErrorKind::Unprocessable(ref msg) if msg.contains("only admin")));

        // Self-changes are rejected with 422 — even a self-promotion.
        let self_err = service.update_user_role(&scope, actor, "admin").await.expect_err("self change rejected");
        assert!(matches!(self_err.kind, ErrorKind::Unprocessable(ref msg) if msg.contains("your own account")));

        // Unknown roles are rejected with 422 before any lookup.
        let role_err = service.update_user_role(&scope, only_admin, "owner").await.expect_err("unknown role");
        assert!(matches!(role_err.kind, ErrorKind::Unprocessable(_)));

        // Unknown targets are a 404.
        let missing = service.update_user_role(&scope, Uuid::new_v4(), "admin").await.expect_err("missing user");
        assert!(matches!(missing.kind, ErrorKind::NotFound(_)));

        // No rejected attempt wrote an audit row.
        assert!(audit_rows(&pool, org_id).await.is_empty(), "rejections leave no audit rows");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn delete_user_soft_deletes_members_and_writes_audit(pool: PgPool) {
        let service = admin_service(&pool);
        let org_id = seed_org(&pool).await;
        let actor = seed_user(&pool, "operator@example.com", true).await;
        let target = seed_user(&pool, "leaver@example.com", false).await;
        let scope = scope_for(org_id, actor);

        service.delete_user(&scope, target).await.expect("delete member");

        let deleted_at: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT deleted_at FROM users WHERE id = $1")
                .bind(target)
                .fetch_one(&pool)
                .await
                .expect("read deleted_at");
        assert!(deleted_at.is_some(), "removal is a soft delete");

        // The admin user list no longer returns the removed account.
        let page = service.list_user_page(1, 50, None).await.expect("user page");
        assert!(page.users.iter().all(|user| user.id != target), "deleted user left the admin list");

        let rows = audit_rows(&pool, org_id).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "admin.user.deleted");
        assert_eq!(rows[0].1, Some(target));
        assert_eq!(rows[0].2["email"], "leaver@example.com");
        assert_eq!(rows[0].2["role"], "member");

        // Deleting again is a 404 — the account is already gone.
        let again = service.delete_user(&scope, target).await.expect_err("already removed");
        assert!(matches!(again.kind, ErrorKind::NotFound(_)));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn delete_user_rejects_self_and_last_admin(pool: PgPool) {
        let service = admin_service(&pool);
        let org_id = seed_org(&pool).await;
        let actor = seed_user(&pool, "operator@example.com", false).await;
        let only_admin = seed_user(&pool, "solo-admin@example.com", true).await;
        let scope = scope_for(org_id, actor);

        // Self-deletion is rejected with 422.
        let self_err = service.delete_user(&scope, actor).await.expect_err("self deletion rejected");
        assert!(matches!(self_err.kind, ErrorKind::Unprocessable(ref msg) if msg.contains("your own account")));

        // Deleting the final remaining admin is rejected with 422.
        let err = service.delete_user(&scope, only_admin).await.expect_err("last admin protected");
        assert!(matches!(err.kind, ErrorKind::Unprocessable(ref msg) if msg.contains("only admin")));

        // A second admin unblocks the deletion (deleting one of two succeeds).
        let second_admin = seed_user(&pool, "second-admin@example.com", true).await;
        service.delete_user(&scope, second_admin).await.expect("delete one of two admins");

        // Both targets stay untouched / the deleted one is stamped.
        let live_admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin AND deleted_at IS NULL")
            .fetch_one(&pool)
            .await
            .expect("count admins");
        assert_eq!(live_admins, 1, "exactly the protected admin remains");
    }

    /// The dead-letter reader gate keys off `users.is_admin`, not the per-org
    /// JWT role. Seed one platform admin (passes) and one ordinary user (403),
    /// proving a self-registered org owner cannot read the cross-org table.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn require_platform_admin_keys_off_is_admin_column(pool: PgPool) {
        let service = admin_service(&pool);
        let platform_admin = seed_user(&pool, "platform-admin@example.com", true).await;
        let ordinary = seed_user(&pool, "ordinary@example.com", false).await;

        service.require_platform_admin(platform_admin).await.expect("is_admin=true passes the gate");

        let err = service.require_platform_admin(ordinary).await.expect_err("is_admin=false is rejected");
        assert!(matches!(err.kind, ErrorKind::Forbidden(_)), "non-admin → 403 Forbidden, got {:?}", err.kind);

        // An unknown user id is a 404 (the lookup fails before the gate).
        let missing = service.require_platform_admin(Uuid::new_v4()).await.expect_err("unknown user");
        assert!(matches!(missing.kind, ErrorKind::NotFound(_)), "unknown user → 404, got {:?}", missing.kind);
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
