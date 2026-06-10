//! Admin repository — cross-tenant database queries for admin operations.

use agentforge_core::{AgentStatus, AppResult, RuntimeKind, TenantScope};
use agentforge_db::entities::{ImpersonationLog, User};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool, QueryBuilder};
use uuid::Uuid;

use crate::domain::admin::{AdminAgentSort, AdminRepositoryPolicy, SortOrder};

/// Filter parameters for the admin agent list query.
#[derive(Debug, Default, Clone)]
pub struct AdminAgentFilters {
    pub search: Option<String>,
    pub status: Option<AgentStatus>,
    pub runtime_kind: Option<RuntimeKind>,
    pub user_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub sort_by: AdminAgentSort,
    pub sort_order: SortOrder,
    pub limit: i64,
    pub offset: i64,
}

/// Row shape for the admin agent list endpoint. Joins in owner/project info and
/// computes `events_count` from the events table; `last_activity` prefers the
/// cached `agents.last_activity_at` column (migration 013) and falls back to
/// `agents.updated_at` for legacy rows.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AdminAgentRow {
    pub id: Uuid,
    pub name: Option<String>,
    pub status: AgentStatus,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub container_id: Option<String>,
    pub cli_session_id: Option<String>,
    pub cwd: Option<String>,
    pub current_tool: Option<String>,
    pub cli_tool: Option<String>,
    pub tokens_current: i64,
    pub tokens_cumulative: i64,
    pub git_status: Option<String>,
    pub runtime_id: Option<String>,
    pub runtime_kind: RuntimeKind,
    pub organization_id: Uuid,
    pub project_id: Option<Uuid>,
    pub user_id: Uuid,
    pub owner_username: Option<String>,
    pub owner_email: Option<String>,
    pub project_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub events_count: i64,
}

/// Base SELECT used by every admin agent query, with LEFT JOINs for owner,
/// project, and a lateral subquery for event aggregates. Last-activity falls
/// back to `agents.updated_at` when no events have been recorded yet.
const ADMIN_AGENT_SELECT: &str = r#"SELECT
    a.id,
    a.name,
    a.status,
    a.model,
    a.provider,
    a.container_id,
    a.cli_session_id,
    a.cwd,
    a.current_tool,
    a.cli_tool,
    a.tokens_current,
    a.tokens_cumulative,
    a.git_status,
    a.runtime_id,
    a.runtime_kind,
    a.organization_id,
    a.project_id,
    a.user_id,
    u.display_name AS owner_username,
    u.email        AS owner_email,
    p.name         AS project_name,
    a.created_at,
    a.updated_at,
    COALESCE(a.last_activity_at, ev.max_created_at, a.updated_at) AS last_activity,
    COALESCE(ev.events_count, 0)                                  AS events_count
FROM agents a
LEFT JOIN users u    ON a.user_id    = u.id
LEFT JOIN projects p ON a.project_id = p.id
LEFT JOIN LATERAL (
    SELECT MAX(created_at) AS max_created_at, COUNT(*) AS events_count
    FROM events
    WHERE agent_id = a.id
) ev ON true"#;

/// Map the domain sort enum to a SQL column reference used in the enriched query.
fn admin_agent_sort_sql_column(sort: AdminAgentSort) -> &'static str {
    match sort {
        AdminAgentSort::Name => "a.name",
        AdminAgentSort::Status => "a.status",
        AdminAgentSort::LastActivity => "last_activity",
        AdminAgentSort::CreatedAt => "a.created_at",
        AdminAgentSort::OwnerUsername => "u.display_name",
    }
}

fn sort_order_sql_keyword(order: SortOrder) -> &'static str {
    match order {
        SortOrder::Asc => "ASC",
        SortOrder::Desc => "DESC",
    }
}

/// Database access layer for admin operations.
/// Note: some queries are NOT tenant-scoped — admin can see all orgs/users.
pub struct AdminRepository {
    pool: PgPool,
}

