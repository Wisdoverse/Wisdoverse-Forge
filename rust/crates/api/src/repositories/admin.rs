//! Admin repository — cross-tenant database queries for admin operations.

use agentforge_core::{AgentStatus, AppResult, RuntimeKind, TenantScope};
use agentforge_db::entities::{ImpersonationLog, User};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool, QueryBuilder};
use uuid::Uuid;

use crate::domain::admin::{
    AdminAgentSort, AdminRepositoryPolicy, DeadEventRow, OrgControlPlaneSnapshot, SortOrder, admin_user_not_found_error,
};

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

const ORG_OUTBOX_BACKLOG_SQL: &str = "SELECT COUNT(*) FROM orchestration_outbox \
    WHERE organization_id = $1 AND published_at IS NULL AND event_type = 'assignment'";

const ORG_OUTBOX_OLDEST_AGE_SQL: &str = "SELECT COALESCE(CAST(EXTRACT(EPOCH FROM (NOW() - MIN(created_at))) AS DOUBLE PRECISION), 0.0) \
    FROM orchestration_outbox WHERE organization_id = $1 AND published_at IS NULL AND event_type = 'assignment'";

const ORG_STALE_PARTICIPANTS_SQL: &str = "SELECT COUNT(*) FROM participants \
    WHERE organization_id = $1 AND status <> 'offline' \
    AND (last_heartbeat_at IS NULL OR last_heartbeat_at < NOW() - ($2::int * INTERVAL '1 second'))";

const ORG_EXPIRED_LEASES_SQL: &str = "SELECT COUNT(*) FROM orchestration_tasks \
    WHERE organization_id = $1 AND status = 'working' \
    AND lease_expires_at IS NOT NULL AND lease_expires_at < NOW()";

const ORG_BUSY_WITHOUT_WORK_SQL: &str = "SELECT COUNT(*) FROM participants p \
    WHERE p.organization_id = $1 AND p.status = 'busy' \
    AND NOT EXISTS (SELECT 1 FROM orchestration_tasks t \
        WHERE t.organization_id = p.organization_id AND t.assigned_agent_id = p.agent_id AND t.status = 'working')";

