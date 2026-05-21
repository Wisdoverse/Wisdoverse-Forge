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
use agentforge_core::orchestration_protocol::DEFAULT_ASSIGNMENT_LEASE_SECS;
use agentforge_core::{AgentId, AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::{OrchestrationTask, Participant, TaskRun};
use agentforge_db::inbox_notifications::{TaskOwnerNotificationKind, upsert_task_owner_lifecycle_notification_in_tx};
use agentforge_infra::NatsClient;
use agentforge_jobs::insert_assignment_outbox_in_tx;
use uuid::Uuid;

use crate::domain::context::{ContextFeature, ContextFeatureFlags};
use crate::domain::context_resolver::{ContextTaskSnapshot, ResolvedContext};
use crate::domain::orchestration::{
    BlockedTaskPolicy, DispatchSweepDecision, DispatchSweepPolicy, ParticipantAvailabilityAction,
    ParticipantAvailabilityPolicy, ParticipantName, ParticipantStatusPolicy, QuotaBlockPolicy, TaskAssignmentPolicy,
    TaskCreationPolicy, TaskLifecyclePolicy, TaskListPage, TaskPatchAction, TaskPatchPolicy, TaskPriority,
    TaskRunCapabilityProfile, TaskStatusPolicy, TaskTitle, task_assignment_snapshot,
};
pub(crate) use crate::domain::orchestration::{
    CreateTaskParamsInput, create_task_request_parts, orchestration_delete_response,
    orchestration_participant_response, orchestration_participants_response, orchestration_stats_response,
    orchestration_task_context_response, orchestration_task_response, orchestration_tasks_response,
    task_update_broadcast_payload, task_update_broadcast_subject,
};
pub use crate::domain::orchestration::{
    ParticipantSummary, TaskContextCounts, TaskStatsResponse, TaskSummary, task_summary,
};
use crate::repositories::orchestration::run_context_injection::{
    ContextInjectionCounts, RunContextInjectionRepository,
};
use crate::repositories::orchestration::task_run::TaskRunRepository;
use crate::repositories::orchestration::{
    CreateTaskRow, OrchestrationTaskRepository, OrchestrationTaskStats, ParticipantRepository, UpdateTaskRow,
};
use crate::services::context_envelope::ContextEnvelopeService;
use crate::services::context_resolver::ContextResolverService;

impl From<ContextInjectionCounts> for TaskContextCounts {
    fn from(counts: ContextInjectionCounts) -> Self {
        TaskContextCounts::new(counts.applied_memories, counts.applied_skills)
    }
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

impl From<Participant> for ParticipantSummary {
    fn from(p: Participant) -> Self {
        Self {
            id: p.id,
            agent_id: p.agent_id.as_uuid(),
            name: p.name,
            status: p.status,
            capabilities: p.capabilities,
            last_heartbeat_at: p.last_heartbeat_at.map(|t| t.to_rfc3339()),
        }
    }
}

/// Business logic layer for orchestration operations.
pub struct OrchestrationService {
    task_repo: OrchestrationTaskRepository,
    participant_repo: ParticipantRepository,
    task_run_repo: TaskRunRepository,
    context_resolver: Option<Arc<ContextResolverService>>,
    context_injection_enabled: bool,
    broadcast_bus: Option<Arc<NatsClient>>,
}

impl OrchestrationService {
    pub fn new(task_repo: OrchestrationTaskRepository, participant_repo: ParticipantRepository) -> Self {
        let task_run_repo = TaskRunRepository::new(task_repo.pool().clone());
        Self {
            task_repo,
            participant_repo,
            task_run_repo,
            context_resolver: None,
            context_injection_enabled: true,
            broadcast_bus: None,
        }
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

    pub fn with_context_runtime(
        self,
        context_features: ContextFeatureFlags,
        context_resolver: Arc<ContextResolverService>,
    ) -> Self {
        let enabled = context_features.enabled(ContextFeature::Injection);
        let service = self.with_context_injection_enabled(enabled);
        if enabled { service.with_context_resolver(context_resolver) } else { service }
    }

    pub fn with_broadcast_bus(mut self, nats: Arc<NatsClient>) -> Self {
        self.broadcast_bus = Some(nats);
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
        TaskCreationPolicy::ensure_approval_task_is_unassigned(requires_approval, assigned_to)?;
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

        let initial_state =
            TaskCreationPolicy::initial_unassigned_state(&missing_inputs, requires_approval, parent_status.as_deref());

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
                    assigned_agent_id: None,
                    parent_task_id,
                    initial_status: initial_state.initial_status,
                    initial_blocked_reason: initial_state.initial_blocked_reason,
                    initial_blocked_metadata: initial_state.initial_blocked_metadata,
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
        TaskPatchPolicy::validate_progress(progress)?;

        let transition_state = state.as_deref();
        let current_for_guard = if TaskPatchPolicy::requires_current_task(transition_state, &assigned_to) {
            Some(self.task_repo.find_by_id(scope, id).await?)
        } else {
            None
        };
        if let Some(current) = current_for_guard.as_ref() {
            TaskPatchPolicy::ensure_current_allows_transition(
                &current.status,
                current.blocked_reason.as_deref(),
                transition_state,
            )?;
        }
        let patch_action = TaskPatchPolicy::plan(transition_state, priority.as_deref(), progress, &assigned_to)?;

        match patch_action {
            TaskPatchAction::AssignToParticipant(agent_id) => {
                return self.assign_existing_task_to_participant(scope, id, agent_id).await;
            }
            TaskPatchAction::Dispatch => return self.dispatch_task(scope, id).await,
            TaskPatchAction::Complete => {
                return self.complete_task(scope, id, TaskPatchPolicy::manual_complete_result()).await;
            }
            TaskPatchAction::Fail => {
                return self.fail_task(scope, id, TaskPatchPolicy::manual_failure_error()).await;
            }
            TaskPatchAction::Cancel => return self.cancel_task(scope, id).await,
            TaskPatchAction::Unassign => {
                let current = match current_for_guard.as_ref() {
                    Some(task) => task,
                    None => unreachable!("assignment guard should have loaded current task"),
                };
                TaskPatchPolicy::ensure_can_unassign(&current.status)?;
            }
            TaskPatchAction::Patch => {}
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

        if TaskPatchPolicy::should_auto_dispatch_after_patch(next_state.as_deref(), updated.assigned_agent_id) {
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
        TaskLifecyclePolicy::ensure_can_retry(&task.status, task.blocked_reason.as_deref(), task.requires_approval)?;
        let reset = self.task_repo.retry(scope, id).await?;
        self.try_auto_dispatch(scope, reset).await
    }

    /// Dispatch a task: find an available participant, assign it, mark running.
    /// Used by both the explicit `POST /tasks/:id/dispatch` endpoint and the
    /// auto-dispatcher invoked on create / heartbeat.
    pub async fn dispatch_task(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<OrchestrationTask> {
        let task = self.task_repo.find_by_id(scope, task_id).await?;

        BlockedTaskPolicy::ensure_can_enter_dispatch(&task.status, task.blocked_reason.as_deref())?;

        let participant =
            self.participant_repo.find_available(scope).await?.ok_or_else(|| -> agentforge_core::AppError {
                BlockedTaskPolicy::no_available_participants_error().into()
            })?;

        self.assign_to_participant(scope, &task, &participant).await
    }

    /// Try to dispatch a task to an available participant. If no participant is
    /// available the task is marked `blocked/waiting_agent` with metadata that
    /// powers the "还差 X 个 agent" hint. Returns the updated task either way.
    async fn try_auto_dispatch(&self, scope: &TenantScope, task: OrchestrationTask) -> AppResult<OrchestrationTask> {
        if !BlockedTaskPolicy::can_enter_dispatch(&task.status, task.blocked_reason.as_deref()) {
            return Ok(task);
        }
        match self.participant_repo.find_available(scope).await? {
            Some(participant) => self.assign_to_participant(scope, &task, &participant).await,
            None => {
                let (available, busy, offline) = self.participant_repo.count_by_status(scope).await?;
                let metadata = BlockedTaskPolicy::waiting_agent_metadata(available, busy, offline);
                tracing::info!(task_id = %task.id, busy, offline, "No available participant — task blocked on waiting_agent");
                self.task_repo.mark_blocked(scope, task.id, BlockedTaskPolicy::waiting_agent_reason(), metadata).await
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
                TaskRunCapabilityProfile::from_assignment(&participant.capabilities, resolved_context.as_ref()),
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
        let assignment = TaskAssignmentPolicy::build(task_assignment_snapshot(&task), context_envelope)?;
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
        BlockedTaskPolicy::ensure_can_enter_dispatch(&task.status, task.blocked_reason.as_deref())?;

        let participant = self.participant_repo.find_by_agent_id(scope, agent_id).await?;
        ParticipantAvailabilityPolicy::ensure_available(
            &participant.name,
            &participant.status,
            ParticipantAvailabilityAction::AssignTask,
        )?;

        self.assign_to_participant_with_resolved_context(scope, &task, &participant, Some(resolved_context)).await
    }

    async fn assign_existing_task_to_participant(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        agent_id: AgentId,
    ) -> AppResult<OrchestrationTask> {
        let task = self.task_repo.find_by_id(scope, task_id).await?;
        BlockedTaskPolicy::ensure_can_enter_dispatch(&task.status, task.blocked_reason.as_deref())?;

        let participant = self.participant_repo.find_by_agent_id(scope, agent_id).await?;
        ParticipantAvailabilityPolicy::ensure_available(
            &participant.name,
            &participant.status,
            ParticipantAvailabilityAction::AssignTask,
        )?;

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
                Ok(t) => match DispatchSweepPolicy::after_dispatch_attempt(&t.status) {
                    DispatchSweepDecision::ClaimedTask => claimed += 1,
                    DispatchSweepDecision::Stop => break,
                },
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

        TaskLifecyclePolicy::ensure_can_complete(&task.status)?;

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

        TaskLifecyclePolicy::ensure_can_fail(&task.status)?;

        if let Some(metadata) = QuotaBlockPolicy::metadata(&error) {
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
        TaskLifecyclePolicy::ensure_can_approve(&task.status, task.blocked_reason.as_deref(), task.requires_approval)?;

        let parent_status = if let Some(parent_id) = task.parent_task_id {
            Some(self.task_repo.find_by_id(scope, parent_id).await?.status)
        } else {
            None
        };
        let (next_status, next_reason, next_metadata) =
            BlockedTaskPolicy::approval_release_state(parent_status.as_deref());
        let approved = self
            .task_repo
            .approve_waiting_task(scope, task_id, scope.user_id(), next_status, next_reason, next_metadata)
            .await?;
        if BlockedTaskPolicy::should_auto_dispatch_after_approval(&approved.status) {
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
        if ParticipantStatusPolicy::should_sweep_after_heartbeat(&participant.status)
            && let Err(err) = self.sweep_dispatchable(scope).await
        {
            tracing::error!(error = ?err, agent_id = %agent_id, "Post-heartbeat sweep failed");
        }
        Ok(participant)
    }

    pub async fn unregister_participant(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        self.participant_repo.unregister(scope, agent_id).await
    }

    pub(crate) async fn mark_participant_offline(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
    ) -> AppResult<Participant> {
        ParticipantStatusPolicy::validate_filter("offline")?;
        self.participant_repo.update_status(scope, agent_id, "offline").await
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
                let mut summary = task_summary(t, name);
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
        let mut summary = task_summary(task, name);
        if let Some(counts) = RunContextInjectionRepository::new(self.task_repo.pool().clone())
            .count_by_tasks(scope, &[summary.id])
            .await?
            .remove(&summary.id)
        {
            summary.context_counts = counts.into();
        }
        Ok(summary)
    }

    pub(crate) async fn broadcast_task_update(&self, scope: &TenantScope, action: &str, task: &TaskSummary) {
        let Some(nats) = &self.broadcast_bus else {
            return;
        };
        if !nats.is_connected() {
            return;
        }
        let subject = task_update_broadcast_subject(scope.org_id().as_uuid());
        let payload = task_update_broadcast_payload(action, task);
        if let Err(err) = nats.publish_json(&subject, payload).await {
            tracing::warn!(
                error = ?err,
                %subject,
                task_id = %task.id,
                %action,
                "Failed to broadcast orchestration task update"
            );
        }
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
        if let Err(err) = ParticipantAvailabilityPolicy::ensure_available(
            &participant.name,
            &participant.status,
            ParticipantAvailabilityAction::AssignTask,
        ) {
            let _ = tx.rollback().await;
            return Err(err);
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
                TaskRunCapabilityProfile::from_assignment(&participant.capabilities, resolved_context.as_ref()),
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
        let assignment = TaskAssignmentPolicy::build(task_assignment_snapshot(&task), context_envelope)?;
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
        let snapshot = ContextTaskSnapshot {
            task_id: task.id,
            title: task.title.clone(),
            description: task.description.clone(),
            params: task.params.clone(),
        };
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