impl AdminRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List ALL users across all organizations (admin only). An optional
    /// `search` term filters by email or display name substring
    /// (case-insensitive); blank input behaves like "no filter".
    pub async fn list_all_users(&self, limit: i64, offset: i64, search: Option<&str>) -> AppResult<Vec<User>> {
        let users = match Self::search_pattern(search) {
            Some(pattern) => {
                sqlx::query_as::<_, User>(
                    r#"SELECT * FROM users
                       WHERE deleted_at IS NULL
                         AND (email ILIKE $3 OR display_name ILIKE $3)
                       ORDER BY created_at DESC
                       LIMIT $1 OFFSET $2"#,
                )
                .bind(limit)
                .bind(offset)
                .bind(pattern)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, User>(
                    r#"SELECT * FROM users
                       WHERE deleted_at IS NULL
                       ORDER BY created_at DESC
                       LIMIT $1 OFFSET $2"#,
                )
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(users)
    }

    /// Count users matching the same WHERE clauses as [`Self::list_all_users`]
    /// (non-deleted + optional search), for pagination metadata.
    pub async fn count_users(&self, search: Option<&str>) -> AppResult<i64> {
        let total = match Self::search_pattern(search) {
            Some(pattern) => {
                sqlx::query_scalar::<_, i64>(
                    r#"SELECT COUNT(*) FROM users
                       WHERE deleted_at IS NULL
                         AND (email ILIKE $1 OR display_name ILIKE $1)"#,
                )
                .bind(pattern)
                .fetch_one(&self.pool)
                .await?
            }
            None => {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE deleted_at IS NULL")
                    .fetch_one(&self.pool)
                    .await?
            }
        };
        Ok(total)
    }

    /// Normalise an optional search term into an ILIKE pattern. Whitespace-only
    /// input behaves like "no search". The pattern is bound, never interpolated.
    fn search_pattern(search: Option<&str>) -> Option<String> {
        let trimmed = search.map(str::trim).filter(|s| !s.is_empty())?;
        Some(format!("%{trimmed}%"))
    }

    /// List ALL organizations (admin only) with live member and team counts.
    /// Soft-deleted organizations and teams are excluded;
    /// `organization_members` has no `deleted_at`, so membership is a plain count.
    pub async fn list_all_organizations_with_counts(&self, limit: i64, offset: i64) -> AppResult<Vec<AdminOrgRow>> {
        let orgs = sqlx::query_as::<_, AdminOrgRow>(
            r#"SELECT
                   o.id,
                   o.name,
                   o.slug,
                   o.created_at,
                   (SELECT COUNT(*) FROM organization_members m
                     WHERE m.organization_id = o.id)                          AS members_count,
                   (SELECT COUNT(*) FROM teams t
                     WHERE t.organization_id = o.id AND t.deleted_at IS NULL) AS teams_count
               FROM organizations o
               WHERE o.deleted_at IS NULL
               ORDER BY o.created_at DESC
               LIMIT $1 OFFSET $2"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(orgs)
    }

    /// Count non-deleted organizations (for pagination metadata).
    pub async fn count_organizations(&self) -> AppResult<i64> {
        let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM organizations WHERE deleted_at IS NULL")
            .fetch_one(&self.pool)
            .await?;
        Ok(total)
    }

    /// Start an impersonation session (admin only).
    pub async fn start_impersonation(
        &self,
        scope: &TenantScope,
        target_user_id: Uuid,
        reason: Option<&str>,
    ) -> AppResult<ImpersonationLog> {
        sqlx::query_as::<_, ImpersonationLog>(
            r#"INSERT INTO impersonation_log (admin_user_id, target_user_id, organization_id, reason)
               VALUES ($1, $2, $3, $4)
               RETURNING *"#,
        )
        .bind(scope.user_id().as_uuid())
        .bind(target_user_id)
        .bind(scope.org_id().as_uuid())
        .bind(reason)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// End an impersonation session.
    pub async fn end_impersonation(&self, scope: &TenantScope) -> AppResult<ImpersonationLog> {
        sqlx::query_as::<_, ImpersonationLog>(
            r#"UPDATE impersonation_log SET ended_at = NOW()
               WHERE admin_user_id = $1 AND organization_id = $2 AND ended_at IS NULL
               RETURNING *"#,
        )
        .bind(scope.user_id().as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(AdminRepositoryPolicy::active_impersonation_not_found)
    }

    /// List impersonation history for the org.
    pub async fn list_impersonation_log(
        &self,
        scope: &TenantScope,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<ImpersonationLog>> {
        let logs = sqlx::query_as::<_, ImpersonationLog>(
            r#"SELECT * FROM impersonation_log
               WHERE organization_id = $1
               ORDER BY started_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(logs)
    }

    /// List agents across every organization with filtering, sorting, and
    /// pagination (admin only — no tenant filter applied). Returns the page
    /// of rows together with the total row count for pagination display.
    pub async fn list_agents(&self, filters: &AdminAgentFilters) -> AppResult<(Vec<AdminAgentRow>, i64)> {
        let total = self.count_agents(filters).await?;

        let mut builder: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(ADMIN_AGENT_SELECT);
        Self::push_where_clause(&mut builder, filters);

        // Safe: the sort helpers return compile-time string literals.
        builder.push(format!(
            "\nORDER BY {} {} NULLS LAST\nLIMIT ",
            admin_agent_sort_sql_column(filters.sort_by),
            sort_order_sql_keyword(filters.sort_order)
        ));
        builder.push_bind(filters.limit);
        builder.push(" OFFSET ");
        builder.push_bind(filters.offset);

        let rows = builder.build_query_as::<AdminAgentRow>().fetch_all(&self.pool).await?;
        Ok((rows, total))
    }

    /// Count agents matching the list filters (for pagination metadata).
    async fn count_agents(&self, filters: &AdminAgentFilters) -> AppResult<i64> {
        let mut builder: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            r#"SELECT COUNT(*)
               FROM agents a
               LEFT JOIN users u    ON a.user_id    = u.id
               LEFT JOIN projects p ON a.project_id = p.id"#,
        );
        Self::push_where_clause(&mut builder, filters);
        let total: i64 = builder.build_query_scalar().fetch_one(&self.pool).await?;
        Ok(total)
    }

    /// Append WHERE conditions shared by list/count queries. Uses bind params
    /// throughout — no string interpolation of user input.
    fn push_where_clause(builder: &mut QueryBuilder<'_, sqlx::Postgres>, filters: &AdminAgentFilters) {
        let mut has_where = false;
        let add_prefix = |b: &mut QueryBuilder<'_, sqlx::Postgres>, has: &mut bool| {
            if *has {
                b.push(" AND ");
            } else {
                b.push("\nWHERE ");
                *has = true;
            }
        };

        if let Some(search) = filters.search.as_ref().filter(|s| !s.is_empty()) {
            add_prefix(builder, &mut has_where);
            builder.push("(a.name ILIKE ");
            builder.push_bind(format!("%{search}%"));
            builder.push(" OR u.email ILIKE ");
            builder.push_bind(format!("%{search}%"));
            builder.push(" OR u.display_name ILIKE ");
            builder.push_bind(format!("%{search}%"));
            builder.push(")");
        }

        if let Some(status) = filters.status {
            add_prefix(builder, &mut has_where);
            builder.push("a.status = ");
            builder.push_bind(status);
        }

        if let Some(runtime_kind) = filters.runtime_kind {
            add_prefix(builder, &mut has_where);
            builder.push("a.runtime_kind = ");
            builder.push_bind(runtime_kind);
        }

        if let Some(user_id) = filters.user_id {
            add_prefix(builder, &mut has_where);
            builder.push("a.user_id = ");
            builder.push_bind(user_id);
        }

        if let Some(project_id) = filters.project_id {
            add_prefix(builder, &mut has_where);
            builder.push("a.project_id = ");
            builder.push_bind(project_id);
        }
    }

    /// Fetch a single agent across all organizations (admin only).
    pub async fn find_agent_by_id(&self, agent_id: Uuid) -> AppResult<AdminAgentRow> {
        let query = format!("{ADMIN_AGENT_SELECT}\nWHERE a.id = $1");
        sqlx::query_as::<_, AdminAgentRow>(&query)
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AdminRepositoryPolicy::agent_not_found(agent_id))
    }

    /// Recent events for the admin agent detail view.
    pub async fn recent_events_for_agent(&self, agent_id: Uuid, limit: i64) -> AppResult<Vec<AdminAgentEventRow>> {
        let rows = sqlx::query_as::<_, AdminAgentEventRow>(
            r#"SELECT id, event_type, created_at
               FROM events
               WHERE agent_id = $1
               ORDER BY created_at DESC
               LIMIT $2"#,
        )
        .bind(agent_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Hard-delete an agent regardless of organization (admin only).
    pub async fn delete_agent(&self, agent_id: Uuid) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM agents WHERE id = $1").bind(agent_id).execute(&self.pool).await?;
        if result.rows_affected() == 0 {
            return Err(AdminRepositoryPolicy::agent_not_found(agent_id));
        }
        Ok(())
    }

    /// Get system-wide statistics.
    pub async fn stats(&self) -> AppResult<AdminStats> {
        let total_users = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE deleted_at IS NULL")
            .fetch_one(&self.pool)
            .await?;
        let total_agents = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM agents").fetch_one(&self.pool).await?;
        let total_events = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events").fetch_one(&self.pool).await?;
        let total_organizations =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM organizations WHERE deleted_at IS NULL")
                .fetch_one(&self.pool)
                .await?;

        Ok(AdminStats { total_users, total_agents, total_events, total_organizations })
    }

    /// Count agents that currently have an associated container, grouped by
    /// `cli_tool`, across ALL organizations. Deployment-global on purpose: the
    /// CLI image auto-updater status is per host, not per tenant, so this
    /// intentionally spans orgs (it is only reachable from the admin-gated
    /// status endpoint). `container_id IS NOT NULL` is the signal that a
    /// container was provisioned for the agent — a rough blast-radius hint, NOT
    /// an assertion about which image digest each container booted from.
    pub async fn container_agent_counts_by_tool(&self) -> AppResult<Vec<(String, i64)>> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT cli_tool, COUNT(*) \
             FROM agents \
             WHERE cli_tool IS NOT NULL AND container_id IS NOT NULL \
             GROUP BY cli_tool",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Enumerate the RUNNING container agents of one tool across ALL orgs, for
    /// the admin-gated roll. Deployment-global by design (image state is per
    /// host). The `runtime_kind = 'container'` filter is CRITICAL — the count
    /// query omits it, but here it guarantees a cli/api agent with an incidental
    /// `container_id` is never picked up and stopped. Each row carries the
    /// agent's OWN org/user/workspace so the roll acts within that agent's real
    /// tenant scope, never a fabricated-privilege one.
    pub async fn running_container_agents_by_tool(&self, tool: &str) -> AppResult<Vec<RollTargetRow>> {
        let rows = sqlx::query_as::<_, RollTargetRow>(
            "SELECT id, organization_id, user_id, workspace_id, status \
             FROM agents \
             WHERE cli_tool = $1 AND container_id IS NOT NULL AND runtime_kind = 'container'",
        )
        .bind(tool)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

/// One roll target: a running container agent plus its OWN tenant axes, so the
/// roll reconstructs the agent's real scope rather than fabricating privilege.
/// `status` lets the roll skip a `working` agent (rolling one would interrupt
/// in-flight work + risk a redelivered assignment double-executing).
#[derive(Debug, Clone, FromRow)]
pub struct RollTargetRow {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub status: AgentStatus,
}

/// System-wide statistics returned by the admin stats endpoint.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AdminStats {
    pub total_users: i64,
    pub total_agents: i64,
    pub total_events: i64,
    pub total_organizations: i64,
}

/// Recent event row for the admin agent detail panel.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AdminAgentEventRow {
    pub id: Uuid,
    pub event_type: String,
    pub created_at: DateTime<Utc>,
}

/// Organization row for the admin organizations list: org identity plus live
/// member/team counts computed by scalar subqueries.
#[derive(Debug, Clone, FromRow)]
pub struct AdminOrgRow {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
    pub members_count: i64,
    pub teams_count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    /// Seed one user row; `deleted` controls the soft-delete marker.
    async fn seed_user(pool: &PgPool, email: &str, display_name: Option<&str>, deleted: bool) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, display_name, deleted_at)
             VALUES ($1, $2, $3, CASE WHEN $4 THEN now() ELSE NULL END)",
        )
        .bind(id)
        .bind(email)
        .bind(display_name)
        .bind(deleted)
        .execute(pool)
        .await
        .expect("seed user");
        id
    }

    /// Seed one organization row.
    async fn seed_org(pool: &PgPool, name: &str, deleted: bool) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO organizations (id, name, slug, deleted_at)
             VALUES ($1, $2, $3, CASE WHEN $4 THEN now() ELSE NULL END)",
        )
        .bind(id)
        .bind(name)
        .bind(format!("org-{id}"))
        .bind(deleted)
        .execute(pool)
        .await
        .expect("seed organization");
        id
    }

    async fn seed_membership(pool: &PgPool, org_id: Uuid, user_id: Uuid) {
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')")
            .bind(org_id)
            .bind(user_id)
            .execute(pool)
            .await
            .expect("seed membership");
    }

    async fn seed_team(pool: &PgPool, org_id: Uuid, name: &str, deleted: bool) {
        // `teams.slug` is NOT NULL since migration 026.
        sqlx::query(
            "INSERT INTO teams (organization_id, name, slug, deleted_at)
             VALUES ($1, $2, $3, CASE WHEN $4 THEN now() ELSE NULL END)",
        )
        .bind(org_id)
        .bind(name)
        .bind(format!("team-{}", Uuid::new_v4()))
        .bind(deleted)
        .execute(pool)
        .await
        .expect("seed team");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn user_search_filters_by_email_substring_and_count_matches(pool: PgPool) {
        let repo = AdminRepository::new(pool.clone());
        seed_user(&pool, "alice@example.com", Some("Alice"), false).await;
        seed_user(&pool, "bob@example.com", Some("Bob"), false).await;
        seed_user(&pool, "carol@other.test", Some("Carol"), false).await;
        // Soft-deleted rows never appear, even when they match the search.
        seed_user(&pool, "alice-gone@example.com", Some("Alice Gone"), true).await;

        let rows = repo.list_all_users(50, 0, Some("alice")).await.expect("search by email substring");
        assert_eq!(rows.len(), 1, "only the live alice row matches");
        assert_eq!(rows[0].email, "alice@example.com");
        assert_eq!(repo.count_users(Some("alice")).await.expect("count search"), 1, "count matches the list filter");

        // Case-insensitive display-name match.
        let rows = repo.list_all_users(50, 0, Some("CAROL")).await.expect("search by display name");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].email, "carol@other.test");

        // No / blank search returns every live user.
        assert_eq!(repo.list_all_users(50, 0, None).await.expect("unfiltered list").len(), 3);
        assert_eq!(repo.count_users(None).await.expect("unfiltered count"), 3);
        assert_eq!(repo.count_users(Some("   ")).await.expect("blank search count"), 3);
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn org_counts_reflect_memberships_and_live_teams(pool: PgPool) {
        let repo = AdminRepository::new(pool.clone());
        let org_a = seed_org(&pool, "Org A", false).await;
        let org_b = seed_org(&pool, "Org B", false).await;
        // Deleted org must not appear at all.
        seed_org(&pool, "Org Gone", true).await;

        let u1 = seed_user(&pool, "u1@example.com", None, false).await;
        let u2 = seed_user(&pool, "u2@example.com", None, false).await;
        seed_membership(&pool, org_a, u1).await;
        seed_membership(&pool, org_a, u2).await;
        seed_membership(&pool, org_b, u1).await;

        seed_team(&pool, org_a, "Live Team", false).await;
        seed_team(&pool, org_a, "Deleted Team", true).await;

        let rows = repo.list_all_organizations_with_counts(50, 0).await.expect("list orgs with counts");
        assert_eq!(rows.len(), 2, "deleted org excluded");

        let row_a = rows.iter().find(|r| r.id == org_a).expect("org A row");
        assert_eq!(row_a.members_count, 2);
        assert_eq!(row_a.teams_count, 1, "deleted team excluded from the count");

        let row_b = rows.iter().find(|r| r.id == org_b).expect("org B row");
        assert_eq!(row_b.members_count, 1);
        assert_eq!(row_b.teams_count, 0);

        assert_eq!(repo.count_organizations().await.expect("count orgs"), 2);
    }
}
