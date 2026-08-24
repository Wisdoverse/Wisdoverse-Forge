//! Orchestration aggregate — task, participant, task run, context injection,
//! task context, and context link repositories. Tenant-scoped queries for
//! tasks and participants.
//!
//! All task queries must include `WHERE organization_id = $N` so cross-tenant
//! reads are impossible by construction.

pub mod context_link;
#[cfg(test)]
mod dependency_tests;
#[cfg(test)]
mod retire_stale_tests;
pub mod run_context_injection;
pub mod task_comment;
pub mod task_context;
pub mod task_review_check;
pub mod task_run;
#[cfg(test)]
mod task_wait_tests;

pub use context_link::{ContextLinkRepository, ContextLinkedRunRow, CreateContextLinkRecord};
pub use run_context_injection::{ContextAppliedRunRow, ContextInjectionCounts, RunContextInjectionRepository};
pub use task_comment::{HumanMarkerRow, TaskCommentRepository, TaskCommentWithAuthorRow};
pub use task_context::{AppliedContextRow, TaskContextRepository};
pub use task_review_check::{TaskReviewCheckRepository, TaskReviewCheckRow};
pub use task_run::{RunEvidenceRow, TaskRunRepository};

use agentforge_core::{AgentId, AppResult, TenantScope, UserId};
use agentforge_db::entities::{OrchestrationTask, Participant};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::agent::AgentRepositoryPolicy;
use crate::domain::orchestration::OrchestrationRepositoryPolicy;

// ---------------------------------------------------------------------------
// SQL constants exposed for query-shape unit tests (issue #35) AND shared
// between the pool-bound repository methods and their `_in_tx` siblings
// (issue #37). Centralising means transactional and non-tx callers cannot
// drift, and the query-shape unit tests keep guarding the WHERE-clause
// invariants without needing a DB integration harness.
// ---------------------------------------------------------------------------

/// `set_result`: stamp terminal status + result/error on a single task,
/// tenant-scoped. Used by both the pool-bound shim and the in-tx variant
/// (issue #37 transactionalization of `complete_task`).
/// Median completed-task duration query (30-day window appended at call
/// site; widened to all history when the recent window is empty).
pub(crate) const TYPICAL_WAIT_SQL: &str = concat!(
    "SELECT COALESCE(percentile_cont(0.5) WITHIN GROUP ",
    "(ORDER BY EXTRACT(EPOCH FROM (completed_at - started_at))), 0)::float8 ",
    "FROM orchestration_tasks ",
    "WHERE organization_id = $1 AND status = 'completed' ",
    "AND started_at IS NOT NULL AND completed_at IS NOT NULL ",
    "AND (completed_at - started_at) > INTERVAL '5 seconds'"
);

pub(crate) const SET_RESULT_SQL: &str = r#"UPDATE orchestration_tasks
               SET status = $3,
                   result = CASE WHEN $3 = 'completed' THEN $4 ELSE result END,
                   error  = CASE WHEN $3 = 'failed'    THEN $4 ELSE error  END,
                   progress = CASE WHEN $3 = 'completed' THEN 100 ELSE progress END,
                   lease_expires_at = NULL,
                   retryable = FALSE,
                   completed_at = CASE WHEN $3 = 'completed' THEN NOW() ELSE NULL END,
                   updated_at = NOW()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#;

pub(crate) const ASSIGN_TASK_SQL: &str = r#"UPDATE orchestration_tasks
               SET assigned_agent_id = $3,
                   status = 'working',
                   blocked_reason = NULL,
                   blocked_metadata = NULL,
                   started_at = COALESCE(started_at, NOW()),
                   attempt = attempt + 1,
                   lease_expires_at = NOW() + ($5::text || ' seconds')::interval,
                   last_assignment_id = $4,
                   failure_code = NULL,
                   retryable = FALSE,
                   updated_at = NOW()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#;

pub(crate) const MARK_BLOCKED_RETRYABLE_SQL: &str = r#"UPDATE orchestration_tasks
               SET status = 'blocked',
                   assigned_agent_id = NULL,
                   lease_expires_at = NULL,
                   last_assignment_id = NULL,
                   blocked_reason = $3,
                   blocked_metadata = $4,
                   error = $5,
                   failure_code = $3,
                   retryable = TRUE,
                   updated_at = NOW()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#;

pub(crate) const CANCEL_TASK_SQL: &str = r#"UPDATE orchestration_tasks
               SET status = 'canceled',
                   lease_expires_at = NULL,
                   retryable = FALSE,
                   canceled_at = NOW(),
                   updated_at = NOW()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#;

/// `unblock_children_of`: flip dependency-blocked children of a completed
/// parent back to `queued`. Tenant + parent + status + reason guards are all
/// load-bearing — see `test_unblock_children_sql_has_tenant_and_status_guards`.
pub(crate) const UNBLOCK_CHILDREN_SQL: &str = r#"UPDATE orchestration_tasks
               SET status = 'queued',
                   blocked_reason = NULL,
                   blocked_metadata = NULL,
                   updated_at = NOW()
               WHERE organization_id = $1
                 AND parent_task_id = $2
                 AND status = 'blocked'
                 AND blocked_reason = 'waiting_dependency'
               RETURNING *"#;

/// `count_by_status`: task-route-scoped participant counts that exclude stale
/// `offline` rows (heartbeat older than 24h). A participant only counts when its
/// live agent can execute inside the task project's workspace and advertises at
/// least one non-empty task capability.
pub(crate) const PARTICIPANT_COUNT_SQL: &str = r#"SELECT participant.status, COUNT(*) AS n
                 FROM participants participant
                 JOIN agents agent
                   ON agent.id = participant.agent_id
                  AND agent.organization_id = participant.organization_id
                 JOIN orchestration_tasks task
                   ON task.id = $2
                  AND task.organization_id = $1
                 JOIN groups task_group
                   ON task_group.id = task.group_id
                  AND task_group.organization_id = task.organization_id
                  AND task_group.deleted_at IS NULL
                 JOIN projects task_project
                   ON task_project.id = task_group.project_id
                  AND task_project.organization_id = task.organization_id
                  AND task_project.deleted_at IS NULL
                 JOIN workspaces task_workspace
                   ON task_workspace.id = task_project.workspace_id
                  AND task_workspace.organization_id = task.organization_id
                  AND task_workspace.deleted_at IS NULL
                WHERE participant.organization_id = $1
                  AND agent.workspace_id = task_project.workspace_id
                  AND EXISTS (
                        SELECT 1
                          FROM unnest(participant.capabilities) capability
                         WHERE btrim(capability) <> ''
                      )
                  AND (participant.status <> 'offline'
                       OR (participant.last_heartbeat_at IS NOT NULL
                           AND participant.last_heartbeat_at > NOW() - INTERVAL '24 hours'))
                GROUP BY participant.status"#;

/// One task-history export row (compliance export).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct TaskHistoryExportRow {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub progress: i16,
    pub creator_name: Option<String>,
    pub assigned_agent_name: Option<String>,
    pub runs_count: i64,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub blocked_reason: Option<String>,
    pub requires_approval: bool,
}

/// Snapshot of orchestration task counts grouped by kanban state.
/// Returned to the UI as `{ byState: { backlog, queued, working, blocked, ... } }`.
#[derive(Debug, Clone, Default)]
pub struct OrchestrationTaskStats {
    pub backlog: i64,
    pub queued: i64,
    pub working: i64,
    pub blocked: i64,
    pub completed: i64,
    pub failed: i64,
    pub canceled: i64,
}

/// Database access layer for orchestration tasks. All queries enforce tenant
/// isolation via `WHERE organization_id = $N`.
pub struct OrchestrationTaskRepository {
    pool: PgPool,
}

/// A waiting (queued) task's dispatch-order key — the queue snapshot used by
/// queued-time prediction. Order matches the real dispatch order.
#[derive(Debug, Clone, FromRow)]
pub struct QueuedTaskKey {
    pub id: Uuid,
    pub assigned_agent_id: Option<AgentId>,
    pub priority: String,
    pub created_at: DateTime<Utc>,
}

