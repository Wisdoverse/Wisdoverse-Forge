//! Orchestration service — business logic for kanban-style A2A task dispatch.
//!
//! End-to-end flow: user drops a task on the kanban → backend persists in
//! `backlog`/`queued` → auto-dispatcher claims an available participant and the
//! task transitions to `working` → completion or failure releases the
//! participant. When no agent is available the task transitions to `blocked`
//! with a `waiting_agent` reason so the UI can render
//! "还差 N 个 agent 才能开工" without a second roundtrip.

use std::{collections::HashMap, sync::Arc};

use agentforge_core::context_envelope::ContextEnvelope;
use agentforge_core::orchestration_protocol::{DEFAULT_ASSIGNMENT_LEASE_SECS, TaskAssignment};
use agentforge_core::{AgentId, AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::{OrchestrationTask, Participant, TaskRun};
use agentforge_db::inbox_notifications::{TaskOwnerNotificationKind, upsert_task_owner_lifecycle_notification_in_tx};
use agentforge_jobs::insert_assignment_outbox_in_tx;
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::domain::orchestration::{
    BlockedTaskPolicy, ParticipantName, ParticipantStatusPolicy, TaskListPage, TaskPatchPolicy, TaskPriority,
    TaskStatusPolicy, TaskTitle,
};
use crate::repositories::orchestration::{
    CreateTaskRow, OrchestrationTaskRepository, OrchestrationTaskStats, ParticipantRepository, UpdateTaskRow,
};
use crate::repositories::run_context_injection::{ContextInjectionCounts, RunContextInjectionRepository};
use crate::repositories::task_run::TaskRunRepository;
use crate::services::context_envelope::ContextEnvelopeService;
use crate::services::context_resolver::{ContextResolverService, ContextTaskSnapshot, ResolvedContext};

/// JSON shape returned to the UI. Mirrors `TaskSummary` in `src/app/api/orchestration.ts`.
#[derive(Debug, Clone, Serialize)]
pub struct TaskSummary {
    pub id: Uuid,
    #[serde(rename = "groupId")]
    pub group_id: Option<Uuid>,
    pub state: String,
    pub method: String,
    pub params: TaskParams,
    pub priority: String,
    pub progress: i16,
    #[serde(rename = "createdBy")]
    pub created_by: Uuid,
    #[serde(rename = "assignedTo", skip_serializing_if = "Option::is_none")]
    pub assigned_to: Option<Uuid>,
    #[serde(rename = "assignedAgentName", skip_serializing_if = "Option::is_none")]
    pub assigned_agent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(rename = "blockedReason", skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
    #[serde(rename = "blockedHint", skip_serializing_if = "Option::is_none")]
    pub blocked_hint: Option<String>,
    #[serde(rename = "blockedMetadata", skip_serializing_if = "Option::is_none")]
    pub blocked_metadata: Option<serde_json::Value>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "completedAt", skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(rename = "contextCounts")]
    pub context_counts: TaskContextCounts,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct TaskContextCounts {
    #[serde(rename = "appliedMemories")]
    pub applied_memories: i64,
    #[serde(rename = "appliedSkills")]
    pub applied_skills: i64,
    pub total: i64,
}

impl From<ContextInjectionCounts> for TaskContextCounts {
    fn from(counts: ContextInjectionCounts) -> Self {
        Self {
            applied_memories: counts.applied_memories,
            applied_skills: counts.applied_skills,
            total: counts.applied_memories + counts.applied_skills,
        }
    }
}

/// `params.task` + `params.message` shape the legacy/A2A clients send.
#[derive(Debug, Clone, Serialize)]
pub struct TaskParams {
    pub task: String,
    pub message: String,
}

/// Kanban-state count snapshot returned by the stats endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct TaskStatsResponse {
    #[serde(rename = "byState")]
    pub by_state: HashMap<String, i64>,
}

impl From<OrchestrationTaskStats> for TaskStatsResponse {
    fn from(s: OrchestrationTaskStats) -> Self {
        let mut by_state = HashMap::new();
        by_state.insert("backlog".into(), s.backlog);
        by_state.insert("queued".into(), s.queued);
        by_state.insert("working".into(), s.working);
        by_state.insert("blocked".into(), s.blocked);
        by_state.insert("completed".into(), s.completed);
        by_state.insert("failed".into(), s.failed);
        by_state.insert("canceled".into(), s.canceled);
        Self { by_state }
    }
}

/// Business logic layer for orchestration operations.
pub struct OrchestrationService {
    task_repo: OrchestrationTaskRepository,
    participant_repo: ParticipantRepository,
    task_run_repo: TaskRunRepository,
    context_resolver: Option<Arc<ContextResolverService>>,
    context_injection_enabled: bool,
}

impl OrchestrationService {
    pub fn new(task_repo: OrchestrationTaskRepository, participant_repo: ParticipantRepository) -> Self {
        let task_run_repo = TaskRunRepository::new(task_repo.pool().clone());
        Self { task_repo, participant_repo, task_run_repo, context_resolver: None, context_injection_enabled: true }
    }

    pub fn with_context_resolver(mut self, context_resolver: Arc<ContextResolverService>) -> Self {
        self.context_resolver = Some(context_resolver);
        self
    }

    pub fn with_context_injection_enabled(mut self, enabled: bool) -> Self {
        self.context_injection_enabled = enabled;
        if !enabled {
            self.context_resolver = None;
        }
        self
    }

    /// Create a new task. If `assigned_to` names a participant, the task starts
    /// in `working` directly. Otherwise unassigned tasks land in `backlog`
    /// unless they are dependency-blocked; only an explicit promotion to
    /// `queued` enters the auto-dispatch lane.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_task(
        &self,
        scope: &TenantScope,
        title: &str,
        description: Option<&str>,
        params: Option<serde_json::Value>,
        priority: Option<&str>,
        group_id: Option<Uuid>,
        assigned_to: Option<AgentId>,
        parent_task_id: Option<Uuid>,
        requires_approval: bool,
    ) -> AppResult<OrchestrationTask> {
        TaskTitle::validate(title)?;
        let priority = TaskPriority::validate(priority.unwrap_or("normal"))?;
        if requires_approval && assigned_to.is_some() {
            return Err(ErrorKind::Validation(
                "requiresApproval tasks cannot be assigned before approval; approve then dispatch".into(),
            )
            .into());
        }
        let missing_inputs = BlockedTaskPolicy::missing_required_inputs(params.as_ref());
        // Parent status gates child creation on waiting_dependency. Only a genuine
        // `NotFound` is remapped to a validation error; infrastructure errors
        // (pool/IO/decode) propagate unchanged so operators see the real failure.
        let parent_status = if let Some(parent_id) = parent_task_id {
            let parent = self.task_repo.find_by_id(scope, parent_id).await.map_err(|err| match err.kind {
                ErrorKind::NotFound(_) => ErrorKind::Validation(format!("parent task {parent_id} not found")).into(),
                _ => err,
            })?;
            Some(parent.status.clone())
        } else {
            None
        };

        if let Some(agent_id) = assigned_to {
            return self
                .create_task_with_assignee(
                    scope,
                    title,
                    description,
                    params,
                    priority,
                    group_id,
                    agent_id,
                    parent_task_id,
                )
                .await;
        }

        // Unassigned tasks land in `backlog` — a draft state that requires an
        // explicit promotion (drag to "Queued" / pick assignee) before dispatch.
        // This matches the kanban's mental model: backlog = "not ready to run yet".
        //
        // Exceptions: missing declared inputs, human approval, or an unfinished
        // parent all start in `blocked` with a concrete reason. Dependency
        // blocks transition to `queued` when the parent completes; approval
        // blocks transition through the explicit approve endpoint.
        let (initial_status, assigned_agent) = if !missing_inputs.is_empty()
            || requires_approval
            || BlockedTaskPolicy::needs_dependency_block(parent_status.as_deref())
        {
            ("blocked", None)
        } else {
            ("backlog", None)
        };

        // The initial block stamps reason + metadata inside the same INSERT as
        // the status so a partial write can never leave `status='blocked'` with
        // a NULL `blocked_reason` — that combination would leak past
        // `next_dispatchable`. Dependency metadata `{pending: 1}` reflects the
        // single-parent schema; multi-upstream blocking needs a dependency table.
        let (initial_blocked_reason, initial_blocked_metadata) = BlockedTaskPolicy::initial_state(
            &missing_inputs,
            requires_approval,
            BlockedTaskPolicy::needs_dependency_block(parent_status.as_deref()),
        );

        let task = self
            .task_repo
            .create(
                scope,
                CreateTaskRow {
                    group_id,
                    title,
                    description,
                    priority,
                    params: params.as_ref(),
                    assigned_agent_id: assigned_agent,
                    parent_task_id,
                    initial_status,
                    initial_blocked_reason,
                    initial_blocked_metadata,
                    requires_approval,
                },
            )
            .await?;

        Ok(task)
    }

    /// List tasks with optional status + agent filters and pagination (org-scoped).
    /// `agent_id` powers the "Tasks" tab on the agent detail page.
    pub async fn list_tasks(
        &self,
        scope: &TenantScope,
        status: Option<&str>,
        agent_id: Option<AgentId>,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<OrchestrationTask>> {
        if let Some(s) = status {
            TaskStatusPolicy::validate_filter(s)?;
        }
        let page = TaskListPage::new(limit, offset);
        self.task_repo.list(scope, status, agent_id, page.limit(), page.offset()).await
    }

    /// List tasks for a specific group (org + group scoped).
    pub async fn list_tasks_by_group(
        &self,
        scope: &TenantScope,
        group_id: Uuid,
        status: Option<&str>,
    ) -> AppResult<Vec<OrchestrationTask>> {
        if let Some(s) = status {
            TaskStatusPolicy::validate_filter(s)?;
        }
        self.task_repo.list_by_group(scope, group_id, status).await
    }

    pub async fn task_stats_by_group(&self, scope: &TenantScope, group_id: Uuid) -> AppResult<TaskStatsResponse> {
        let stats = self.task_repo.stats_by_group(scope, group_id).await?;
        Ok(stats.into())
    }

    /// Get a single task by ID.
    pub async fn get_task(&self, scope: &TenantScope, id: Uuid) -> AppResult<OrchestrationTask> {
        self.task_repo.find_by_id(scope, id).await
    }

    /// Apply a PATCH update. Validates state/priority/progress, validates the
    /// kanban state transition, and re-runs auto-dispatch on backlog→queued.
    pub async fn update_task(
        &self,
        scope: &TenantScope,
        id: Uuid,
        state: Option<String>,
        priority: Option<String>,
        progress: Option<i16>,
        assigned_to: Option<Option<AgentId>>,
    ) -> AppResult<OrchestrationTask> {
        if let Some(ref s) = state {
            TaskStatusPolicy::validate_patch_state(s)?;
        }
        if let Some(ref p) = priority {
            TaskPriority::validate(p)?;
        }
        if let Some(p) = progress
            && !(0..=100).contains(&p)
        {
            return Err(ErrorKind::Validation("progress must be 0-100".into()).into());
        }

        let transition_state = state.as_deref();
        let touches_assignment = TaskPatchPolicy::touches_assignment(&assigned_to);
        let business_transition = TaskPatchPolicy::is_business_transition(transition_state, &assigned_to);
        let current_for_guard = if state.is_some() || touches_assignment {
            Some(self.task_repo.find_by_id(scope, id).await?)
        } else {
            None
        };
        if let Some(current) = current_for_guard.as_ref()
            && current.status == "blocked"
            && !BlockedTaskPolicy::reason_allows_dispatch(current.blocked_reason.as_deref())
            && !matches!(transition_state, Some("canceled"))
        {
            return Err(ErrorKind::Validation(format!(
                "task is blocked on {}; use its unblock path before dispatching",
                current.blocked_reason.as_deref().unwrap_or("unknown")
            ))
            .into());
        }
        if business_transition && (priority.is_some() || progress.is_some()) {
            return Err(ErrorKind::Validation(
                "state/assignment transitions must not be combined with priority/progress edits".into(),
            )
            .into());
        }
        if touches_assignment && !matches!(transition_state, None | Some("working")) {
            return Err(ErrorKind::Validation(
                "assignedTo changes must use dispatch semantics; combine only with state=working or omit state".into(),
            )
            .into());
        }

        match (transition_state, assigned_to) {
            (Some("working"), Some(Some(agent_id))) | (None, Some(Some(agent_id))) => {
                return self.assign_existing_task_to_participant(scope, id, agent_id).await;
            }
            (Some("working"), Some(None)) => {
                return Err(ErrorKind::Validation("cannot unassign while dispatching to working".into()).into());
            }
            (Some("working"), None) => return self.dispatch_task(scope, id).await,
            (Some("completed"), None) => {
                return self.complete_task(scope, id, json!({ "manual": true, "source": "kanban_patch" })).await;
            }
            (Some("failed"), None) => {
                return self
                    .fail_task(
                        scope,
                        id,
                        json!({
                            "message": "manual failure via task patch",
                            "source": "kanban_patch"
                        }),
                    )
                    .await;
            }
            (Some("canceled"), None) => return self.cancel_task(scope, id).await,
            (None, Some(None)) => {
                let current = match current_for_guard.as_ref() {
                    Some(task) => task,
                    None => unreachable!("assignment guard should have loaded current task"),
                };
                if current.status == "working" {
                    return Err(ErrorKind::Validation(
                        "cannot unassign a working task; cancel, complete, or fail it first".into(),
                    )
                    .into());
                }
            }
            _ => {}
        }

        let next_state = state.clone();
        let updated = self
            .task_repo
            .patch(
                scope,
                id,
                UpdateTaskRow {
                    status: state,
                    priority,
                    progress,
                    assigned_agent_id: assigned_to,
                    blocked_reason: None,
                    blocked_metadata: None,
                },
            )
            .await?;

        // Moving back into the dispatchable lane → re-run the auto-dispatcher
        // so an idle agent picks the task up immediately.
        if matches!(next_state.as_deref(), Some("queued") | Some("backlog")) && updated.assigned_agent_id.is_none() {
            return self.try_auto_dispatch(scope, updated).await;
        }
        Ok(updated)
    }

    /// Cancel a task (terminal).
    pub async fn cancel_task(&self, scope: &TenantScope, id: Uuid) -> AppResult<OrchestrationTask> {
        let task = self.task_repo.find_by_id(scope, id).await?;
        let mut tx = self
            .task_repo
            .pool()
            .begin()
            .await
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("begin cancel_task tx: {err}")))?;
        let updated = OrchestrationTaskRepository::cancel_in_tx(&mut tx, scope, id).await?;
        self.task_run_repo.finish_current_in_tx(&mut tx, scope, id, "canceled").await?;
        tx.commit().await.map_err(|err| ErrorKind::Internal(anyhow::anyhow!("commit cancel_task tx: {err}")))?;
        if let Some(agent_id) = task.assigned_agent_id
            && let Err(err) = self.participant_repo.update_status(scope, agent_id, "available").await
        {
            tracing::warn!(error = ?err, agent_id = %agent_id, "Failed to release participant on cancel");
        }
        if let Err(err) = self.sweep_dispatchable(scope).await {
            tracing::error!(error = ?err, task_id = %id, "Post-cancel sweep failed");
        }
        Ok(updated)
    }

    /// Retry a terminal task: reset to backlog and re-attempt dispatch.
    pub async fn retry_task(&self, scope: &TenantScope, id: Uuid) -> AppResult<OrchestrationTask> {
        let task = self.task_repo.find_by_id(scope, id).await?;
        if task.status == "blocked"
            && task.blocked_reason.as_deref() == Some("waiting_approval")
            && task.requires_approval
        {
            return Err(ErrorKind::Validation("approve or cancel approval-blocked tasks before retry".into()).into());
        }
        let reset = self.task_repo.retry(scope, id).await?;
        self.try_auto_dispatch(scope, reset).await
    }

    /// Dispatch a task: find an available participant, assign it, mark running.
    /// Used by both the explicit `POST /tasks/:id/dispatch` endpoint and the
    /// auto-dispatcher invoked on create / heartbeat.
    pub async fn dispatch_task(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<OrchestrationTask> {
        let task = self.task_repo.find_by_id(scope, task_id).await?;

        if !can_enter_dispatch(&task) {
            return Err(ErrorKind::Validation(format!(
                "can only dispatch queued or waiting-agent tasks, current status: {}, blocked reason: {}",
                task.status,
                task.blocked_reason.as_deref().unwrap_or("none")
            ))
            .into());
        }

        let participant =
            self.participant_repo.find_available(scope).await?.ok_or_else(|| -> agentforge_core::AppError {
                ErrorKind::Validation("no available participants for dispatch".into()).into()
            })?;

        self.assign_to_participant(scope, &task, &participant).await
    }

    /// Try to dispatch a task to an available participant. If no participant is
    /// available the task is marked `blocked/waiting_agent` with metadata that
    /// powers the "还差 X 个 agent" hint. Returns the updated task either way.
    async fn try_auto_dispatch(&self, scope: &TenantScope, task: OrchestrationTask) -> AppResult<OrchestrationTask> {
        if !can_enter_dispatch(&task) {
            return Ok(task);
        }
        match self.participant_repo.find_available(scope).await? {
            Some(participant) => self.assign_to_participant(scope, &task, &participant).await,
            None => {
                let (available, busy, offline) = self.participant_repo.count_by_status(scope).await?;
                let metadata = json!({
                    "available": available,
                    "busy": busy,
                    "offline": offline,
                });
                tracing::info!(task_id = %task.id, busy, offline, "No available participant — task blocked on waiting_agent");
                self.task_repo.mark_blocked(scope, task.id, "waiting_agent", metadata).await
            }
        }
    }

    /// Assign a task to a specific participant (mark participant busy first,
    /// revert on failure).
    async fn assign_to_participant(
        &self,
        scope: &TenantScope,
        task: &OrchestrationTask,
        participant: &Participant,
    ) -> AppResult<OrchestrationTask> {
        self.assign_to_participant_with_resolved_context(scope, task, participant, None).await
    }

    async fn assign_to_participant_with_resolved_context(
        &self,
        scope: &TenantScope,
        task: &OrchestrationTask,
        participant: &Participant,
        previewed_context: Option<ResolvedContext>,
    ) -> AppResult<OrchestrationTask> {
        tracing::info!(task_id = %task.id, agent_id = %participant.agent_id, "Dispatching task to participant");

        let mut tx = self
            .task_repo
            .pool()
            .begin()
            .await
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("begin assignment tx: {err}")))?;
        ParticipantRepository::update_status_in_tx(&mut tx, scope, participant.agent_id, "busy").await?;
        let task = match OrchestrationTaskRepository::assign_agent_in_tx(
            &mut tx,
            scope,
            task.id,
            participant.agent_id,
            Uuid::now_v7(),
            DEFAULT_ASSIGNMENT_LEASE_SECS,
        )
        .await
        {
            Ok(task) => task,
            Err(err) => {
                let _ = tx.rollback().await;
                return Err(err);
            }
        };
        let delivery_id = task
            .last_assignment_id
            .ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("task {} missing last_assignment_id", task.id)))?;
        let idempotency_key = delivery_id.to_string();
        let resolved_context = match previewed_context {
            Some(resolved_context) => Some(resolved_context),
            None => match self.resolve_assignment_context(scope, &task, participant).await {
                Ok(resolved_context) => resolved_context,
                Err(err) => {
                    let _ = tx.rollback().await;
                    return Err(err);
                }
            },
        };
        let task_run = match self
            .task_run_repo
            .create_for_assignment_in_tx(
                &mut tx,
                scope,
                &task,
                &idempotency_key,
                capability_profile_from_participant(participant, resolved_context.as_ref()),
            )
            .await
        {
            Ok(task_run) => task_run,
            Err(err) => {
                let _ = tx.rollback().await;
                return Err(err);
            }
        };
        let context_envelope = match self
            .build_assignment_context_envelope(scope, &task, participant.agent_id, &task_run, resolved_context.clone())
            .await
        {
            Ok(context_envelope) => context_envelope,
            Err(err) => {
                let _ = tx.rollback().await;
                return Err(err);
            }
        };
        if let Some(context_envelope) = context_envelope.as_ref() {
            match RunContextInjectionRepository::record_envelope_in_tx(&mut tx, scope, &task_run, context_envelope)
                .await
            {
                Ok(_) => {}
                Err(err) => {
                    let _ = tx.rollback().await;
                    return Err(err);
                }
            }
        }
        let assignment = assignment_from_task(&task, context_envelope)?;
        if let Err(err) = insert_assignment_outbox_in_tx(&mut tx, scope.org_id().as_uuid(), task.id, &assignment).await
        {
            let _ = tx.rollback().await;
            return Err(ErrorKind::Internal(anyhow::anyhow!("insert assignment outbox: {err}")).into());
        }
        tx.commit().await.map_err(|err| ErrorKind::Internal(anyhow::anyhow!("commit assignment tx: {err}")))?;
        Ok(task)
    }

    pub async fn assign_existing_task_to_agent_with_context(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        agent_id: AgentId,
        resolved_context: ResolvedContext,
    ) -> AppResult<OrchestrationTask> {
        let task = self.task_repo.find_by_id(scope, task_id).await?;
        if !can_enter_dispatch(&task) {
            return Err(ErrorKind::Validation(format!(
                "can only dispatch queued or waiting-agent tasks, current status: {}, blocked reason: {}",
                task.status,
                task.blocked_reason.as_deref().unwrap_or("none")
            ))
            .into());
        }

        let participant = self.participant_repo.find_by_agent_id(scope, agent_id).await?;
        if participant.status != "available" {
            return Err(ErrorKind::Validation(format!(
                "participant {} is {} — pick an available agent or leave unassigned",
                participant.name, participant.status
            ))
            .into());
        }

        self.assign_to_participant_with_resolved_context(scope, &task, &participant, Some(resolved_context)).await
    }

    async fn assign_existing_task_to_participant(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        agent_id: AgentId,
    ) -> AppResult<OrchestrationTask> {
        let task = self.task_repo.find_by_id(scope, task_id).await?;
        if !can_enter_dispatch(&task) {
            return Err(ErrorKind::Validation(format!(
                "can only dispatch queued or waiting-agent tasks, current status: {}, blocked reason: {}",
                task.status,
                task.blocked_reason.as_deref().unwrap_or("none")
            ))
            .into());
        }

        let participant = self.participant_repo.find_by_agent_id(scope, agent_id).await?;
        if participant.status != "available" {
            return Err(ErrorKind::Validation(format!(
                "participant {} is {} — pick an available agent or leave unassigned",
                participant.name, participant.status
            ))
            .into());
        }

        self.assign_to_participant(scope, &task, &participant).await
    }
    /// Sweep blocked-on-agent tasks and try to dispatch each one. Called on
    /// participant heartbeat so a returning agent immediately starts work.
    /// Returns the number of tasks successfully claimed.
    pub async fn sweep_dispatchable(&self, scope: &TenantScope) -> AppResult<usize> {
        let mut claimed = 0;
        loop {
            // Stop once there are no available participants — avoids one
            // wasted query per pending task.
            if self.participant_repo.find_available(scope).await?.is_none() {
                break;
            }
            let Some(task) = self.task_repo.next_dispatchable(scope).await? else {
                break;
            };
            match self.try_auto_dispatch(scope, task).await {
                Ok(t) if t.status == "working" => claimed += 1,
                Ok(_) => break, // Marked blocked again — stop sweeping
                Err(err) => {
                    tracing::error!(error = ?err, "Auto-dispatch sweep aborted");
                    break;
                }
            }
        }
        Ok(claimed)
    }

    /// Complete a task with a result.
    pub async fn complete_task(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        result: serde_json::Value,
    ) -> AppResult<OrchestrationTask> {
        let task = self.task_repo.find_by_id(scope, task_id).await?;

        if !TaskStatusPolicy::can_complete_or_fail(&task.status) {
            return Err(ErrorKind::Validation(format!(
                "can only complete working tasks, current status: {}",
                task.status
            ))
            .into());
        }

        // Issue #37: parent completion + waiting_dependency unblock must commit
        // atomically. If unblock fails after set_result has been committed,
        // children stay stuck on `waiting_dependency` forever — `next_dispatchable`
        // filters them out and `can_complete_or_fail` rejects re-completing the
        // already-completed parent, so no retry path exists. Wrapping both in a
        // single tx means a failure rolls the parent back to `working` and the
        // caller (sidecar, MCP, retry worker) can simply re-issue `complete_task`.
        //
        // Participant release is deliberately AFTER `tx.commit()`: if it ran
        // before and the tx rolled back, the agent would be marked `available`
        // while the task is still `working`, and `sweep_dispatchable` could
        // hand the just-released agent to another task. Releasing post-commit
        // means a release failure is best-effort recoverable (the next
        // heartbeat or admin tool can fix it) but never opens a double-claim
        // window.
        let mut tx = self
            .task_repo
            .pool()
            .begin()
            .await
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("begin complete_task tx: {err}")))?;
        let updated =
            OrchestrationTaskRepository::set_result_in_tx(&mut tx, scope, task_id, "completed", result).await?;
        self.task_run_repo.finish_current_in_tx(&mut tx, scope, task_id, "completed").await?;
        let unblocked_children =
            OrchestrationTaskRepository::unblock_children_of_in_tx(&mut tx, scope, task_id).await?;
        tx.commit().await.map_err(|err| ErrorKind::Internal(anyhow::anyhow!("commit complete_task tx: {err}")))?;

        if !unblocked_children.is_empty() {
            tracing::info!(
                parent = %task_id,
                unblocked = unblocked_children.len(),
                "Unblocked children on parent completion"
            );
        }

        // Post-commit: release the participant so it's eligible for the next
        // sweep. Failure here doesn't roll back the completion (the row's
        // already terminal); operators can rely on the heartbeat path or
        // an admin tool to fix the participant row.
        if let Some(agent_id) = task.assigned_agent_id
            && let Err(err) = self.participant_repo.update_status(scope, agent_id, "available").await
        {
            tracing::error!(error = ?err, agent_id = %agent_id, "Failed to release participant after completion");
        }
        // Released agent → re-sweep so a queued task picks them up immediately.
        // Outside the tx on purpose: sweep is best-effort and shouldn't fail completion.
        if let Err(err) = self.sweep_dispatchable(scope).await {
            tracing::error!(error = ?err, task_id = %task_id, "Post-completion sweep failed");
        }
        Ok(updated)
    }

    /// Fail a task with an error.
    pub async fn fail_task(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        error: serde_json::Value,
    ) -> AppResult<OrchestrationTask> {
        let task = self.task_repo.find_by_id(scope, task_id).await?;

        if !TaskStatusPolicy::can_complete_or_fail(&task.status) {
            return Err(
                ErrorKind::Validation(format!("can only fail working tasks, current status: {}", task.status)).into()
            );
        }

        if let Some(metadata) = quota_block_metadata(&error) {
            let mut tx = self
                .task_repo
                .pool()
                .begin()
                .await
                .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("begin quota block tx: {err}")))?;
            let updated = OrchestrationTaskRepository::mark_blocked_retryable_in_tx(
                &mut tx,
                scope,
                task_id,
                "quota_exceeded",
                metadata,
                error,
            )
            .await?;
            self.task_run_repo.finish_current_in_tx(&mut tx, scope, task_id, "failed").await?;
            upsert_task_owner_lifecycle_notification_in_tx(&mut tx, &updated, None, TaskOwnerNotificationKind::Blocked)
                .await?;
            tx.commit().await.map_err(|err| ErrorKind::Internal(anyhow::anyhow!("commit quota block tx: {err}")))?;
            if let Some(agent_id) = task.assigned_agent_id
                && let Err(err) = self.participant_repo.update_status(scope, agent_id, "available").await
            {
                tracing::error!(error = ?err, agent_id = %agent_id, "Failed to release participant after quota block");
            }
            return Ok(updated);
        }

        let mut tx = self
            .task_repo
            .pool()
            .begin()
            .await
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("begin fail_task tx: {err}")))?;
        let updated = OrchestrationTaskRepository::set_result_in_tx(&mut tx, scope, task_id, "failed", error).await?;
        self.task_run_repo.finish_current_in_tx(&mut tx, scope, task_id, "failed").await?;
        upsert_task_owner_lifecycle_notification_in_tx(&mut tx, &updated, None, TaskOwnerNotificationKind::Failed)
            .await?;
        tx.commit().await.map_err(|err| ErrorKind::Internal(anyhow::anyhow!("commit fail_task tx: {err}")))?;
        if let Some(agent_id) = task.assigned_agent_id
            && let Err(err) = self.participant_repo.update_status(scope, agent_id, "available").await
        {
            tracing::error!(error = ?err, agent_id = %agent_id, "Failed to release participant after failure");
        }
        if let Err(err) = self.sweep_dispatchable(scope).await {
            tracing::error!(error = ?err, task_id = %task_id, "Post-failure sweep failed");
        }
        Ok(updated)
    }

    /// Approve a task created with `requiresApproval=true`. Approval clears the
    /// human gate, then either queues the task or keeps it blocked on an
    /// unfinished parent. Queued tasks immediately re-enter auto-dispatch.
    pub async fn approve_task(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<OrchestrationTask> {
        let task = self.task_repo.find_by_id(scope, task_id).await?;
        if task.status != "blocked" || task.blocked_reason.as_deref() != Some("waiting_approval") {
            return Err(ErrorKind::Validation("task is not waiting for approval".into()).into());
        }
        if !task.requires_approval {
            return Err(ErrorKind::Validation("task approval has already been consumed".into()).into());
        }

        let parent_status = if let Some(parent_id) = task.parent_task_id {
            Some(self.task_repo.find_by_id(scope, parent_id).await?.status)
        } else {
            None
        };
        let (next_status, next_reason, next_metadata) =
            if BlockedTaskPolicy::needs_dependency_block(parent_status.as_deref()) {
                ("blocked", Some("waiting_dependency"), Some(json!({ "pending": 1 })))
            } else {
                ("queued", None, None)
            };
        let approved = self
            .task_repo
            .approve_waiting_task(scope, task_id, scope.user_id(), next_status, next_reason, next_metadata)
            .await?;
        if approved.status == "queued" {
            return self.try_auto_dispatch(scope, approved).await;
        }
        Ok(approved)
    }

    /// Register an agent as an orchestration participant.
    pub async fn register_participant(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        name: &str,
        capabilities: &[String],
    ) -> AppResult<Participant> {
        ParticipantName::validate(name)?;
        let participant = self.participant_repo.register(scope, agent_id, name, capabilities).await?;
        if let Err(err) = self.sweep_dispatchable(scope).await {
            tracing::error!(error = ?err, agent_id = %agent_id, "Post-registration sweep failed");
        }
        Ok(participant)
    }

    pub async fn list_participants(&self, scope: &TenantScope, status: Option<&str>) -> AppResult<Vec<Participant>> {
        if let Some(s) = status {
            ParticipantStatusPolicy::validate_filter(s)?;
        }
        self.participant_repo.list(scope, status).await
    }

    /// Update heartbeat for a participant. Heartbeat also bumps `offline → available`
    /// (in the repo) so a returning agent becomes pickup-eligible, then sweeps
    /// blocked-on-agent tasks.
    pub async fn participant_heartbeat(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<Participant> {
        let participant = self.participant_repo.heartbeat(scope, agent_id).await?;
        if participant.status == "available"
            && let Err(err) = self.sweep_dispatchable(scope).await
        {
            tracing::error!(error = ?err, agent_id = %agent_id, "Post-heartbeat sweep failed");
        }
        Ok(participant)
    }

    pub async fn unregister_participant(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        self.participant_repo.unregister(scope, agent_id).await
    }

    /// Convert a single task entity into the JSON-friendly summary the UI consumes.
    /// Resolves the assigned agent's display name in a separate batch helper for lists.
    pub fn to_summary_with_name(task: OrchestrationTask, agent_name: Option<String>) -> TaskSummary {
        let blocked_hint = match task.status.as_str() {
            "blocked" => task
                .blocked_reason
                .as_deref()
                .map(|reason| BlockedTaskPolicy::hint(reason, task.blocked_metadata.as_ref())),
            _ => None,
        };

        let params = task
            .params
            .as_ref()
            .map(|p| TaskParams {
                task: p.get("task").and_then(|v| v.as_str()).unwrap_or(&task.title).to_string(),
                message: p.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
            .unwrap_or_else(|| TaskParams {
                task: task.title.clone(),
                message: task.description.clone().unwrap_or_default(),
            });

        let error = task
            .error
            .as_ref()
            .map(|e| e.get("message").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| e.to_string()));

        let is_completed = task.status == "completed";

        TaskSummary {
            id: task.id,
            group_id: task.group_id,
            state: task.status,
            method: "tasks/send".into(),
            params,
            priority: task.priority,
            progress: task.progress,
            created_by: task.created_by.as_uuid(),
            assigned_to: task.assigned_agent_id.map(|a| a.as_uuid()),
            assigned_agent_name: agent_name,
            error,
            result: task.result,
            blocked_reason: task.blocked_reason,
            blocked_hint,
            blocked_metadata: task.blocked_metadata,
            created_at: task.created_at.to_rfc3339(),
            updated_at: task.updated_at.to_rfc3339(),
            completed_at: if is_completed { task.completed_at.map(|t| t.to_rfc3339()) } else { None },
            context_counts: TaskContextCounts::default(),
        }
    }

    /// Resolve agent display names for a batch of tasks in a single query.
    pub async fn summarize_tasks(
        &self,
        scope: &TenantScope,
        tasks: Vec<OrchestrationTask>,
    ) -> AppResult<Vec<TaskSummary>> {
        let agent_ids: Vec<Uuid> = tasks.iter().filter_map(|t| t.assigned_agent_id.map(|a| a.as_uuid())).collect();
        let names = self.task_repo.resolve_agent_names(scope, &agent_ids).await?;
        let task_ids: Vec<Uuid> = tasks.iter().map(|task| task.id).collect();
        let mut context_counts =
            RunContextInjectionRepository::new(self.task_repo.pool().clone()).count_by_tasks(scope, &task_ids).await?;
        Ok(tasks
            .into_iter()
            .map(|t| {
                let name = t.assigned_agent_id.and_then(|a| names.get(&a.as_uuid()).cloned());
                let mut summary = Self::to_summary_with_name(t, name);
                if let Some(counts) = context_counts.remove(&summary.id) {
                    summary.context_counts = counts.into();
                }
                summary
            })
            .collect())
    }

    /// Single-task summary helper that resolves the assigned agent name lazily.
    pub async fn summarize_task(&self, scope: &TenantScope, task: OrchestrationTask) -> AppResult<TaskSummary> {
        let name = if let Some(agent_id) = task.assigned_agent_id {
            let names = self.task_repo.resolve_agent_names(scope, &[agent_id.as_uuid()]).await?;
            names.get(&agent_id.as_uuid()).cloned()
        } else {
            None
        };
        let mut summary = Self::to_summary_with_name(task, name);
        if let Some(counts) = RunContextInjectionRepository::new(self.task_repo.pool().clone())
            .count_by_tasks(scope, &[summary.id])
            .await?
            .remove(&summary.id)
        {
            summary.context_counts = counts.into();
        }
        Ok(summary)
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_task_with_assignee(
        &self,
        scope: &TenantScope,
        title: &str,
        description: Option<&str>,
        params: Option<serde_json::Value>,
        priority: &str,
        group_id: Option<Uuid>,
        agent_id: AgentId,
        parent_task_id: Option<Uuid>,
    ) -> AppResult<OrchestrationTask> {
        let mut tx = self
            .task_repo
            .pool()
            .begin()
            .await
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("begin create assigned task tx: {err}")))?;
        let participant = ParticipantRepository::find_by_agent_id_in_tx(&mut tx, scope, agent_id).await?;
        if participant.status != "available" {
            let _ = tx.rollback().await;
            return Err(ErrorKind::Validation(format!(
                "participant {} is {} — pick an available agent or leave unassigned",
                participant.name, participant.status
            ))
            .into());
        }

        ParticipantRepository::update_status_in_tx(&mut tx, scope, agent_id, "busy").await?;
        let task = OrchestrationTaskRepository::create_in_tx(
            &mut tx,
            scope,
            CreateTaskRow {
                group_id,
                title,
                description,
                priority,
                params: params.as_ref(),
                assigned_agent_id: Some(agent_id),
                parent_task_id,
                initial_status: "working",
                initial_blocked_reason: None,
                initial_blocked_metadata: None,
                requires_approval: false,
            },
        )
        .await?;
        let task = OrchestrationTaskRepository::assign_agent_in_tx(
            &mut tx,
            scope,
            task.id,
            agent_id,
            Uuid::now_v7(),
            DEFAULT_ASSIGNMENT_LEASE_SECS,
        )
        .await?;
        let delivery_id = task
            .last_assignment_id
            .ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("task {} missing last_assignment_id", task.id)))?;
        let idempotency_key = delivery_id.to_string();
        let resolved_context = match self.resolve_assignment_context(scope, &task, &participant).await {
            Ok(resolved_context) => resolved_context,
            Err(err) => {
                let _ = tx.rollback().await;
                return Err(err);
            }
        };
        let task_run = match self
            .task_run_repo
            .create_for_assignment_in_tx(
                &mut tx,
                scope,
                &task,
                &idempotency_key,
                capability_profile_from_participant(&participant, resolved_context.as_ref()),
            )
            .await
        {
            Ok(task_run) => task_run,
            Err(err) => {
                let _ = tx.rollback().await;
                return Err(err);
            }
        };
        let context_envelope = match self
            .build_assignment_context_envelope(scope, &task, participant.agent_id, &task_run, resolved_context.clone())
            .await
        {
            Ok(context_envelope) => context_envelope,
            Err(err) => {
                let _ = tx.rollback().await;
                return Err(err);
            }
        };
        if let Some(context_envelope) = context_envelope.as_ref() {
            match RunContextInjectionRepository::record_envelope_in_tx(&mut tx, scope, &task_run, context_envelope)
                .await
            {
                Ok(_) => {}
                Err(err) => {
                    let _ = tx.rollback().await;
                    return Err(err);
                }
            }
        }
        let assignment = assignment_from_task(&task, context_envelope)?;
        if let Err(err) = insert_assignment_outbox_in_tx(&mut tx, scope.org_id().as_uuid(), task.id, &assignment).await
        {
            let _ = tx.rollback().await;
            return Err(ErrorKind::Internal(anyhow::anyhow!("insert assignment outbox: {err}")).into());
        }
        tx.commit()
            .await
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("commit create assigned task tx: {err}")))?;
        Ok(task)
    }

    async fn resolve_assignment_context(
        &self,
        scope: &TenantScope,
        task: &OrchestrationTask,
        participant: &Participant,
    ) -> AppResult<Option<ResolvedContext>> {
        if !self.context_injection_enabled {
            return Ok(None);
        }
        let Some(resolver) = &self.context_resolver else {
            return Ok(None);
        };
        let snapshot = ContextTaskSnapshot::from_task(task);
        resolver.resolve_for_task_snapshot(&scope.scoped_read(), snapshot, participant.agent_id).await.map(Some)
    }

    async fn build_assignment_context_envelope(
        &self,
        scope: &TenantScope,
        task: &OrchestrationTask,
        agent_id: AgentId,
        run: &TaskRun,
        resolved_context: Option<ResolvedContext>,
    ) -> AppResult<Option<ContextEnvelope>> {
        if !self.context_injection_enabled {
            return Ok(None);
        }
        let Some(resolved_context) = resolved_context else {
            return Ok(None);
        };
        let Some(resolver) = &self.context_resolver else {
            return Ok(None);
        };
        ContextEnvelopeService::new(self.task_repo.pool().clone(), resolver.clone())
            .build_from_resolved(&scope.scoped_read(), task.id, run.id, agent_id, resolved_context)
            .await
            .map(Some)
    }
}

