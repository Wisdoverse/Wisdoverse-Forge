//! Admin repository — cross-tenant database queries for admin operations.

use agentforge_core::{AgentStatus, AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::{ImpersonationLog, Organization, User};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool, QueryBuilder};
use uuid::Uuid;

use crate::domain::admin::{AdminAgentSort, SortOrder};

/// Filter parameters for the admin agent list query.
#[derive(Debug, Default, Clone)]
pub struct AdminAgentFilters {
    pub search: Option<String>,
    pub status: Option<AgentStatus>,
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

    /// List ALL users across all organizations (admin only).
    pub async fn list_all_users(&self, limit: i64, offset: i64) -> AppResult<Vec<User>> {
        let users = sqlx::query_as::<_, User>(
            r#"SELECT * FROM users
               WHERE deleted_at IS NULL
               ORDER BY created_at DESC
               LIMIT $1 OFFSET $2"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(users)
    }

    /// List ALL organizations (admin only).
    pub async fn list_all_organizations(&self, limit: i64, offset: i64) -> AppResult<Vec<Organization>> {
        let orgs = sqlx::query_as::<_, Organization>(
            r#"SELECT * FROM organizations
               WHERE deleted_at IS NULL
               ORDER BY created_at DESC
               LIMIT $1 OFFSET $2"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(orgs)
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
        .ok_or_else(|| ErrorKind::NotFound("no active impersonation session".into()).into())
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
            .ok_or_else(|| ErrorKind::NotFound(format!("agent {agent_id}")).into())
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
            return Err(ErrorKind::NotFound(format!("agent {agent_id}")).into());
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