const ORG_WORK_WITHOUT_BUSY_SQL: &str = "SELECT COUNT(*) FROM orchestration_tasks t \
    WHERE t.organization_id = $1 AND t.status = 'working' \
    AND NOT EXISTS (SELECT 1 FROM participants p \
        WHERE p.organization_id = t.organization_id AND p.agent_id = t.assigned_agent_id AND p.status = 'busy')";

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

    /// Fetch a single live user across all organizations (admin only).
    pub async fn find_user_by_id(&self, user_id: Uuid) -> AppResult<User> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1 AND deleted_at IS NULL")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| admin_user_not_found_error(user_id))
    }

    /// Set the GLOBAL admin flag for a live user (admin only) and return the
    /// updated row. Soft-deleted users cannot be modified — that is a 404.
    pub async fn set_user_admin(&self, user_id: Uuid, is_admin: bool) -> AppResult<User> {
        sqlx::query_as::<_, User>(
            r#"UPDATE users SET is_admin = $2, updated_at = NOW()
               WHERE id = $1 AND deleted_at IS NULL
               RETURNING *"#,
        )
        .bind(user_id)
        .bind(is_admin)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| admin_user_not_found_error(user_id))
    }

    /// Soft-delete a user account (admin only). The row keeps its history but
    /// disappears from the admin list and can no longer sign in. Deleting an
    /// already-deleted or unknown user is a 404.
    pub async fn soft_delete_user(&self, user_id: Uuid) -> AppResult<()> {
        let result =
            sqlx::query("UPDATE users SET deleted_at = NOW(), updated_at = NOW() WHERE id = $1 AND deleted_at IS NULL")
                .bind(user_id)
                .execute(&self.pool)
                .await?;
        if result.rows_affected() == 0 {
            return Err(admin_user_not_found_error(user_id));
        }
        Ok(())
    }

    /// Count live admin accounts deployment-wide. Backs the last-admin guard:
    /// the final remaining admin can never be demoted or deleted.
    pub async fn count_active_admins(&self) -> AppResult<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE is_admin AND deleted_at IS NULL")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    /// The target's membership role in one organization (`None` = not a member).
    /// The JWT `role` claim is minted from this column at login/context-switch,
    /// so role edits must keep it in step with `users.is_admin`.
    pub async fn member_role(&self, org_id: Uuid, user_id: Uuid) -> AppResult<Option<String>> {
        let role = sqlx::query_scalar::<_, String>(
            "SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2 LIMIT 1",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(role)
    }

    /// Sync the org-membership role so the next minted JWT carries the new
    /// access level. Returns whether a membership row existed to update.
    /// `owner` rows are never written here — the service rejects owner targets
    /// before calling.
    pub async fn set_member_role(&self, org_id: Uuid, user_id: Uuid, role: &str) -> AppResult<bool> {
        let result = sqlx::query(
            "UPDATE organization_members SET role = $3 WHERE organization_id = $1 AND user_id = $2 AND role <> 'owner'",
        )
        .bind(org_id)
        .bind(user_id)
        .bind(role)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
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

    /// Org-scoped orchestration control-plane snapshot. Runs the same wedged-state
    /// checks the `OrchestrationMetricsWorker` emits globally, constrained to one
    /// organization via `WHERE organization_id = $1`. `job_queue` depth is omitted
    /// (no org column). `stale_after_secs` is the participant-staleness threshold.
    pub(crate) async fn org_control_plane_snapshot(
        &self,
        scope: &TenantScope,
        stale_after_secs: i64,
    ) -> AppResult<OrgControlPlaneSnapshot> {
        let org = scope.org_id().as_uuid();
        let assignment_outbox_backlog =
            sqlx::query_scalar::<_, i64>(ORG_OUTBOX_BACKLOG_SQL).bind(org).fetch_one(&self.pool).await?;
        let assignment_outbox_oldest_age_seconds =
            sqlx::query_scalar::<_, f64>(ORG_OUTBOX_OLDEST_AGE_SQL).bind(org).fetch_one(&self.pool).await?;
        // ponytail: clamp to i32 for the `$2::int` bind; staleness is seconds-to-minutes,
        // so saturating at i32::MAX (~68 yr) can never affect a real threshold.
        let stale_after_param = stale_after_secs.clamp(0, i64::from(i32::MAX)) as i32;
        let stale_participants = sqlx::query_scalar::<_, i64>(ORG_STALE_PARTICIPANTS_SQL)
            .bind(org)
            .bind(stale_after_param)
            .fetch_one(&self.pool)
            .await?;
        let expired_working_leases =
            sqlx::query_scalar::<_, i64>(ORG_EXPIRED_LEASES_SQL).bind(org).fetch_one(&self.pool).await?;
        let busy_participants_without_work =
            sqlx::query_scalar::<_, i64>(ORG_BUSY_WITHOUT_WORK_SQL).bind(org).fetch_one(&self.pool).await?;
        let working_tasks_without_busy_participant =
            sqlx::query_scalar::<_, i64>(ORG_WORK_WITHOUT_BUSY_SQL).bind(org).fetch_one(&self.pool).await?;

        Ok(OrgControlPlaneSnapshot {
            assignment_outbox_backlog,
            assignment_outbox_oldest_age_seconds,
            stale_participants,
            expired_working_leases,
            busy_participants_without_work,
            working_tasks_without_busy_participant,
            stale_after_seconds: stale_after_secs,
        })
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

    /// Every container-runtime agent that still references a container, with its
    /// OWN tenant axes. The reconcile backstop inspects each against Docker and
    /// clears the reference when the container is actually gone — converging rows
    /// left stale by an unverified or partial stop. The
    /// `runtime_kind = 'container'` filter mirrors the roll query so a cli/api
    /// agent with an incidental `container_id` is never touched.
    pub async fn container_agents_with_reference(&self) -> AppResult<Vec<ContainerAgentRef>> {
        let rows = sqlx::query_as::<_, ContainerAgentRef>(
            "SELECT id, organization_id, user_id, workspace_id, container_id \
             FROM agents \
             WHERE container_id IS NOT NULL AND runtime_kind = 'container'",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// List dead-letter rows (permanently-dropped NATS envelopes), newest first,
    /// across ALL organizations. PLATFORM-scoped on purpose — `dead_events` is
    /// cross-org by design (most rows are pre-auth drops with a NULL `org_id`),
    /// following the `list_all_users` / `list_agents` unscoped-platform precedent
    /// in this file. The route gates this to platform OWNER only. An optional
    /// `reason` filters exactly (bound, never interpolated); blank input behaves
    /// like "no filter".
    pub async fn list_dead_events(
        &self,
        limit: i64,
        offset: i64,
        reason: Option<&str>,
    ) -> AppResult<Vec<DeadEventRow>> {
        let rows = match Self::reason_filter(reason) {
            Some(reason) => {
                sqlx::query_as::<_, DeadEventRow>(
                    r#"SELECT id, source, reason, subject, detail, delivery_id, org_id, payload_excerpt, recorded_at
                       FROM dead_events
                       WHERE reason = $3
                       ORDER BY recorded_at DESC
                       LIMIT $1 OFFSET $2"#,
                )
                .bind(limit)
                .bind(offset)
                .bind(reason)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, DeadEventRow>(
                    r#"SELECT id, source, reason, subject, detail, delivery_id, org_id, payload_excerpt, recorded_at
                       FROM dead_events
                       ORDER BY recorded_at DESC
                       LIMIT $1 OFFSET $2"#,
                )
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(rows)
    }

    /// Count dead-letter rows matching the same optional `reason` filter as
    /// [`Self::list_dead_events`], for pagination metadata. Platform-scoped.
    pub async fn count_dead_events(&self, reason: Option<&str>) -> AppResult<i64> {
        let total = match Self::reason_filter(reason) {
            Some(reason) => {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM dead_events WHERE reason = $1")
                    .bind(reason)
                    .fetch_one(&self.pool)
                    .await?
            }
            None => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM dead_events").fetch_one(&self.pool).await?,
        };
        Ok(total)
    }

    /// Normalise an optional reason filter: whitespace-only input behaves like
    /// "no filter". The value is bound, never interpolated.
    fn reason_filter(reason: Option<&str>) -> Option<&str> {
        reason.map(str::trim).filter(|s| !s.is_empty())
    }
}

/// One container-runtime agent that references a container, plus its OWN tenant
/// axes, for the reconcile backstop. `container_id` is non-null by query
/// construction.
#[derive(Debug, Clone, FromRow)]
pub struct ContainerAgentRef {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub container_id: String,
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

    /// Seed one live user row with an explicit `is_admin` flag.
    async fn seed_user_with_admin(pool: &PgPool, email: &str, is_admin: bool) -> Uuid {
        let id = seed_user(pool, email, None, false).await;
        sqlx::query("UPDATE users SET is_admin = $2 WHERE id = $1")
            .bind(id)
            .bind(is_admin)
            .execute(pool)
            .await
            .expect("set is_admin");
        id
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn set_user_admin_flips_the_flag_and_skips_deleted_rows(pool: PgPool) {
        let repo = AdminRepository::new(pool.clone());
        let member = seed_user_with_admin(&pool, "member@example.com", false).await;

        let promoted = repo.set_user_admin(member, true).await.expect("promote member");
        assert!(promoted.is_admin);
        assert_eq!(promoted.email, "member@example.com");

        let demoted = repo.set_user_admin(member, false).await.expect("demote admin");
        assert!(!demoted.is_admin);

        // Unknown and soft-deleted rows are both a 404 — never a silent no-op.
        let missing = repo.set_user_admin(Uuid::new_v4(), true).await.expect_err("unknown user");
        assert!(matches!(missing.kind, agentforge_core::ErrorKind::NotFound(_)));
        let deleted = seed_user(&pool, "gone@example.com", None, true).await;
        let gone = repo.set_user_admin(deleted, true).await.expect_err("deleted user");
        assert!(matches!(gone.kind, agentforge_core::ErrorKind::NotFound(_)));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn soft_delete_user_marks_the_row_once(pool: PgPool) {
        let repo = AdminRepository::new(pool.clone());
        let user = seed_user(&pool, "leaver@example.com", Some("Leaver"), false).await;

        repo.soft_delete_user(user).await.expect("soft delete");
        let deleted_at: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT deleted_at FROM users WHERE id = $1")
                .bind(user)
                .fetch_one(&pool)
                .await
                .expect("read deleted_at");
        assert!(deleted_at.is_some(), "soft delete stamps deleted_at");

        // A second delete (or a lookup) sees the row as gone.
        let again = repo.soft_delete_user(user).await.expect_err("already deleted");
        assert!(matches!(again.kind, agentforge_core::ErrorKind::NotFound(_)));
        let lookup = repo.find_user_by_id(user).await.expect_err("deleted user lookup");
        assert!(matches!(lookup.kind, agentforge_core::ErrorKind::NotFound(_)));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn count_active_admins_ignores_members_and_deleted_admins(pool: PgPool) {
        let repo = AdminRepository::new(pool.clone());
        assert_eq!(repo.count_active_admins().await.expect("empty count"), 0);

        seed_user_with_admin(&pool, "admin-1@example.com", true).await;
        let admin_2 = seed_user_with_admin(&pool, "admin-2@example.com", true).await;
        seed_user_with_admin(&pool, "member@example.com", false).await;
        assert_eq!(repo.count_active_admins().await.expect("two admins"), 2);

        repo.soft_delete_user(admin_2).await.expect("delete one admin");
        assert_eq!(repo.count_active_admins().await.expect("one admin"), 1, "deleted admins do not count");
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

    #[sqlx::test(migrations = "../db/migrations")]
    async fn org_control_plane_snapshot_is_tenant_isolated(pool: PgPool) {
        // `TenantScope` and `Uuid` are already in scope via `super::*`.
        use agentforge_core::{AgentId, OrgId, UserId};

        async fn seed_org_row(pool: &PgPool, org: Uuid) {
            sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
                .bind(org)
                .bind(format!("Org {org}"))
                .bind(format!("org-{org}"))
                .execute(pool)
                .await
                .expect("seed org");
        }
        async fn seed_user_row(pool: &PgPool, user: Uuid) {
            sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
                .bind(user)
                .bind(format!("u-{user}@example.com"))
                .execute(pool)
                .await
                .expect("seed user");
        }
        async fn seed_workspace_row(pool: &PgPool, org: Uuid) -> Uuid {
            let workspace = Uuid::new_v4();
            sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, $3)")
                .bind(workspace)
                .bind(org)
                .bind(format!("Workspace {workspace}"))
                .execute(pool)
                .await
                .expect("seed workspace");
            workspace
        }
        // `agents.id` is the FK target for both `orchestration_tasks.assigned_agent_id`
        // and `participants.agent_id`; every agent referenced below must exist first.
        async fn seed_agent_row(pool: &PgPool, agent: Uuid, org: Uuid, workspace: Uuid, user: Uuid) {
            sqlx::query("INSERT INTO agents (id, organization_id, workspace_id, user_id) VALUES ($1, $2, $3, $4)")
                .bind(agent)
                .bind(org)
                .bind(workspace)
                .bind(user)
                .execute(pool)
                .await
                .expect("seed agent");
        }

        let org_a = OrgId::new();
        let org_b = OrgId::new();
        let user = UserId::new();
        seed_org_row(&pool, org_a.as_uuid()).await;
        seed_org_row(&pool, org_b.as_uuid()).await;
        seed_user_row(&pool, user.as_uuid()).await;
        let workspace_a = seed_workspace_row(&pool, org_a.as_uuid()).await;
        let workspace_b = seed_workspace_row(&pool, org_b.as_uuid()).await;

        let agent_a = AgentId::new();
        let agent_a_participant = AgentId::new();
        let agent_b_participant = AgentId::new();
        seed_agent_row(&pool, agent_a.as_uuid(), org_a.as_uuid(), workspace_a, user.as_uuid()).await;
        seed_agent_row(&pool, agent_a_participant.as_uuid(), org_a.as_uuid(), workspace_a, user.as_uuid()).await;
        seed_agent_row(&pool, agent_b_participant.as_uuid(), org_b.as_uuid(), workspace_b, user.as_uuid()).await;

        // Org A: an expired-lease working task (assigned, lease in the past).
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, title, status, assigned_agent_id, lease_expires_at, created_by, updated_at) \
             VALUES ($1, $2, 'A task', 'working', $3, NOW() - INTERVAL '1 hour', $4, NOW())",
        )
        .bind(Uuid::new_v4())
        .bind(org_a.as_uuid())
        .bind(agent_a.as_uuid())
        .bind(user.as_uuid())
        .execute(&pool)
        .await
        .expect("seed org A task");

        // Org A: a stale participant (non-offline, old heartbeat) that is also
        // 'busy' but has no matching working task for THAT agent -> also counts as
        // busy_participants_without_work. `participants.name` is NOT NULL.
        sqlx::query(
            "INSERT INTO participants (organization_id, agent_id, name, status, last_heartbeat_at) \
             VALUES ($1, $2, 'org-a-busy', 'busy', NOW() - INTERVAL '2 hours')",
        )
        .bind(org_a.as_uuid())
        .bind(agent_a_participant.as_uuid())
        .execute(&pool)
        .await
        .expect("seed org A participant");

        // Org A: an unpublished assignment outbox row.
        sqlx::query(
            "INSERT INTO orchestration_outbox (id, organization_id, aggregate_type, aggregate_id, event_type, payload, created_at) \
             VALUES ($1, $2, 'task', $3, 'assignment', '{}'::jsonb, NOW() - INTERVAL '30 seconds')",
        )
        .bind(Uuid::new_v4())
        .bind(org_a.as_uuid())
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .expect("seed org A outbox");

        // Org B: a FRESH participant + a PUBLISHED outbox row -> must NOT be counted
        // for either org. `participants.name` is NOT NULL.
        sqlx::query(
            "INSERT INTO participants (organization_id, agent_id, name, status, last_heartbeat_at) \
             VALUES ($1, $2, 'org-b-available', 'available', NOW())",
        )
        .bind(org_b.as_uuid())
        .bind(agent_b_participant.as_uuid())
        .execute(&pool)
        .await
        .expect("seed org B participant");
        sqlx::query(
            "INSERT INTO orchestration_outbox (id, organization_id, aggregate_type, aggregate_id, event_type, payload, published_at, created_at) \
             VALUES ($1, $2, 'task', $3, 'assignment', '{}'::jsonb, NOW(), NOW())",
        )
        .bind(Uuid::new_v4())
        .bind(org_b.as_uuid())
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .expect("seed org B outbox");

        let repo = AdminRepository::new(pool.clone());

        let scope_a = crate::test_support::tenant_scope_for_ids(org_a.as_uuid(), user.as_uuid());
        let snap_a = repo.org_control_plane_snapshot(&scope_a, 60).await.expect("snapshot A");
        assert_eq!(snap_a.expired_working_leases, 1, "org A has one expired lease");
        assert_eq!(snap_a.stale_participants, 1, "org A has one stale participant");
        assert_eq!(snap_a.assignment_outbox_backlog, 1, "org A has one unpublished assignment");
        assert!(snap_a.assignment_outbox_oldest_age_seconds > 0.0, "oldest age is positive");
        assert_eq!(snap_a.busy_participants_without_work, 1, "org A busy participant has no working task");
        assert_eq!(
            snap_a.working_tasks_without_busy_participant, 1,
            "org A working task's agent has no busy participant"
        );
        assert_eq!(snap_a.stale_after_seconds, 60);

        let scope_b = crate::test_support::tenant_scope_for_ids(org_b.as_uuid(), user.as_uuid());
        let snap_b = repo.org_control_plane_snapshot(&scope_b, 60).await.expect("snapshot B");
        assert_eq!(snap_b.expired_working_leases, 0, "org B sees none of org A's tasks");
        assert_eq!(snap_b.stale_participants, 0, "org B participant is fresh");
        assert_eq!(snap_b.assignment_outbox_backlog, 0, "org B outbox row is published");
        assert_eq!(snap_b.assignment_outbox_oldest_age_seconds, 0.0);
        assert_eq!(snap_b.working_tasks_without_busy_participant, 0, "org B sees none of org A's working tasks");
        // If ORG_BUSY_WITHOUT_WORK_SQL lost its org filter, org A's busy
        // participant would leak into org B's count here.
        assert_eq!(snap_b.busy_participants_without_work, 0, "org B sees none of org A's busy participants");
    }

    /// Insert one dead_events row. `org` is optional so the cross-org / NULL-org
    /// nature of the table is exercised.
    async fn seed_dead_event(pool: &PgPool, source: &str, reason: &str, subject: &str, org: Option<Uuid>) {
        sqlx::query(
            "INSERT INTO dead_events (source, reason, subject, org_id, payload_excerpt) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(source)
        .bind(reason)
        .bind(subject)
        .bind(org)
        .bind(format!("excerpt for {subject}"))
        .execute(pool)
        .await
        .expect("seed dead event");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn dead_events_paginate_newest_first_and_filter_by_reason(pool: PgPool) {
        let repo = AdminRepository::new(pool.clone());
        let org_a = Uuid::new_v4();

        // Mixed reasons across orgs incl. a NULL-org row (the common pre-auth drop).
        seed_dead_event(&pool, "events.ingest", "signature_mismatch", "events.ingest.cli.a", None).await;
        seed_dead_event(&pool, "events.ingest", "agent_unknown", "events.ingest.cli.b", Some(org_a)).await;
        seed_dead_event(&pool, "orchestration.result", "signature_mismatch", "orchestration.result.c", None).await;
        seed_dead_event(&pool, "orchestration.result", "bad_subject", "orchestration.result.d", Some(org_a)).await;

        // Unfiltered count + cross-org visibility (a NULL-org row is included).
        assert_eq!(repo.count_dead_events(None).await.expect("count all"), 4);
        let all = repo.list_dead_events(50, 0, None).await.expect("list all");
        assert_eq!(all.len(), 4);
        assert!(all.iter().any(|r| r.org_id.is_none()), "a NULL-org drop is listed");

        // Newest first: the last seeded row (bad_subject) sorts first.
        assert_eq!(all[0].reason, "bad_subject");

        // Reason filter (bound, exact).
        assert_eq!(repo.count_dead_events(Some("signature_mismatch")).await.expect("count filtered"), 2);
        let filtered = repo.list_dead_events(50, 0, Some("signature_mismatch")).await.expect("list filtered");
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|r| r.reason == "signature_mismatch"));

        // Blank reason behaves like "no filter".
        assert_eq!(repo.count_dead_events(Some("   ")).await.expect("blank count"), 4);

        // Pagination: limit 1 returns the newest, offset 1 the next.
        let page0 = repo.list_dead_events(1, 0, None).await.expect("page 0");
        let page1 = repo.list_dead_events(1, 1, None).await.expect("page 1");
        assert_eq!(page0.len(), 1);
        assert_eq!(page1.len(), 1);
        assert_ne!(page0[0].id, page1[0].id, "pages do not overlap");
    }
}