fn capability_profile_from_participant(
    participant: &Participant,
    resolved_context: Option<&ResolvedContext>,
) -> serde_json::Value {
    match resolved_context {
        Some(resolved_context) => json!({
            "participant_capabilities": participant.capabilities,
            "runtime_capability": resolved_context.capability,
            "context_resolution": {
                "envelope_version": resolved_context.envelope_version,
                "applied": resolved_context.applied,
                "suggested": resolved_context.suggested,
                "degradation": resolved_context.degradation,
            }
        }),
        None => json!({
            "capabilities": participant.capabilities,
        }),
    }
}

fn assignment_from_task(
    task: &OrchestrationTask,
    context_envelope: Option<ContextEnvelope>,
) -> AppResult<TaskAssignment> {
    let agent_id = task
        .assigned_agent_id
        .ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("task {} missing assigned_agent_id", task.id)))?;
    let delivery_id = task
        .last_assignment_id
        .ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("task {} missing last_assignment_id", task.id)))?;
    let lease_expires_at = task
        .lease_expires_at
        .ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("task {} missing lease_expires_at", task.id)))?;
    let (task_text, message) = task
        .params
        .as_ref()
        .map(|p| {
            (
                p.get("task").and_then(|v| v.as_str()).unwrap_or(&task.title).to_string(),
                p.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            )
        })
        .unwrap_or_else(|| (task.title.clone(), task.description.clone().unwrap_or_default()));

    Ok(TaskAssignment {
        delivery_id: Some(delivery_id),
        attempt: Some(task.attempt),
        lease_expires_at: Some(lease_expires_at),
        task_id: task.id,
        agent_id: agent_id.as_uuid(),
        title: task.title.clone(),
        task: task_text,
        message,
        priority: task.priority.clone(),
        context_envelope,
    })
}