/// Fields used to insert a new orchestration task.
///
/// `initial_blocked_reason` and `initial_blocked_metadata` are stamped inside
/// the same INSERT as `initial_status` so a row never exists in the DB with
/// `status='blocked'` and `blocked_reason=NULL` — that combination would leak
/// through the `next_dispatchable` gate and silently bypass the blocked lane.
#[derive(Debug, Clone, Default)]
pub struct CreateTaskRow<'a> {
    pub group_id: Option<Uuid>,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub priority: &'a str,
    pub params: Option<&'a serde_json::Value>,
    pub assigned_agent_id: Option<AgentId>,
    pub parent_task_id: Option<Uuid>,
    pub initial_status: &'a str,
    pub initial_blocked_reason: Option<&'a str>,
    pub initial_blocked_metadata: Option<serde_json::Value>,
    pub requires_approval: bool,
    /// Marks a self-fix code task against this repo. Only this column is set at
    /// create time; the `pr_*` / `base_commit_sha` / `review_status` columns are
    /// written later via the dedicated UPDATE methods, so they stay NULL here.
    pub self_fix: bool,
}

/// Fields a PATCH `/tasks/:id` request can update. `None` leaves the column unchanged.
#[derive(Debug, Clone, Default)]
pub struct UpdateTaskRow {
    pub status: Option<String>,
    pub priority: Option<String>,
    pub progress: Option<i16>,
    pub assigned_agent_id: Option<Option<AgentId>>, // outer Some = touch field, inner None = unassign
    pub blocked_reason: Option<Option<String>>,
    pub blocked_metadata: Option<Option<serde_json::Value>>,
}

