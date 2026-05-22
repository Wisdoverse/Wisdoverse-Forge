//! Orchestration aggregate — task, participant, task run, context injection,
//! task context, and context link repositories. Tenant-scoped queries for
//! tasks and participants.
//!
//! All task queries must include `WHERE organization_id = $N` so cross-tenant
//! reads are impossible by construction.

pub mod context_link;
pub mod run_context_injection;
pub mod task_context;
pub mod task_run;

pub use context_link::{ContextLinkRepository, ContextLinkedRunRow, CreateContextLinkRecord};
pub use run_context_injection::{ContextAppliedRunRow, ContextInjectionCounts, RunContextInjectionRepository};
pub use task_context::{AppliedContextRow, TaskContextRepository};
pub use task_run::{RunEvidenceRow, TaskRunRepository};

use agentforge_core::{AgentId, AppResult, TenantScope, UserId};
use agentforge_db::entities::{OrchestrationTask, Participant};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

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

/// `count_by_status`: tenant-scoped participant counts that exclude stale
/// `offline` rows (heartbeat older than 24h). The 24h window protects the
/// pool-status hint from showing phantom participants — see
/// `test_count_by_status_sql_excludes_stale_offline`.
pub(crate) const PARTICIPANT_COUNT_SQL: &str = r#"SELECT status, COUNT(*) AS n FROM participants
               WHERE organization_id = $1
                 AND (status <> 'offline'
                      OR (last_heartbeat_at IS NOT NULL
                          AND last_heartbeat_at > NOW() - INTERVAL '24 hours'))
               GROUP BY status"#;

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
                  blocked_reason, blocked_metadata, requires_approval)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                  CASE WHEN $5 = 'working' THEN NOW() ELSE NULL END,
                  $11, $12, $13)
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

    /// Find the next dispatchable task — `queued` or `blocked-on-agent` only.
    /// Backlog is excluded by design: it represents draft tasks the user has not
    /// promoted yet, so the auto-pickup loop must not silently start them.
    /// Highest priority first, FIFO within priority.
    pub async fn next_dispatchable(&self, scope: &TenantScope) -> AppResult<Option<OrchestrationTask>> {
        let task = sqlx::query_as::<_, OrchestrationTask>(
            r#"SELECT * FROM orchestration_tasks
               WHERE organization_id = $1
                 AND status IN ('queued', 'blocked')
                 AND (blocked_reason IS NULL OR blocked_reason = 'waiting_agent')
                 AND assigned_agent_id IS NULL
               ORDER BY
                 CASE priority
                   WHEN 'urgent' THEN 0
                   WHEN 'high'   THEN 1
                   WHEN 'normal' THEN 2
                   WHEN 'low'    THEN 3
                   ELSE 4
                 END,
                 created_at ASC
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
        sqlx::query_as::<_, Participant>(
            r#"INSERT INTO participants (organization_id, agent_id, name, capabilities)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (organization_id, agent_id) DO UPDATE
               SET name = EXCLUDED.name, capabilities = EXCLUDED.capabilities, status = 'available'
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(agent_id.as_uuid())
        .bind(name)
        .bind(capabilities)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// List participants with optional status filter (tenant-scoped).
    pub async fn list(&self, scope: &TenantScope, status: Option<&str>) -> AppResult<Vec<Participant>> {
        let participants = match status {
            Some(s) => {
                sqlx::query_as::<_, Participant>(
                    r#"SELECT * FROM participants
                       WHERE organization_id = $1 AND status = $2
                       ORDER BY registered_at DESC"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(s)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, Participant>(
                    r#"SELECT * FROM participants
                       WHERE organization_id = $1
                       ORDER BY registered_at DESC"#,
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
    pub async fn count_by_status(&self, scope: &TenantScope) -> AppResult<(i64, i64, i64)> {
        let rows: Vec<(String, i64)> =
            sqlx::query_as(PARTICIPANT_COUNT_SQL).bind(scope.org_id().as_uuid()).fetch_all(&self.pool).await?;
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

    pub async fn find_by_agent_id_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        agent_id: AgentId,
    ) -> AppResult<Participant> {
        sqlx::query_as::<_, Participant>("SELECT * FROM participants WHERE agent_id = $1 AND organization_id = $2")
            .bind(agent_id.as_uuid())
            .bind(scope.org_id().as_uuid())
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| OrchestrationRepositoryPolicy::participant_not_found(agent_id))
    }

    /// Find first available participant (tenant-scoped).
    pub async fn find_available(&self, scope: &TenantScope) -> AppResult<Option<Participant>> {
        let participant = sqlx::query_as::<_, Participant>(
            r#"SELECT * FROM participants
               WHERE organization_id = $1 AND status = 'available'
               ORDER BY last_heartbeat_at DESC NULLS LAST
               LIMIT 1"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?;
        Ok(participant)
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