fn can_enter_dispatch(task: &OrchestrationTask) -> bool {
    BlockedTaskPolicy::can_enter_dispatch(&task.status, task.blocked_reason.as_deref())
}

fn quota_block_metadata(error: &serde_json::Value) -> Option<serde_json::Value> {
    let code = json_string(error, "code").or_else(|| json_nested_string(error, "error", "code"));
    let status = json_i64(error, "status").or_else(|| json_nested_i64(error, "error", "status"));
    let message = json_string(error, "message").or_else(|| json_nested_string(error, "error", "message"));

    let code_lc = code.as_deref().map(str::to_ascii_lowercase);
    let message_lc = message.as_deref().map(str::to_ascii_lowercase);
    let quota_like = matches!(
        code_lc.as_deref(),
        Some(
            "quota_exceeded"
                | "insufficient_quota"
                | "rate_limited"
                | "rate_limit_exceeded"
                | "billing_hard_limit_reached"
        )
    ) || status == Some(429)
        || message_lc.as_deref().is_some_and(|m| {
            m.contains("quota") || m.contains("rate limit") || m.contains("rate_limit") || m.contains("billing limit")
        });

    if !quota_like {
        return None;
    }

    let used = json_i64(error, "used")
        .or_else(|| json_nested_i64(error, "quota", "used"))
        .or_else(|| json_nested_i64(error, "error", "used"))
        .unwrap_or(0);
    let limit = json_i64(error, "limit")
        .or_else(|| json_nested_i64(error, "quota", "limit"))
        .or_else(|| json_nested_i64(error, "error", "limit"))
        .unwrap_or(0);
    Some(json!({
        "code": code.unwrap_or_else(|| "quota_exceeded".to_string()),
        "status": status,
        "used": used,
        "limit": limit,
        "provider": json_string(error, "provider").or_else(|| json_nested_string(error, "error", "provider")),
    }))
}

fn json_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn json_nested_string(value: &serde_json::Value, parent: &str, key: &str) -> Option<String> {
    value.get(parent).and_then(|v| json_string(v, key))
}

fn json_i64(value: &serde_json::Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|v| v.as_i64())
}

fn json_nested_i64(value: &serde_json::Value, parent: &str, key: &str) -> Option<i64> {
    value.get(parent).and_then(|v| json_i64(v, key))
}