impl OrchestrationTaskRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Borrow the underlying pool so service-layer callers can open their own
    /// transaction. Required by `complete_task` (issue #37): it needs to wrap
    /// `set_result` + `unblock_children_of` in a single `Transaction` so a
    /// crashed unblock can't leave children stuck on `waiting_dependency`.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Queued (waiting) tasks of the org in real dispatch order — priority
    /// (urgent..normal) then age. The basis for queued-time prediction.
    pub async fn queued_tasks_ordered(&self, scope: &TenantScope) -> AppResult<Vec<QueuedTaskKey>> {
        sqlx::query_as::<_, QueuedTaskKey>(
            r#"SELECT id, assigned_agent_id, priority, created_at FROM orchestration_tasks
               WHERE organization_id = $1 AND status = 'queued'
               ORDER BY CASE priority
                          WHEN 'urgent' THEN 0 WHEN 'high' THEN 1
                          WHEN 'normal' THEN 2 ELSE 3
                        END, created_at"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Batch-retire stale (never-started) tasks in a group: `backlog`/`queued`
    /// tasks with `progress = 0` untouched for at least `older_days` become
    /// `canceled`. Returns the retired task ids (capped by `batch_limit`).
    pub async fn retire_stale_tasks(
        &self,
        scope: &TenantScope,
        group_id: Uuid,
        older_days: i32,
        batch_limit: i64,
    ) -> AppResult<Vec<Uuid>> {
        let ids: Vec<Uuid> = sqlx::query_scalar::<_, Uuid>(
            r#"UPDATE orchestration_tasks
                 SET status = 'canceled',
                     canceled_at = NOW(),
                     progress = 0,
                     updated_at = NOW()
               WHERE id IN (
                   SELECT id FROM orchestration_tasks
                    WHERE organization_id = $1 AND group_id = $2
                      AND status IN ('backlog', 'queued')
                      AND progress = 0
                      AND updated_at < NOW() - ($3 || ' days')::interval
                    ORDER BY updated_at ASC
                    LIMIT $4
               )
               RETURNING id"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(group_id)
        .bind(older_days)
        .bind(batch_limit)
        .fetch_all(&self.pool)
        .await
        .map_err(agentforge_core::AppError::from)?;
        Ok(ids)
    }

    /// Median completed-task duration (seconds) for this org: the last 30 days
    /// first, widening to all history when the recent window is empty.
    pub async fn typical_wait_seconds(&self, scope: &TenantScope) -> AppResult<Option<u32>> {
        for window in [" AND completed_at > NOW() - INTERVAL '30 days'", ""] {
            let sql = format!("{TYPICAL_WAIT_SQL}{window}");
            let median: f64 = sqlx::query_scalar(&sql).bind(scope.org_id().as_uuid()).fetch_one(&self.pool).await?;
            if median > 0.0 {
                return Ok(Some(median.round() as u32));
            }
        }
        Ok(None)
    }

    /// Create a new orchestration task with full kanban metadata.
    pub async fn create(&self, scope: &TenantScope, row: CreateTaskRow<'_>) -> AppResult<OrchestrationTask> {
        let mut tx = self.pool.begin().await?;
        let task = Self::create_in_tx(&mut tx, scope, row).await?;
        tx.commit().await?;
        Ok(task)
    }

    pub async fn create_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        row: CreateTaskRow<'_>,
    ) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(
            r#"INSERT INTO orchestration_tasks
                 (organization_id, group_id, title, description, status, priority, params,
                  created_by, assigned_agent_id, parent_task_id, started_at,
                  blocked_reason, blocked_metadata, requires_approval, self_fix)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                  CASE WHEN $5 = 'working' THEN NOW() ELSE NULL END,
                  $11, $12, $13, $14)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(row.group_id)
        .bind(row.title)
        .bind(row.description)
        .bind(row.initial_status)
        .bind(row.priority)
        .bind(row.params)
        .bind(scope.user_id().as_uuid())
        .bind(row.assigned_agent_id.map(|id| id.as_uuid()))
        .bind(row.parent_task_id)
        .bind(row.initial_blocked_reason)
        .bind(row.initial_blocked_metadata)
        .bind(row.requires_approval)
        .bind(row.self_fix)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    /// List tasks with optional status filter (tenant-scoped).
    pub async fn list(
        &self,
        scope: &TenantScope,
        status: Option<&str>,
        agent_id: Option<AgentId>,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<OrchestrationTask>> {
        // `$3 = ''` lets us pass an unused status sentinel without changing the SQL shape.
        // `$4::uuid IS NULL` lets us pass NULL to skip the agent filter while keeping a
        // single prepared statement (cheaper plan reuse than four query branches).
        let tasks = sqlx::query_as::<_, OrchestrationTask>(
            r#"SELECT * FROM orchestration_tasks
               WHERE organization_id = $1
                 AND ($2::text IS NULL OR status = $2)
                 AND ($3::uuid IS NULL OR assigned_agent_id = $3)
               ORDER BY created_at DESC
               LIMIT $4 OFFSET $5"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(status)
        .bind(agent_id.map(|a| a.as_uuid()))
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(tasks)
    }

    /// List tasks for a specific group (tenant-scoped + group-scoped).
    /// Returned newest-first so the kanban distributes tasks in a stable order.
    pub async fn list_by_group(
        &self,
        scope: &TenantScope,
        group_id: Uuid,
        status: Option<&str>,
    ) -> AppResult<Vec<OrchestrationTask>> {
        let tasks = match status {
            Some(s) => {
                sqlx::query_as::<_, OrchestrationTask>(
                    r#"SELECT * FROM orchestration_tasks
                       WHERE organization_id = $1 AND group_id = $2 AND status = $3
                       ORDER BY created_at DESC"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(group_id)
                .bind(s)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, OrchestrationTask>(
                    r#"SELECT * FROM orchestration_tasks
                       WHERE organization_id = $1 AND group_id = $2
                       ORDER BY created_at DESC"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(group_id)
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(tasks)
    }

    /// Compute a per-state task count snapshot for a group.
    pub async fn stats_by_group(&self, scope: &TenantScope, group_id: Uuid) -> AppResult<OrchestrationTaskStats> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            r#"SELECT status, COUNT(*) AS n FROM orchestration_tasks
               WHERE organization_id = $1 AND group_id = $2
               GROUP BY status"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(group_id)
        .fetch_all(&self.pool)
        .await?;

        let mut stats = OrchestrationTaskStats::default();
        for (status, n) in rows {
            match status.as_str() {
                "backlog" => stats.backlog = n,
                "queued" => stats.queued = n,
                "working" => stats.working = n,
                "blocked" => stats.blocked = n,
                "completed" => stats.completed = n,
                "failed" => stats.failed = n,
                "canceled" => stats.canceled = n,
                _ => {}
            }
        }
        Ok(stats)
    }

    /// Find a task by ID (tenant-scoped).
    pub async fn find_by_id(&self, scope: &TenantScope, id: Uuid) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(
            "SELECT * FROM orchestration_tasks WHERE id = $1 AND organization_id = $2",
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(id))
    }

    /// Task history for compliance exports: newest first, tenant-scoped, with
    /// creator / assigned agent names and a run count per task.
    pub async fn export_task_history(&self, scope: &TenantScope, limit: i64) -> AppResult<Vec<TaskHistoryExportRow>> {
        let rows = sqlx::query_as::<_, TaskHistoryExportRow>(
            r#"SELECT t.id, t.title, t.status, t.priority, t.progress,
                      cu.display_name AS creator_name,
                      a.name AS assigned_agent_name,
                      (SELECT COUNT(*)::bigint FROM task_runs r
                        WHERE r.orchestration_task_id = t.id AND r.organization_id = t.organization_id) AS runs_count,
                      t.created_at, t.completed_at, t.updated_at,
                      t.blocked_reason, t.requires_approval
                 FROM orchestration_tasks t
                 LEFT JOIN users cu ON cu.id = t.created_by
                 LEFT JOIN agents a ON a.id = t.assigned_agent_id
                WHERE t.organization_id = $1
                ORDER BY t.created_at DESC
                LIMIT $2"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Update a task's status (tenant-scoped).
    pub async fn update_status(&self, scope: &TenantScope, id: Uuid, status: &str) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(
            r#"UPDATE orchestration_tasks SET status = $3, updated_at = NOW()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(status)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(id))
    }

    /// Mark task `blocked` and record reason + metadata (tenant-scoped).
    pub async fn mark_blocked(
        &self,
        scope: &TenantScope,
        id: Uuid,
        reason: &str,
        metadata: serde_json::Value,
    ) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(
            r#"UPDATE orchestration_tasks
               SET status = 'blocked', blocked_reason = $3, blocked_metadata = $4, updated_at = NOW()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(reason)
        .bind(metadata)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(id))
    }

    /// Mark a working task as retryably blocked and detach it from the current
    /// participant. Used for external limits such as provider quota: the agent
    /// must be released, but the task must not become dispatchable again until
    /// an operator retries it or the missing resource is restored.
    pub async fn mark_blocked_retryable(
        &self,
        scope: &TenantScope,
        id: Uuid,
        reason: &str,
        metadata: serde_json::Value,
        error: serde_json::Value,
    ) -> AppResult<OrchestrationTask> {
        let mut tx = self.pool.begin().await?;
        let task = Self::mark_blocked_retryable_in_tx(&mut tx, scope, id, reason, metadata, error).await?;
        tx.commit().await?;
        Ok(task)
    }

    pub async fn mark_blocked_retryable_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        id: Uuid,
        reason: &str,
        metadata: serde_json::Value,
        error: serde_json::Value,
    ) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(MARK_BLOCKED_RETRYABLE_SQL)
            .bind(id)
            .bind(scope.org_id().as_uuid())
            .bind(reason)
            .bind(metadata)
            .bind(error)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(id))
    }

    /// Approve a task that is explicitly blocked on human approval. The caller
    /// decides whether the next state is `queued` or another blocked reason
    /// such as `waiting_dependency` after checking the current parent state.
    pub async fn approve_waiting_task(
        &self,
        scope: &TenantScope,
        id: Uuid,
        approved_by: UserId,
        next_status: &str,
        next_blocked_reason: Option<&str>,
        next_blocked_metadata: Option<serde_json::Value>,
    ) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(
            r#"UPDATE orchestration_tasks
               SET status = $4,
                   blocked_reason = $5,
                   blocked_metadata = $6,
                   requires_approval = FALSE,
                   approved_at = NOW(),
                   approved_by = $3,
                   updated_at = NOW()
               WHERE id = $1
                 AND organization_id = $2
                 AND status = 'blocked'
                 AND blocked_reason = 'waiting_approval'
                 AND requires_approval = TRUE
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(approved_by.as_uuid())
        .bind(next_status)
        .bind(next_blocked_reason)
        .bind(next_blocked_metadata)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::approval_blocked_task_not_found(id))
    }

    /// Apply a partial PATCH update. `None` fields are left untouched.
    pub async fn patch(&self, scope: &TenantScope, id: Uuid, update: UpdateTaskRow) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(
            r#"UPDATE orchestration_tasks SET
                 status = COALESCE($3, status),
                 priority = COALESCE($4, priority),
                 progress = COALESCE($5, progress),
                 assigned_agent_id = CASE WHEN $6 THEN $7 ELSE assigned_agent_id END,
                 blocked_reason = CASE WHEN $8 THEN $9 ELSE blocked_reason END,
                 blocked_metadata = CASE WHEN $10 THEN $11 ELSE blocked_metadata END,
                 updated_at = NOW()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(update.status)
        .bind(update.priority)
        .bind(update.progress)
        .bind(update.assigned_agent_id.is_some())
        .bind(update.assigned_agent_id.flatten().map(|a| a.as_uuid()))
        .bind(update.blocked_reason.is_some())
        .bind(update.blocked_reason.flatten())
        .bind(update.blocked_metadata.is_some())
        .bind(update.blocked_metadata.flatten())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(id))
    }

    /// Assign an agent to a task and atomically promote it from queued → working.
    pub async fn assign_agent(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        agent_id: AgentId,
        delivery_id: Uuid,
        lease_secs: i64,
    ) -> AppResult<OrchestrationTask> {
        let mut tx = self.pool.begin().await?;
        let task = Self::assign_agent_in_tx(&mut tx, scope, task_id, agent_id, delivery_id, lease_secs).await?;
        tx.commit().await?;
        Ok(task)
    }

    pub async fn assign_agent_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        task_id: Uuid,
        agent_id: AgentId,
        delivery_id: Uuid,
        lease_secs: i64,
    ) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(ASSIGN_TASK_SQL)
            .bind(task_id)
            .bind(scope.org_id().as_uuid())
            .bind(agent_id.as_uuid())
            .bind(delivery_id)
            .bind(lease_secs.to_string())
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(task_id))
    }

    /// Set a task's result and terminal status (tenant-scoped).
    pub async fn set_result(
        &self,
        scope: &TenantScope,
        id: Uuid,
        status: &str,
        result: serde_json::Value,
    ) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(SET_RESULT_SQL)
            .bind(id)
            .bind(scope.org_id().as_uuid())
            .bind(status)
            .bind(result)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(id))
    }

    /// Same as [`Self::set_result`] but inside a caller-owned transaction.
    /// The caller commits or rolls back. Issue #37: lets `complete_task` pair
    /// `set_result` + `unblock_children_of` atomically so a crashed unblock
    /// can't leave dependency-blocked children orphaned forever.
    pub async fn set_result_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        id: Uuid,
        status: &str,
        result: serde_json::Value,
    ) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(SET_RESULT_SQL)
            .bind(id)
            .bind(scope.org_id().as_uuid())
            .bind(status)
            .bind(result)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(id))
    }

    /// Pin the base `origin/main` SHA a self-fix task's PR is rebuilt onto
    /// (tenant-scoped). Written at dispatch by the PR Bridge.
    pub async fn set_base_commit_sha(&self, scope: &TenantScope, id: Uuid, sha: &str) -> AppResult<()> {
        sqlx::query("UPDATE orchestration_tasks SET base_commit_sha = $1, updated_at = NOW() WHERE id = $2 AND organization_id = $3")
            .bind(sha)
            .bind(id)
            .bind(scope.org_id().as_uuid())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Record the draft-PR linkage (number, URL, head SHA) and the initial
    /// review status on a self-fix task (tenant-scoped).
    pub async fn set_pr_metadata(
        &self,
        scope: &TenantScope,
        id: Uuid,
        pr_number: i32,
        pr_url: &str,
        pr_head_sha: &str,
        review_status: &str,
    ) -> AppResult<()> {
        sqlx::query(
            r#"UPDATE orchestration_tasks
               SET pr_number = $1, pr_url = $2, pr_head_sha = $3, review_status = $4,
                   -- Stamp once: never reset the review-window clock if this is re-called
                   -- (e.g. to refresh pr_head_sha after a force-push), so the stuck-review
                   -- reaper deadline cannot be silently extended.
                   review_opened_at = COALESCE(review_opened_at, NOW()), updated_at = NOW()
               WHERE id = $5 AND organization_id = $6"#,
        )
        .bind(pr_number)
        .bind(pr_url)
        .bind(pr_head_sha)
        .bind(review_status)
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update only the review status on a self-fix task (tenant-scoped). Mirrors
    /// the orchestrator `ReviewState` vocabulary but is driven API-side.
    pub async fn set_review_status(&self, scope: &TenantScope, id: Uuid, status: &str) -> AppResult<()> {
        sqlx::query("UPDATE orchestration_tasks SET review_status = $1, updated_at = NOW() WHERE id = $2 AND organization_id = $3")
            .bind(status)
            .bind(id)
            .bind(scope.org_id().as_uuid())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Increment the merge-attempt counter on a self-fix task (tenant-scoped).
    ///
    /// Called immediately before each `run_merge_executor` invocation so every
    /// attempt is counted even when the executor fails. The increment is
    /// unconditional (no CAS); the cap check happens BEFORE this call, so this
    /// only runs when the attempt is within the allowed budget.
    pub async fn bump_merge_attempts(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        sqlx::query(
            "UPDATE orchestration_tasks \
             SET merge_attempts = merge_attempts + 1, updated_at = NOW() \
             WHERE id = $1 AND organization_id = $2",
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Cancel a task (sets status='canceled' and records timestamp). Idempotent.
    pub async fn cancel(&self, scope: &TenantScope, id: Uuid) -> AppResult<OrchestrationTask> {
        let mut tx = self.pool.begin().await?;
        let task = Self::cancel_in_tx(&mut tx, scope, id).await?;
        tx.commit().await?;
        Ok(task)
    }

    pub async fn cancel_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        id: Uuid,
    ) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(CANCEL_TASK_SQL)
            .bind(id)
            .bind(scope.org_id().as_uuid())
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(id))
    }

    /// Reset a terminal task back to backlog so it can be redispatched. Clears
    /// assignment, error, and timing so the task looks new to the dispatcher.
    pub async fn retry(&self, scope: &TenantScope, id: Uuid) -> AppResult<OrchestrationTask> {
        sqlx::query_as::<_, OrchestrationTask>(
            r#"UPDATE orchestration_tasks
               SET status = 'backlog',
                   assigned_agent_id = NULL,
                   lease_expires_at = NULL,
                   last_assignment_id = NULL,
                   failure_code = NULL,
                   retryable = FALSE,
                   started_at = NULL,
                   completed_at = NULL,
                   canceled_at = NULL,
                   error = NULL,
                   progress = 0,
                   blocked_reason = NULL,
                   blocked_metadata = NULL,
                   updated_at = NOW()
               WHERE id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(id))
    }

    /// Record the workspace an image task's images were materialized into (so the
    /// cleanup sweeper can find them without the assigned-agent row) and clear any
    /// prior cleanup mark so a re-materialized (retried) task's new images are
    /// eligible again. Best-effort, called at dispatch when image_paths are set.
    /// Persist the workspace a task's instruction images were materialized into
    /// (so the cleanup sweeper finds them even after the assigned agent is
    /// deleted) and clear any prior cleanup marker so a retried task's fresh images
    /// are eligible again. Runs INSIDE the dispatch transaction: the caller already
    /// holds this task's row lock via `assign_agent_in_tx`, so a separate-connection
    /// UPDATE would self-deadlock against the open tx — and holding the lock here is
    /// also what serialises this re-materialize against the sweeper's row-locked
    /// removal. `$3` = workspace id.
    pub async fn set_task_images_workspace_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        id: Uuid,
        workspace_id: Uuid,
    ) -> AppResult<()> {
        sqlx::query(
            "UPDATE orchestration_tasks
                SET task_images_workspace_id = $3,
                    task_images_cleaned_at = NULL,
                    task_images_retry_after = NULL
              WHERE id = $1 AND organization_id = $2",
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(workspace_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Unblock children that were waiting on the given parent task. Children
    /// marked `blocked/waiting_dependency` for this parent transition to
    /// `queued` so the auto-dispatcher can claim them. Returns the affected
    /// rows so the caller can kick off dispatch per child without another
    /// query.
    pub async fn unblock_children_of(&self, scope: &TenantScope, parent_id: Uuid) -> AppResult<Vec<OrchestrationTask>> {
        let rows = sqlx::query_as::<_, OrchestrationTask>(UNBLOCK_CHILDREN_SQL)
            .bind(scope.org_id().as_uuid())
            .bind(parent_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    /// Same as [`Self::unblock_children_of`] but inside a caller-owned
    /// transaction. Pairs with [`Self::set_result_in_tx`] so `complete_task`
    /// can atomically commit "parent completed AND children unblocked" or
    /// roll both back together. Issue #37.
    pub async fn unblock_children_of_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        parent_id: Uuid,
    ) -> AppResult<Vec<OrchestrationTask>> {
        let rows = sqlx::query_as::<_, OrchestrationTask>(UNBLOCK_CHILDREN_SQL)
            .bind(scope.org_id().as_uuid())
            .bind(parent_id)
            .fetch_all(&mut **tx)
            .await?;
        Ok(rows)
    }

    /// Blocked tasks that declare the completed task in `params.dependency_ids`
    /// (candidates for prerequisite release; the service re-checks siblings).
    pub async fn dependent_task_ids(&self, scope: &TenantScope, completed_id: Uuid) -> AppResult<Vec<Uuid>> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            r#"SELECT id FROM orchestration_tasks
               WHERE organization_id = $1
                 AND status = 'blocked'
                 AND assigned_agent_id IS NULL
                 AND params->'dependency_ids' ? $2::text"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(completed_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Find the next dispatchable task — `queued` or `blocked-on-agent` only.
    /// Backlog is excluded by design: it represents draft tasks the user has not
    /// promoted yet, so the auto-pickup loop must not silently start them.
    /// Highest priority first, FIFO within priority.
    pub async fn next_dispatchable(&self, scope: &TenantScope) -> AppResult<Option<OrchestrationTask>> {
        let task = sqlx::query_as::<_, OrchestrationTask>(
            r#"SELECT task.*
                 FROM orchestration_tasks task
                 JOIN groups task_group
                   ON task_group.id = task.group_id
                  AND task_group.organization_id = task.organization_id
                  AND task_group.deleted_at IS NULL
                 JOIN projects task_project
                   ON task_project.id = task_group.project_id
                  AND task_project.organization_id = task.organization_id
                  AND task_project.deleted_at IS NULL
                 JOIN workspaces task_workspace
                   ON task_workspace.id = task_project.workspace_id
                  AND task_workspace.organization_id = task.organization_id
                  AND task_workspace.deleted_at IS NULL
                WHERE task.organization_id = $1
                 AND task.status IN ('queued', 'blocked')
                 AND (task.blocked_reason IS NULL OR task.blocked_reason = 'waiting_agent')
                 AND task.assigned_agent_id IS NULL
                 -- Image tasks are PUSH-only to an explicitly chosen vision-capable
                 -- container agent (the self-claim/auto-dispatch lanes can't
                 -- materialize images). Excluding them here also keeps a blocked
                 -- image task off the head of the sweep so it can't starve later
                 -- plain queued tasks. Mirrors jobs::NEXT_DISPATCHABLE_SQL.
                 AND COALESCE(
                       CASE WHEN jsonb_typeof(task.params -> 'imageAttachmentIds') = 'array'
                            THEN jsonb_array_length(task.params -> 'imageAttachmentIds')
                            ELSE 0 END,
                       0) = 0
                 AND EXISTS (
                       SELECT 1
                         FROM participants participant
                         JOIN agents agent
                           ON agent.id = participant.agent_id
                          AND agent.organization_id = participant.organization_id
                        WHERE participant.organization_id = task.organization_id
                          AND participant.status = 'available'
                          AND agent.workspace_id = task_project.workspace_id
                          AND EXISTS (
                                SELECT 1
                                  FROM unnest(participant.capabilities) capability
                                 WHERE btrim(capability) <> ''
                              )
                     )
               ORDER BY
                 CASE task.priority
                   WHEN 'urgent' THEN 0
                   WHEN 'high'   THEN 1
                   WHEN 'normal' THEN 2
                   WHEN 'low'    THEN 3
                   ELSE 4
                 END,
                 task.created_at ASC
               LIMIT 1"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?;
        Ok(task)
    }

    /// Resolve assigned agents' display names for a batch of tasks.
    /// Returns a map of agent_id -> name for the agents referenced by the input.
    pub async fn resolve_agent_names(
        &self,
        scope: &TenantScope,
        agent_ids: &[Uuid],
    ) -> AppResult<std::collections::HashMap<Uuid, String>> {
        if agent_ids.is_empty() {
            return Ok(Default::default());
        }
        let rows: Vec<(Uuid, Option<String>)> = sqlx::query_as(
            r#"SELECT id, COALESCE(name, '') AS name FROM agents
               WHERE organization_id = $1 AND id = ANY($2)"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(agent_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id, name)| (id, name.unwrap_or_default())).collect())
    }
}

/// Database access layer for orchestration participants.
pub struct ParticipantRepository {
    pool: PgPool,
}

impl ParticipantRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Register an agent as a participant.
    pub async fn register(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        name: &str,
        capabilities: &[String],
    ) -> AppResult<Participant> {
        // F013: the agent_id must belong to the caller's org. The raw FK only
        // proves the agent exists globally, so `INSERT ... SELECT ... WHERE
        // EXISTS(agent in this org)` rejects registering a foreign-org agent as a
        // participant atomically, keeping orchestration dispatch state free of
        // cross-tenant agent references.
        sqlx::query_as::<_, Participant>(
            r#"INSERT INTO participants (organization_id, agent_id, name, capabilities)
               SELECT $1, $2, $3, $4
               WHERE EXISTS (SELECT 1 FROM agents WHERE id = $2 AND organization_id = $1)
               ON CONFLICT (organization_id, agent_id) DO UPDATE
               SET name = EXCLUDED.name, capabilities = EXCLUDED.capabilities, status = 'available'
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(agent_id.as_uuid())
        .bind(name)
        .bind(capabilities)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AgentRepositoryPolicy::agent_not_found(agent_id))
    }

    /// List participants with optional status filter (tenant-scoped).
    pub async fn list(&self, scope: &TenantScope, status: Option<&str>) -> AppResult<Vec<Participant>> {
        // LEFT JOIN agents so each participant carries its agent's typed
        // `runtime_kind` (used by the task form's image-capability gate). LEFT so
        // a participant whose agent row is missing still lists (runtime_kind NULL).
        let participants = match status {
            Some(s) => {
                sqlx::query_as::<_, Participant>(
                    r#"SELECT participants.*, agents.runtime_kind
                       FROM participants
                       LEFT JOIN agents ON agents.id = participants.agent_id
                       WHERE participants.organization_id = $1 AND participants.status = $2
                       ORDER BY participants.registered_at DESC"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(s)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, Participant>(
                    r#"SELECT participants.*, agents.runtime_kind
                       FROM participants
                       LEFT JOIN agents ON agents.id = participants.agent_id
                       WHERE participants.organization_id = $1
                       ORDER BY participants.registered_at DESC"#,
                )
                .bind(scope.org_id().as_uuid())
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(participants)
    }

    /// Per-status participant counts. Used for the "waiting_agent" blocked hint.
    ///
    /// Liveness filter on the `offline` bucket:
    ///
    /// - `last_heartbeat_at > NOW() - INTERVAL '24 hours'` → counted (recently
    ///   offline, could plausibly come back).
    /// - `last_heartbeat_at` older than 24h → dropped (stale).
    /// - `last_heartbeat_at IS NULL` → dropped (a participant that registered but
    ///   never heartbeat'd is not meaningfully "about to come back").
    ///
    /// `available` / `busy` rows are always counted regardless of heartbeat age.
    pub async fn count_by_status(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<(i64, i64, i64)> {
        let rows: Vec<(String, i64)> = sqlx::query_as(PARTICIPANT_COUNT_SQL)
            .bind(scope.org_id().as_uuid())
            .bind(task_id)
            .fetch_all(&self.pool)
            .await?;
        let (mut available, mut busy, mut offline) = (0, 0, 0);
        for (status, n) in rows {
            match status.as_str() {
                "available" => available = n,
                "busy" => busy = n,
                "offline" => offline = n,
                _ => {}
            }
        }
        Ok((available, busy, offline))
    }

    /// Find a participant by agent_id (tenant-scoped).
    pub async fn find_by_agent_id(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<Participant> {
        let mut tx = self.pool.begin().await?;
        let participant = Self::find_by_agent_id_in_tx(&mut tx, scope, agent_id).await?;
        tx.commit().await?;
        Ok(participant)
    }

    /// Lock participant -> agent so callers can safely acquire task/FK locks
    /// later in the same transaction without reversing the dispatch lock order.
    pub async fn find_by_agent_id_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        agent_id: AgentId,
    ) -> AppResult<Participant> {
        sqlx::query_as::<_, Participant>(
            r#"SELECT participant.*
                 FROM participants participant
                 JOIN agents agent
                   ON agent.id = participant.agent_id
                  AND agent.organization_id = participant.organization_id
                WHERE participant.agent_id = $1
                  AND participant.organization_id = $2
                  FOR UPDATE OF participant, agent"#,
        )
        .bind(agent_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::participant_not_found(agent_id))
    }

    /// Find the best available participant inside a task's live workspace route.
    pub async fn find_available(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<Option<Participant>> {
        let participant = sqlx::query_as::<_, Participant>(
            r#"SELECT participant.*
                 FROM participants participant
                 JOIN agents agent
                   ON agent.id = participant.agent_id
                  AND agent.organization_id = participant.organization_id
                 JOIN orchestration_tasks task
                   ON task.id = $2
                  AND task.organization_id = $1
                 JOIN groups task_group
                   ON task_group.id = task.group_id
                  AND task_group.organization_id = task.organization_id
                  AND task_group.deleted_at IS NULL
                 JOIN projects task_project
                   ON task_project.id = task_group.project_id
                  AND task_project.organization_id = task.organization_id
                  AND task_project.deleted_at IS NULL
                 JOIN workspaces task_workspace
                   ON task_workspace.id = task_project.workspace_id
                  AND task_workspace.organization_id = task.organization_id
                  AND task_workspace.deleted_at IS NULL
                WHERE participant.organization_id = $1
                  AND participant.status = 'available'
                  AND agent.workspace_id = task_project.workspace_id
                  AND EXISTS (
                        SELECT 1
                          FROM unnest(participant.capabilities) capability
                         WHERE btrim(capability) <> ''
                      )
               ORDER BY CASE WHEN agent.project_id = task_project.id THEN 0 ELSE 1 END,
                        CASE WHEN participant.last_heartbeat_at IS NULL THEN 1 ELSE 0 END,
                        participant.last_heartbeat_at DESC
               LIMIT 1"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(participant)
    }

    /// Atomically reserve a participant for a task while revalidating the live
    /// task route and the caller's task snapshot. Locks participant -> agent ->
    /// task so API and jobs claims share one order; snapshot drift fails closed.
    pub async fn claim_for_task_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        expected_task: &OrchestrationTask,
        agent_id: AgentId,
    ) -> AppResult<Participant> {
        sqlx::query_as::<_, Participant>(
            r#"WITH locked_participant AS MATERIALIZED (
                   SELECT participant.id,
                          agent.workspace_id AS agent_workspace_id
                     FROM participants participant
                     JOIN agents agent
                       ON agent.id = participant.agent_id
                      AND agent.organization_id = participant.organization_id
                    WHERE participant.agent_id = $3
                      AND participant.organization_id = $1
                      FOR UPDATE OF participant, agent
               ),
               locked_task AS MATERIALIZED (
                   SELECT task.id,
                          task.organization_id,
                          task.group_id
                     FROM orchestration_tasks task
                     JOIN locked_participant locked ON TRUE
                    WHERE task.id = $2
                      AND task.organization_id = $1
                      AND task.status = $4
                      AND task.blocked_reason IS NOT DISTINCT FROM $5
                      AND task.assigned_agent_id IS NOT DISTINCT FROM $6
                      FOR UPDATE OF task
               )
               UPDATE participants participant
                  SET status = 'busy'
                 FROM locked_participant locked,
                      locked_task task,
                      groups task_group,
                      projects task_project,
                      workspaces task_workspace
                WHERE participant.agent_id = $3
                  AND participant.organization_id = $1
                  AND participant.status = 'available'
                  AND participant.id = locked.id
                  AND task.id = $2
                  AND task.organization_id = participant.organization_id
                  AND task_group.id = task.group_id
                  AND task_group.organization_id = task.organization_id
                  AND task_group.deleted_at IS NULL
                  AND task_project.id = task_group.project_id
                  AND task_project.organization_id = task.organization_id
                  AND task_project.deleted_at IS NULL
                  AND task_workspace.id = task_project.workspace_id
                  AND task_workspace.organization_id = task.organization_id
                  AND task_workspace.deleted_at IS NULL
                  AND locked.agent_workspace_id = task_project.workspace_id
                  AND EXISTS (
                        SELECT 1
                          FROM unnest(participant.capabilities) capability
                         WHERE btrim(capability) <> ''
                      )
            RETURNING participant.*"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(expected_task.id)
        .bind(agent_id.as_uuid())
        .bind(&expected_task.status)
        .bind(expected_task.blocked_reason.as_deref())
        .bind(expected_task.assigned_agent_id.map(|assigned| assigned.as_uuid()))
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::participant_not_found(agent_id))
    }

    /// Update participant status (tenant-scoped).
    pub async fn update_status(&self, scope: &TenantScope, agent_id: AgentId, status: &str) -> AppResult<Participant> {
        let mut tx = self.pool.begin().await?;
        let participant = Self::update_status_in_tx(&mut tx, scope, agent_id, status).await?;
        tx.commit().await?;
        Ok(participant)
    }

    pub async fn update_status_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        agent_id: AgentId,
        status: &str,
    ) -> AppResult<Participant> {
        sqlx::query_as::<_, Participant>(
            r#"UPDATE participants SET status = $3
               WHERE agent_id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(agent_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(status)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::participant_not_found(agent_id))
    }

    /// Update heartbeat timestamp (tenant-scoped). Also bumps status to `available`
    /// so a returning agent immediately becomes pickup-eligible.
    pub async fn heartbeat(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<Participant> {
        sqlx::query_as::<_, Participant>(
            r#"UPDATE participants
               SET last_heartbeat_at = NOW(),
                   status = CASE WHEN status = 'offline' THEN 'available' ELSE status END
               WHERE agent_id = $1 AND organization_id = $2
               RETURNING *"#,
        )
        .bind(agent_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| OrchestrationRepositoryPolicy::participant_not_found(agent_id))
    }

    /// Unregister a participant (tenant-scoped).
    pub async fn unregister(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM participants WHERE agent_id = $1 AND organization_id = $2")
            .bind(agent_id.as_uuid())
            .bind(scope.org_id().as_uuid())
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(OrchestrationRepositoryPolicy::participant_not_found(agent_id));
        }
        Ok(())
    }
}

#[cfg(test)]
mod participant_list_runtime_tests {
    use super::*;
    use crate::test_support::tenant_scope_for_ids;
    use uuid::Uuid;

    struct DispatchFixture {
        org_id: Uuid,
        user_id: Uuid,
        workspace_a: Uuid,
        workspace_b: Uuid,
        project_a: Uuid,
        project_a_fallback: Uuid,
        project_b: Uuid,
        group_a: Uuid,
        group_a_fallback: Uuid,
        group_b: Uuid,
    }

    async fn seed_dispatch_fixture(pool: &sqlx::PgPool) -> DispatchFixture {
        let fixture = DispatchFixture {
            org_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            workspace_a: Uuid::new_v4(),
            workspace_b: Uuid::new_v4(),
            project_a: Uuid::new_v4(),
            project_a_fallback: Uuid::new_v4(),
            project_b: Uuid::new_v4(),
            group_a: Uuid::new_v4(),
            group_a_fallback: Uuid::new_v4(),
            group_b: Uuid::new_v4(),
        };
        let team_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Dispatch Org', $2)")
            .bind(fixture.org_id)
            .bind(format!("dispatch-{}", fixture.org_id))
            .execute(pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(fixture.user_id)
            .bind(format!("u-{}@example.com", fixture.user_id))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Dispatch', $3)")
            .bind(team_id)
            .bind(fixture.org_id)
            .bind(format!("dispatch-{team_id}"))
            .execute(pool)
            .await
            .expect("seed team");
        for (workspace_id, name) in [(fixture.workspace_a, "Workspace A"), (fixture.workspace_b, "Workspace B")] {
            sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, $3)")
                .bind(workspace_id)
                .bind(fixture.org_id)
                .bind(name)
                .execute(pool)
                .await
                .expect("seed workspace");
        }
        for (project_id, workspace_id, name) in [
            (fixture.project_a, fixture.workspace_a, "Project A"),
            (fixture.project_a_fallback, fixture.workspace_a, "Project A fallback"),
            (fixture.project_b, fixture.workspace_b, "Project B"),
        ] {
            sqlx::query(
                "INSERT INTO projects (id, organization_id, workspace_id, team_id, name, slug) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(project_id)
            .bind(fixture.org_id)
            .bind(workspace_id)
            .bind(team_id)
            .bind(name)
            .bind(format!("project-{project_id}"))
            .execute(pool)
            .await
            .expect("seed project");
        }
        for (group_id, project_id, name) in [
            (fixture.group_a, fixture.project_a, "Group A"),
            (fixture.group_a_fallback, fixture.project_a_fallback, "Group A fallback"),
            (fixture.group_b, fixture.project_b, "Group B"),
        ] {
            sqlx::query(
                "INSERT INTO groups (id, organization_id, project_id, name, created_by) VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(group_id)
            .bind(fixture.org_id)
            .bind(project_id)
            .bind(name)
            .bind(fixture.user_id)
            .execute(pool)
            .await
            .expect("seed group");
        }
        fixture
    }

    async fn seed_participant(
        pool: &sqlx::PgPool,
        fixture: &DispatchFixture,
        workspace_id: Uuid,
        project_id: Option<Uuid>,
        capabilities: &[&str],
    ) -> Uuid {
        let agent_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, project_id, user_id, name, status) \
             VALUES ($1, $2, $3, $4, $5, 'dispatch-agent', 'idle')",
        )
        .bind(agent_id)
        .bind(fixture.org_id)
        .bind(workspace_id)
        .bind(project_id)
        .bind(fixture.user_id)
        .execute(pool)
        .await
        .expect("seed agent");
        let capabilities: Vec<String> = capabilities.iter().map(|capability| (*capability).to_owned()).collect();
        sqlx::query(
            "INSERT INTO participants (organization_id, agent_id, name, capabilities, status, last_heartbeat_at) \
             VALUES ($1, $2, 'dispatch-agent', $3, 'available', NOW())",
        )
        .bind(fixture.org_id)
        .bind(agent_id)
        .bind(&capabilities)
        .execute(pool)
        .await
        .expect("seed participant");
        agent_id
    }

    async fn seed_task(pool: &sqlx::PgPool, fixture: &DispatchFixture, group_id: Option<Uuid>, title: &str) -> Uuid {
        let task_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, group_id, title, status, priority, created_by) \
             VALUES ($1, $2, $3, $4, 'queued', 'normal', $5)",
        )
        .bind(task_id)
        .bind(fixture.org_id)
        .bind(group_id)
        .bind(title)
        .bind(fixture.user_id)
        .execute(pool)
        .await
        .expect("seed task");
        task_id
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn route_aware_participant_selection_enforces_workspace_capability_and_exact_project(pool: sqlx::PgPool) {
        let fixture = seed_dispatch_fixture(&pool).await;
        let task_id = seed_task(&pool, &fixture, Some(fixture.group_a), "Task A").await;
        let exact_empty = seed_participant(&pool, &fixture, fixture.workspace_a, Some(fixture.project_a), &[]).await;
        let fallback =
            seed_participant(&pool, &fixture, fixture.workspace_a, Some(fixture.project_a_fallback), &["codex"]).await;
        let _cross = seed_participant(&pool, &fixture, fixture.workspace_b, Some(fixture.project_b), &["codex"]).await;
        let repo = ParticipantRepository::new(pool.clone());
        let scope = tenant_scope_for_ids(fixture.org_id, fixture.user_id);

        let selected = repo.find_available(&scope, task_id).await.expect("find fallback").expect("fallback exists");
        assert_eq!(
            selected.agent_id.as_uuid(),
            fallback,
            "empty-capability exact agent must not hide workspace fallback"
        );
        assert_eq!(repo.count_by_status(&scope, task_id).await.expect("route count"), (1, 0, 0));

        sqlx::query("UPDATE participants SET capabilities = ARRAY['codex'] WHERE agent_id = $1")
            .bind(exact_empty)
            .execute(&pool)
            .await
            .expect("enable exact agent");
        let selected = repo.find_available(&scope, task_id).await.expect("find exact").expect("exact exists");
        assert_eq!(selected.agent_id.as_uuid(), exact_empty, "exact project must win over workspace fallback");

        sqlx::query("UPDATE agents SET workspace_id = $1 WHERE id = $2")
            .bind(fixture.workspace_b)
            .bind(exact_empty)
            .execute(&pool)
            .await
            .expect("move exact agent across workspace boundary");
        sqlx::query("UPDATE projects SET deleted_at = NOW() WHERE id = $1")
            .bind(fixture.project_a_fallback)
            .execute(&pool)
            .await
            .expect("soft-delete fallback agent primary project");
        let selected = repo.find_available(&scope, task_id).await.expect("find after move").expect("fallback remains");
        assert_eq!(selected.agent_id.as_uuid(), fallback, "workspace eligibility must ignore a stale primary project");

        sqlx::query("UPDATE participants SET capabilities = '{}' WHERE agent_id = $1")
            .bind(fallback)
            .execute(&pool)
            .await
            .expect("make fallback chat-only");
        assert!(repo.find_available(&scope, task_id).await.expect("find none").is_none());
        assert_eq!(repo.count_by_status(&scope, task_id).await.expect("empty route count"), (0, 0, 0));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn shared_assignment_claim_fails_closed_outside_task_route(pool: sqlx::PgPool) {
        let fixture = seed_dispatch_fixture(&pool).await;
        let task_id = seed_task(&pool, &fixture, Some(fixture.group_a), "Task A").await;
        let cross = seed_participant(&pool, &fixture, fixture.workspace_b, Some(fixture.project_b), &["codex"]).await;
        let empty = seed_participant(&pool, &fixture, fixture.workspace_a, Some(fixture.project_a), &[]).await;
        let fallback =
            seed_participant(&pool, &fixture, fixture.workspace_a, Some(fixture.project_a_fallback), &["codex"]).await;
        let scope = tenant_scope_for_ids(fixture.org_id, fixture.user_id);
        let task_repo = OrchestrationTaskRepository::new(pool.clone());
        let task = task_repo.find_by_id(&scope, task_id).await.expect("load task snapshot");

        for rejected in [cross, empty] {
            let mut tx = pool.begin().await.expect("begin rejected claim");
            assert!(
                ParticipantRepository::claim_for_task_in_tx(&mut tx, &scope, &task, AgentId::from(rejected))
                    .await
                    .is_err(),
                "cross-workspace and empty-capability explicit assignment must fail closed"
            );
            tx.rollback().await.expect("rollback rejected claim");
        }

        let mut tx = pool.begin().await.expect("begin fallback claim");
        let claimed = ParticipantRepository::claim_for_task_in_tx(&mut tx, &scope, &task, AgentId::from(fallback))
            .await
            .expect("same-workspace fallback claim");
        assert_eq!(claimed.agent_id.as_uuid(), fallback);
        assert_eq!(claimed.status, "busy");
        let update_pool = pool.clone();
        let blocked_update = tokio::spawn(async move {
            let mut update_tx = update_pool.begin().await.expect("begin concurrent task update");
            sqlx::query("SET LOCAL lock_timeout = '250ms'").execute(&mut *update_tx).await.expect("set lock timeout");
            sqlx::query("UPDATE orchestration_tasks SET status = 'canceled' WHERE id = $1")
                .bind(task_id)
                .execute(&mut *update_tx)
                .await
        })
        .await
        .expect("join concurrent task update");
        let lock_error = blocked_update.expect_err("claim must hold the task lock until assignment commits");
        assert_eq!(
            lock_error.as_database_error().and_then(|error| error.code()).as_deref(),
            Some("55P03"),
            "concurrent task mutation must wait behind the participant -> agent -> task claim lock order"
        );
        tx.rollback().await.expect("rollback fallback claim");

        let winner = seed_participant(&pool, &fixture, fixture.workspace_a, None, &["codex"]).await;
        let loser = seed_participant(&pool, &fixture, fixture.workspace_a, None, &["codex"]).await;
        let race_task_id = seed_task(&pool, &fixture, Some(fixture.group_a), "Serialized assignment").await;
        let race_task = task_repo.find_by_id(&scope, race_task_id).await.expect("load race task snapshot");
        let mut winner_tx = pool.begin().await.expect("begin winning claim");
        ParticipantRepository::claim_for_task_in_tx(&mut winner_tx, &scope, &race_task, AgentId::from(winner))
            .await
            .expect("winning claim");
        OrchestrationTaskRepository::assign_agent_in_tx(
            &mut winner_tx,
            &scope,
            race_task.id,
            AgentId::from(winner),
            Uuid::now_v7(),
            900,
        )
        .await
        .expect("commit winning assignment");
        winner_tx.commit().await.expect("commit winning claim");

        let mut loser_tx = pool.begin().await.expect("begin losing claim");
        assert!(
            ParticipantRepository::claim_for_task_in_tx(&mut loser_tx, &scope, &race_task, AgentId::from(loser))
                .await
                .is_err(),
            "a serialized second claimant must not overwrite the committed assignee"
        );
        loser_tx.rollback().await.expect("rollback losing claim");

        let service = crate::services::orchestration::OrchestrationService::new(
            OrchestrationTaskRepository::new(pool.clone()),
            ParticipantRepository::new(pool.clone()),
        )
        .with_context_injection_enabled(false);
        for (title, rejected) in [("Cross-workspace assigned create", cross), ("Empty-cap assigned create", empty)] {
            assert!(
                service
                    .create_task(
                        &scope,
                        title,
                        None,
                        None,
                        None,
                        Some(fixture.group_a),
                        Some(AgentId::from(rejected)),
                        None,
                        false,
                    )
                    .await
                    .is_err(),
                "explicit assigned create must use the same route-aware claim"
            );
        }
        let rejected_tasks: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM orchestration_tasks WHERE organization_id = $1 AND title LIKE '%assigned create'",
        )
        .bind(fixture.org_id)
        .fetch_one(&pool)
        .await
        .expect("count rolled-back assigned creates");
        assert_eq!(rejected_tasks, 0, "rejected assigned creates must roll back task insertion");

        let created = service
            .create_task(
                &scope,
                "Same-workspace assigned create",
                None,
                None,
                None,
                Some(fixture.group_a),
                Some(AgentId::from(fallback)),
                Some(task_id),
                false,
            )
            .await
            .expect("same-workspace fallback assigned create");
        assert_eq!(created.assigned_agent_id, Some(AgentId::from(fallback)));
        assert_eq!(created.status, "working");
        assert_eq!(created.parent_task_id, Some(task_id));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn next_dispatchable_skips_unrouteable_and_unserved_workspaces(pool: sqlx::PgPool) {
        let fixture = seed_dispatch_fixture(&pool).await;
        seed_participant(&pool, &fixture, fixture.workspace_b, Some(fixture.project_b), &["codex"]).await;
        let unrouteable = seed_task(&pool, &fixture, None, "No group").await;
        let unserved = seed_task(&pool, &fixture, Some(fixture.group_a), "Workspace A").await;
        let served = seed_task(&pool, &fixture, Some(fixture.group_b), "Workspace B").await;
        sqlx::query("UPDATE orchestration_tasks SET priority = 'urgent', created_at = NOW() - INTERVAL '2 hours' WHERE id = ANY($1)")
            .bind([unrouteable, unserved])
            .execute(&pool)
            .await
            .expect("prioritize blocked routes");

        let repo = OrchestrationTaskRepository::new(pool.clone());
        let scope = tenant_scope_for_ids(fixture.org_id, fixture.user_id);
        let next = repo.next_dispatchable(&scope).await.expect("next dispatchable").expect("served task");
        assert_eq!(next.id, served, "workspace A and unrouteable heads must not starve workspace B");

        sqlx::query("UPDATE groups SET deleted_at = NOW() WHERE id = $1")
            .bind(fixture.group_b)
            .execute(&pool)
            .await
            .expect("soft-delete route");
        assert!(repo.next_dispatchable(&scope).await.expect("deleted route").is_none());
    }

    // The participant list LEFT JOINs agents so each row carries the agent's
    // typed runtime_kind, which the task form uses to gate image upload. Verify
    // the JOIN executes and FromRow maps the joined column onto the
    // `#[sqlx(default)]` field.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn list_surfaces_agent_runtime_kind(pool: sqlx::PgPool) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org_id)
            .bind("Img Org")
            .bind(format!("img-{org_id}"))
            .execute(&pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_id)
            .execute(&pool)
            .await
            .expect("seed workspace");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(&pool)
            .await
            .expect("seed user");
        // Container agent (overrides the 'api' column default) with a CLI tool,
        // satisfying the runtime_kind invariant.
        sqlx::query(
            r#"INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status, cli_tool, runtime_kind)
               VALUES ($1, $2, $2, $3, 'a', 'idle', 'claude', 'container')"#,
        )
        .bind(agent_id)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed agent");

        let repo = ParticipantRepository::new(pool.clone());
        let scope = tenant_scope_for_ids(org_id, user_id);
        repo.register(&scope, AgentId::from(agent_id), "worker", &["claude".to_string()])
            .await
            .expect("register participant");

        let participants = repo.list(&scope, None).await.expect("list participants");
        assert_eq!(participants.len(), 1);
        assert_eq!(
            participants[0].runtime_kind.as_deref(),
            Some("container"),
            "list() must surface the agent's runtime_kind via the JOIN"
        );
    }

    // An image task is push-only to a vision-capable container agent; the
    // auto-dispatch sweep (next_dispatchable) must NEVER pick one, or a blocked
    // image task sits at the head of every sweep and starves later plain tasks.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn next_dispatchable_skips_image_tasks(pool: sqlx::PgPool) {
        let fixture = seed_dispatch_fixture(&pool).await;
        seed_participant(&pool, &fixture, fixture.workspace_a, Some(fixture.project_a), &["codex"]).await;

        // Image task: unassigned + queued + sorted FIRST (urgent) — must be skipped.
        let image_task = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO orchestration_tasks
               (id, organization_id, group_id, title, status, priority, created_by, params, created_at, updated_at)
               VALUES ($1, $2, $3, 'Image', 'queued', 'urgent', $4, $5::jsonb, NOW() - INTERVAL '1 hour', NOW())"#,
        )
        .bind(image_task)
        .bind(fixture.org_id)
        .bind(fixture.group_a)
        .bind(fixture.user_id)
        .bind(serde_json::json!({ "imageAttachmentIds": ["11111111-1111-1111-1111-111111111111"] }))
        .execute(&pool)
        .await
        .expect("seed image task");

        // Plain task: unassigned + queued, lower priority + later.
        let plain_task = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO orchestration_tasks
               (id, organization_id, group_id, title, status, priority, created_by, created_at, updated_at)
               VALUES ($1, $2, $3, 'Plain', 'queued', 'normal', $4, NOW(), NOW())"#,
        )
        .bind(plain_task)
        .bind(fixture.org_id)
        .bind(fixture.group_a)
        .bind(fixture.user_id)
        .execute(&pool)
        .await
        .expect("seed plain task");

        let repo = OrchestrationTaskRepository::new(pool.clone());
        let scope = tenant_scope_for_ids(fixture.org_id, fixture.user_id);
        let next = repo.next_dispatchable(&scope).await.expect("next_dispatchable");
        // Despite urgent priority + earlier created_at, the image task is skipped.
        assert_eq!(next.map(|t| t.id), Some(plain_task), "next_dispatchable must skip image tasks");

        // With ONLY an image task queued, the sweep finds nothing to dispatch.
        sqlx::query("DELETE FROM orchestration_tasks WHERE id = $1")
            .bind(plain_task)
            .execute(&pool)
            .await
            .expect("del");
        let none = repo.next_dispatchable(&scope).await.expect("next_dispatchable again");
        assert!(none.is_none(), "an image-only queue must not auto-dispatch");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn export_task_history_lists_rows_with_names_and_run_counts(pool: sqlx::PgPool) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Export Org', $2)")
            .bind(org_id)
            .bind(format!("export-org-{org_id}"))
            .execute(&pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'Creator')")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(&pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_id)
            .execute(&pool)
            .await
            .expect("seed workspace");
        let task_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, user_id, name)
                     VALUES ($1, $2, $2, $3, 'Audit Agent')",
        )
        .bind(agent_id)
        .bind(org_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("seed agent");
        sqlx::query(
            r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, priority, created_by, assigned_agent_id,
                requires_approval, created_at, updated_at)
               VALUES ($1, $2, 'Audit, me', 'completed', 'high', $3, $4, TRUE, NOW(), NOW())"#,
        )
        .bind(task_id)
        .bind(org_id)
        .bind(user_id)
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("seed task");
        let run_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO task_runs (id, organization_id, workspace_id, orchestration_task_id, agent_id,
                                      idempotency_key, status, started_at)
               VALUES ($1, $2, $2, $3, $4, 'idem-1', 'completed', NOW())"#,
        )
        .bind(run_id)
        .bind(org_id)
        .bind(task_id)
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("seed run");

        let scope = tenant_scope_for_ids(org_id, user_id);
        let other_scope = tenant_scope_for_ids(Uuid::new_v4(), user_id);
        let repo = OrchestrationTaskRepository::new(pool.clone());

        let rows = repo.export_task_history(&scope, 100).await.expect("export");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Audit, me");
        assert_eq!(rows[0].creator_name.as_deref(), Some("Creator"));
        assert_eq!(rows[0].assigned_agent_name.as_deref(), Some("Audit Agent"));
        assert_eq!(rows[0].runs_count, 1);
        assert!(rows[0].requires_approval);

        let cross = repo.export_task_history(&other_scope, 100).await.expect("cross export");
        assert!(cross.is_empty(), "cross-tenant export must be empty");

        let capped = repo.export_task_history(&other_scope, 0).await.expect("zero limit");
        assert!(capped.is_empty(), "zero limit is a safe no-op");
    }
}
