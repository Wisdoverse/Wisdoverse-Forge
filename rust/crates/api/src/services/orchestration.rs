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
use agentforge_core::{AgentId, AppResult, TenantScope};
use agentforge_db::entities::{OrchestrationTask, Participant, TaskRun};
use agentforge_db::inbox_notifications::{TaskOwnerNotificationKind, upsert_task_owner_lifecycle_notification_in_tx};
use agentforge_infra::NatsClient;
use agentforge_jobs::insert_assignment_outbox_in_tx;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::context::{ContextFeature, ContextFeatureFlags};
use crate::domain::context_resolver::{ContextTaskSnapshot, ResolvedContext};
use crate::domain::orchestration::{
    BlockedTaskPolicy, DispatchSweepDecision, DispatchSweepPolicy, OrchestrationRepositoryPolicy,
    OrchestrationTransactionPolicy, ParticipantAvailabilityAction, ParticipantAvailabilityPolicy, ParticipantName,
    ParticipantStatusPolicy, QuotaBlockPolicy, ReviewGatePolicy, TaskAssignmentPolicy, TaskAssignmentSnapshot,
    TaskCreationPolicy, TaskDependencyPolicy, TaskLifecyclePolicy, TaskListPage, TaskPatchAction, TaskPatchPolicy,
    TaskPriority, TaskRetirePolicy, TaskRunCapabilityProfile, TaskStatusPolicy, TaskTitle, TaskWaitEstimate,
    TaskWaitEstimatePolicy,
};
pub(crate) use crate::domain::orchestration::{
    CreateTaskParamsInput, TaskHistoryExportRowProjection, create_task_request_parts, orchestration_delete_response,
    orchestration_human_marks_response, orchestration_participant_response, orchestration_participants_response,
    orchestration_stats_response, orchestration_task_comment_response, orchestration_task_comments_response,
    orchestration_task_context_response, orchestration_task_export_response, orchestration_task_response,
    orchestration_task_review_check_response, orchestration_task_review_checks_response,
    orchestration_task_review_gates_response, orchestration_task_runs_response, orchestration_tasks_response,
    task_history_csv, task_update_broadcast_payload, task_update_broadcast_subject,
};
pub use crate::domain::orchestration::{
    HumanMarkerSummary, ParticipantSummary, ReviewGateStatus, TaskCommentAuthor, TaskCommentSummary, TaskContextCounts,
    TaskReviewCheckSummary, TaskRunImageSummary, TaskRunSummary, TaskStatsResponse, TaskSummary,
};
use crate::domain::self_fix::self_fix_pr_job_payload;
use crate::repositories::orchestration::run_context_injection::{
    ContextInjectionCounts, RunContextInjectionRepository,
};
use crate::repositories::orchestration::task_comment::{
    HumanMarkerRow, TaskCommentRepository, TaskCommentWithAuthorRow,
};
use crate::repositories::orchestration::task_run::TaskRunRepository;
use crate::repositories::orchestration::{
    CreateTaskRow, OrchestrationTaskRepository, OrchestrationTaskStats, ParticipantRepository, TaskHistoryExportRow,
    TaskReviewCheckRepository, TaskReviewCheckRow, UpdateTaskRow,
};
use crate::services::context_envelope::ContextEnvelopeService;
use crate::services::context_resolver::ContextResolverService;

// Row -> projection adapters (DDD-2): these map persisted `OrchestrationTask` /
// `TaskRun` rows onto the pure domain projections. They live in the service so
// `domain::orchestration` stays free of `agentforge_db`; the projection TYPES
// (`TaskSummary`, `TaskRunSummary`, `TaskAssignmentSnapshot`) remain in the domain.

/// Project a persisted `OrchestrationTask` row onto the kanban [`TaskSummary`].
///
/// Thin wrapper over the canonical adapter in
/// `agentforge_jobs::orchestration_realtime::task_summary` (MS-3 PR-E) — the
/// same code the jobs WS projector uses, so the REST responses and both
/// `orchestration:task_update` producers can no longer drift. Kept owned-arg
/// here to match the existing api call sites.
pub fn task_summary(task: OrchestrationTask, agent_name: Option<String>) -> TaskSummary {
    agentforge_jobs::orchestration_realtime::task_summary(&task, agent_name.as_deref())
}

/// Project a persisted `TaskRun` row onto [`TaskRunSummary`].
pub fn task_run_summary(run: TaskRun) -> TaskRunSummary {
    TaskRunSummary {
        id: run.id,
        agent_id: run.agent_id.as_uuid(),
        status: run.status,
        started_at: run.started_at.to_rfc3339(),
        finished_at: run.finished_at.map(|t| t.to_rfc3339()),
        runtime_kind: string_value(&run.capability_profile, "runtime_kind"),
        cli_tool: string_value(&run.capability_profile, "cli_tool"),
        provider_name: string_value(&run.capability_profile, "provider_name"),
        max_context_tokens: capability_value(&run.capability_profile, "max_context_tokens")
            .and_then(serde_json::Value::as_u64),
        image: task_run_image_summary(&run.capability_profile),
    }
}

fn task_run_image_summary(capability_profile: &serde_json::Value) -> Option<TaskRunImageSummary> {
    TaskRunImageSummary::from_capability_profile(capability_profile)
}

fn string_value(value: &serde_json::Value, key: &str) -> Option<String> {
    capability_value(value, key).and_then(serde_json::Value::as_str).map(str::to_owned)
}

fn capability_value<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    value.get(key).or_else(|| value.get("runtime_capability")?.get(key))
}

/// Project a persisted comment row onto TaskCommentSummary.
pub fn task_comment_summary(row: TaskCommentWithAuthorRow) -> TaskCommentSummary {
    TaskCommentSummary {
        id: row.id,
        task_id: row.task_id,
        kind: row.kind,
        body: row.body,
        author: TaskCommentAuthor { id: row.author_user_id.as_uuid(), name: row.author_name.unwrap_or_default() },
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }
}

/// Map a persisted export row onto the domain CSV projection.
pub(crate) fn task_history_projection(row: TaskHistoryExportRow) -> TaskHistoryExportRowProjection {
    TaskHistoryExportRowProjection {
        id: row.id,
        title: row.title,
        status: row.status,
        priority: row.priority,
        progress: row.progress,
        creator_name: row.creator_name,
        assigned_agent_name: row.assigned_agent_name,
        runs_count: row.runs_count,
        created_at: row.created_at,
        completed_at: row.completed_at,
        updated_at: row.updated_at,
        blocked_reason: row.blocked_reason,
        requires_approval: row.requires_approval,
    }
}

/// Project a review-check row onto TaskReviewCheckSummary.
pub fn task_review_check_summary(row: TaskReviewCheckRow) -> TaskReviewCheckSummary {
    TaskReviewCheckSummary { check_key: row.check_key, done: row.done, updated_at: row.updated_at.to_rfc3339() }
}

/// Project the latest blocker/unblock signal onto HumanMarkerSummary.
pub fn human_marker_summary(row: HumanMarkerRow) -> HumanMarkerSummary {
    HumanMarkerSummary {
        task_id: row.task_id,
        kind: row.kind,
        body: row.body,
        author_name: row.author_name,
        created_at: row.created_at.to_rfc3339(),
    }
}

/// Borrow the assignment-relevant fields of an `OrchestrationTask` row into the
/// domain [`TaskAssignmentSnapshot`] the assignment policy operates on.
pub(crate) fn task_assignment_snapshot(task: &OrchestrationTask) -> TaskAssignmentSnapshot<'_> {
    TaskAssignmentSnapshot {
        task_id: task.id,
        assigned_agent_id: task.assigned_agent_id,
        last_assignment_id: task.last_assignment_id,
        lease_expires_at: task.lease_expires_at,
        attempt: task.attempt,
        title: &task.title,
        description: task.description.as_deref(),
        params: task.params.as_ref(),
        priority: &task.priority,
    }
}

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
            runtime_kind: p.runtime_kind,
            last_heartbeat_at: p.last_heartbeat_at.map(|t| t.to_rfc3339()),
        }
    }
}

/// Business logic layer for orchestration operations.
pub struct OrchestrationService {
    task_repo: OrchestrationTaskRepository,
    participant_repo: ParticipantRepository,
    task_run_repo: TaskRunRepository,
    task_comment_repo: TaskCommentRepository,
    task_review_check_repo: TaskReviewCheckRepository,
    /// Agent lookups for image dispatch (workspace resolution). Held as a field so
    /// service methods don't construct repositories from `self` (DDD boundary).
    agents: crate::repositories::agent::AgentRepository,
    context_injections: RunContextInjectionRepository,
    context_resolver: Option<Arc<ContextResolverService>>,
    context_envelopes: Option<ContextEnvelopeService>,
    context_injection_enabled: bool,
    broadcast_bus: Option<Arc<NatsClient>>,
    /// Materializes instruction images into the agent workspace at dispatch.
    /// `None` in tests and where image input is not configured (image tasks then
    /// fail closed at the create-time gate).
    image_materializer: Option<Arc<crate::services::task_image_materializer::TaskImageMaterializer>>,
}

impl OrchestrationService {
    pub fn new(task_repo: OrchestrationTaskRepository, participant_repo: ParticipantRepository) -> Self {
        let task_run_repo = TaskRunRepository::new(task_repo.pool().clone());
        let task_comment_repo = TaskCommentRepository::new(task_repo.pool().clone());
        let task_review_check_repo = TaskReviewCheckRepository::new(task_repo.pool().clone());
        let agents = crate::repositories::agent::AgentRepository::new(task_repo.pool().clone());
        let context_injections = RunContextInjectionRepository::new(task_repo.pool().clone());
        Self {
            task_repo,
            participant_repo,
            task_run_repo,
            task_comment_repo,
            task_review_check_repo,
            agents,
            context_injections,
            context_resolver: None,
            context_envelopes: None,
            context_injection_enabled: true,
            broadcast_bus: None,
            image_materializer: None,
        }
    }

    pub fn with_image_materializer(
        mut self,
        materializer: Arc<crate::services::task_image_materializer::TaskImageMaterializer>,
    ) -> Self {
        self.image_materializer = Some(materializer);
        self
    }

    pub fn from_runtime(
        pool: PgPool,
        object_storage: Arc<agentforge_infra::ObjectStorageClient>,
        workspace_root: String,
        context_features: ContextFeatureFlags,
        context_resolver: Arc<ContextResolverService>,
        nats: Arc<NatsClient>,
    ) -> Self {
        let materializer = Arc::new(crate::services::task_image_materializer::TaskImageMaterializer::new(
            Arc::new(crate::repositories::attachment::AttachmentRepository::new(pool.clone())),
            object_storage,
            workspace_root,
        ));
        Self::new(OrchestrationTaskRepository::new(pool.clone()), ParticipantRepository::new(pool))
            .with_context_runtime(context_features, context_resolver)
            .with_broadcast_bus(nats)
            .with_image_materializer(materializer)
    }

    pub fn with_context_resolver(mut self, context_resolver: Arc<ContextResolverService>) -> Self {
        self.context_envelopes =
            Some(ContextEnvelopeService::new(self.task_repo.pool().clone(), context_resolver.clone()));
        self.context_resolver = Some(context_resolver);
        self
    }

    pub fn with_context_injection_enabled(mut self, enabled: bool) -> Self {
        self.context_injection_enabled = enabled;
        if !enabled {
            self.context_resolver = None;
            self.context_envelopes = None;
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

        // An instruction with images must be explicitly assigned to a
        // vision-capable container agent, so it goes through create_task_with_assignee
        // (which materializes) and never reaches the capability-unaware
        // auto-dispatcher with unmaterialized image references.
        if assigned_to.is_none() && !crate::domain::orchestration::task_image_attachment_ids(params.as_ref()).is_empty()
        {
            return Err(crate::domain::instruction_image::images_require_assigned_vision_agent());
        }
        let missing_inputs = BlockedTaskPolicy::missing_required_inputs(params.as_ref());
        TaskDependencyPolicy::ensure_within_limit(params.as_ref())?;
        let dependencies = TaskDependencyPolicy::from_params(params.as_ref());
        let dependencies_unresolved = self.dependencies_unresolved(scope, &dependencies).await?;
        // Parent status gates child creation on waiting_dependency. Missing
        // parents become validation errors; infrastructure failures propagate.
        let parent_status = if let Some(parent_id) = parent_task_id {
            let parent = self
                .task_repo
                .find_by_id(scope, parent_id)
                .await
                .map_err(|err| TaskCreationPolicy::map_parent_lookup_error(parent_id, err))?;
            Some(parent.status.clone())
        } else {
            None
        };

        if let Some(agent_id) = assigned_to {
            TaskCreationPolicy::ensure_assigned_task_can_start(
                &missing_inputs,
                parent_status.as_deref(),
                dependencies_unresolved,
            )?;
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

        let initial_state = TaskCreationPolicy::initial_unassigned_state(
            &missing_inputs,
            requires_approval,
            parent_status.as_deref(),
            if dependencies_unresolved { &dependencies } else { &[] },
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
                    assigned_agent_id: None,
                    parent_task_id,
                    initial_status: initial_state.initial_status,
                    initial_blocked_reason: initial_state.initial_blocked_reason,
                    initial_blocked_metadata: initial_state.initial_blocked_metadata,
                    requires_approval,
                    self_fix: false,
                },
            )
            .await?;

        self.release_ready_dependency_block(scope, task, "backlog").await
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

    /// Batch-retire stale, never-started tasks in a group (`backlog`/`queued`,
    /// `progress = 0`, untouched for `older_than_days`). Governor action: the
    /// route requires an org admin and audits the operation.
    pub async fn retire_stale_tasks(
        &self,
        scope: &TenantScope,
        group_id: Uuid,
        older_than_days: Option<i32>,
        batch_limit: Option<i64>,
    ) -> AppResult<(i64, Vec<uuid::Uuid>)> {
        let (days, batch) = TaskRetirePolicy::validate(older_than_days, batch_limit)?;
        let ids = self.task_repo.retire_stale_tasks(scope, group_id, days, batch).await?;
        let count = ids.len() as i64;
        Ok((count, ids))
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
                    expected_status: current_for_guard.as_ref().map(|task| task.status.clone()),
                    expected_row_version: current_for_guard.as_ref().map(|task| task.row_version),
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
        TaskLifecyclePolicy::ensure_can_cancel(&task.status)?;
        let mut tx = self
            .task_repo
            .pool()
            .begin()
            .await
            .map_err(|err| OrchestrationTransactionPolicy::begin_failed("cancel_task", err))?;
        let updated = OrchestrationTaskRepository::cancel_in_tx(&mut tx, scope, id, task.row_version).await?;
        self.task_run_repo.finish_current_in_tx(&mut tx, scope, id, "canceled").await?;
        tx.commit().await.map_err(|err| OrchestrationTransactionPolicy::commit_failed("cancel_task", err))?;
        if let Some(agent_id) = task.assigned_agent_id
            && let Err(err) = self.participant_repo.release_if_idle(scope, agent_id).await
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
        TaskLifecyclePolicy::ensure_can_retry(
            &task.status,
            task.blocked_reason.as_deref(),
            task.requires_approval,
            task.retryable,
        )?;
        TaskDependencyPolicy::ensure_within_limit(task.params.as_ref())?;
        let missing_inputs = BlockedTaskPolicy::missing_required_inputs(task.params.as_ref());
        let dependencies = TaskDependencyPolicy::from_params(task.params.as_ref());
        let dependencies_unresolved = self.dependencies_unresolved(scope, &dependencies).await?;
        let parent_status = if let Some(parent_id) = task.parent_task_id {
            let parent = self
                .task_repo
                .find_by_id(scope, parent_id)
                .await
                .map_err(|err| TaskCreationPolicy::map_parent_lookup_error(parent_id, err))?;
            Some(parent.status)
        } else {
            None
        };
        let initial_state = TaskCreationPolicy::initial_unassigned_state(
            &missing_inputs,
            task.requires_approval,
            parent_status.as_deref(),
            if dependencies_unresolved { &dependencies } else { &[] },
        );
        let reset = self
            .task_repo
            .retry(
                scope,
                id,
                &task.status,
                task.row_version,
                initial_state.initial_status,
                initial_state.initial_blocked_reason,
                initial_state.initial_blocked_metadata,
            )
            .await?;
        let reset = self.release_ready_dependency_block(scope, reset, "backlog").await?;
        self.try_auto_dispatch(scope, reset).await
    }

    /// Dispatch a task: find an available participant, assign it, mark running.
    /// Used by both the explicit `POST /tasks/:id/dispatch` endpoint and the
    /// auto-dispatcher invoked on create / heartbeat.
    pub async fn dispatch_task(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<OrchestrationTask> {
        let task = self.task_repo.find_by_id(scope, task_id).await?;
        // Explicit operator dispatch: tolerates a #793/#875 waiting_verification
        // re-run; the auto-sweep keeps the stricter `can_enter_dispatch`.
        BlockedTaskPolicy::ensure_operator_can_dispatch(&task.status, task.blocked_reason.as_deref())?;
        self.ensure_task_prerequisites_ready(scope, &task).await?;

        let participant = self.participant_repo.find_available(scope, task.id).await?.ok_or_else(
            || -> agentforge_core::AppError { BlockedTaskPolicy::no_available_participants_error().into() },
        )?;

        self.assign_to_participant(scope, &task, &participant).await
    }

    /// True when the task declares prerequisites that are not all completed
    /// (params `dependency_ids`); such tasks must not dispatch.
    async fn prerequisites_unresolved(&self, scope: &TenantScope, task: &OrchestrationTask) -> AppResult<bool> {
        if let Some(parent_id) = task.parent_task_id {
            let parent = self
                .task_repo
                .find_by_id(scope, parent_id)
                .await
                .map_err(|err| TaskCreationPolicy::map_parent_lookup_error(parent_id, err))?;
            if BlockedTaskPolicy::needs_dependency_block(Some(&parent.status)) {
                return Ok(true);
            }
        }
        let dependencies = TaskDependencyPolicy::from_params(task.params.as_ref());
        self.dependencies_unresolved(scope, &dependencies).await
    }

    async fn release_ready_dependency_block(
        &self,
        scope: &TenantScope,
        task: OrchestrationTask,
        ready_status: &str,
    ) -> AppResult<OrchestrationTask> {
        if task.status != "blocked" || task.blocked_reason.as_deref() != Some("waiting_dependency") {
            return Ok(task);
        }
        TaskDependencyPolicy::ensure_within_limit(task.params.as_ref())?;
        if task.requires_approval
            || !BlockedTaskPolicy::missing_required_inputs(task.params.as_ref()).is_empty()
            || self.prerequisites_unresolved(scope, &task).await?
        {
            return Ok(task);
        }
        match self.task_repo.release_waiting_dependency(scope, task.id, ready_status).await? {
            Some(released) => Ok(released),
            None => self.task_repo.find_by_id(scope, task.id).await,
        }
    }

    async fn ensure_task_prerequisites_ready(&self, scope: &TenantScope, task: &OrchestrationTask) -> AppResult<()> {
        TaskDependencyPolicy::ensure_within_limit(task.params.as_ref())?;
        let parent_status = if let Some(parent_id) = task.parent_task_id {
            let parent = self
                .task_repo
                .find_by_id(scope, parent_id)
                .await
                .map_err(|err| TaskCreationPolicy::map_parent_lookup_error(parent_id, err))?;
            Some(parent.status)
        } else {
            None
        };
        let dependencies = TaskDependencyPolicy::from_params(task.params.as_ref());
        let dependencies_unresolved = self.dependencies_unresolved(scope, &dependencies).await?;
        TaskCreationPolicy::ensure_assigned_task_can_start(
            &BlockedTaskPolicy::missing_required_inputs(task.params.as_ref()),
            parent_status.as_deref(),
            dependencies_unresolved,
        )
    }

    async fn ensure_task_prerequisites_ready_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        scope: &TenantScope,
        params: Option<&serde_json::Value>,
        parent_task_id: Option<Uuid>,
    ) -> AppResult<()> {
        TaskDependencyPolicy::ensure_within_limit(params)?;
        let dependencies = TaskDependencyPolicy::from_params(params);
        let mut prerequisite_ids = dependencies.clone();
        if let Some(parent_id) = parent_task_id {
            prerequisite_ids.push(parent_id);
        }
        prerequisite_ids.sort_unstable();
        prerequisite_ids.dedup();
        let statuses = OrchestrationTaskRepository::lock_statuses_in_tx(tx, scope, &prerequisite_ids).await?;
        let parent_status = parent_task_id.map(|parent_id| {
            statuses.iter().find_map(|(id, status)| (*id == parent_id).then_some(status.as_str())).unwrap_or("missing")
        });
        TaskCreationPolicy::ensure_assigned_task_can_start(
            &BlockedTaskPolicy::missing_required_inputs(params),
            parent_status,
            TaskDependencyPolicy::unresolved(&dependencies, &statuses),
        )
    }

    async fn dependencies_unresolved(&self, scope: &TenantScope, dependencies: &[Uuid]) -> AppResult<bool> {
        if dependencies.is_empty() {
            return Ok(false);
        }
        let mut statuses: Vec<(uuid::Uuid, String)> = Vec::new();
        for dependency in dependencies {
            match self.task_repo.find_by_id(scope, *dependency).await {
                Ok(dep) => statuses.push((*dependency, dep.status.clone())),
                Err(_) => statuses.push((*dependency, "missing".to_string())),
            }
        }
        Ok(TaskDependencyPolicy::unresolved(dependencies, &statuses))
    }

    /// Try to dispatch a task to an available participant. If no participant is
    /// available the task is marked `blocked/waiting_agent` with metadata that
    /// powers the "还差 X 个 agent" hint. Returns the updated task either way.
    async fn try_auto_dispatch(&self, scope: &TenantScope, task: OrchestrationTask) -> AppResult<OrchestrationTask> {
        if !BlockedTaskPolicy::can_enter_dispatch(&task.status, task.blocked_reason.as_deref()) {
            return Ok(task);
        }
        if self.prerequisites_unresolved(scope, &task).await? {
            return Ok(task);
        }
        // An image task must be PUSH-dispatched to an explicitly chosen
        // vision-capable container agent (create_task_with_assignee / re-assign),
        // which materializes the images. This capability-blind auto-dispatcher
        // picks an arbitrary available participant, for which materialization
        // fails closed — so an image task that lost its assignee (retried, or
        // patched to queued/unassigned) would just churn on failed dispatch.
        // Block it instead so the operator re-assigns a vision-capable agent.
        if !crate::domain::orchestration::task_image_attachment_ids(task.params.as_ref()).is_empty() {
            let (available, busy, offline) = self.participant_repo.count_by_status(scope, task.id).await?;
            let metadata = BlockedTaskPolicy::waiting_agent_metadata(available, busy, offline);
            tracing::info!(task_id = %task.id, "Image task without assignee — blocked pending vision-capable assignment");
            return self
                .task_repo
                .mark_blocked_if_unchanged(scope, &task, BlockedTaskPolicy::waiting_agent_reason(), metadata)
                .await;
        }
        match self.participant_repo.find_available(scope, task.id).await? {
            Some(participant) => self.assign_to_participant(scope, &task, &participant).await,
            None => {
                let (available, busy, offline) = self.participant_repo.count_by_status(scope, task.id).await?;
                let metadata = BlockedTaskPolicy::waiting_agent_metadata(available, busy, offline);
                tracing::info!(task_id = %task.id, busy, offline, "No available participant — task blocked on waiting_agent");
                self.task_repo
                    .mark_blocked_if_unchanged(scope, &task, BlockedTaskPolicy::waiting_agent_reason(), metadata)
                    .await
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

    /// Resolve + materialize instruction images for an assignment into the
    /// agent workspace, or empty when the task has none / no materializer is
    /// configured. Fails closed (capability/workspace/kind violations error).
    /// Materialize a task's instruction images and persist the cleanup marker.
    /// Returns the container paths plus, when images were actually written, the
    /// workspace they landed in — the caller passes that back to
    /// `compensate_materialized_images` if a later step in the dispatch tx rolls
    /// back, so the non-transactional files are not orphaned.
    async fn resolve_assignment_images(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        scope: &TenantScope,
        agent_id: AgentId,
        task: &OrchestrationTask,
        participant_capabilities: &[String],
    ) -> AppResult<(Vec<String>, Option<Uuid>)> {
        let Some(materializer) = self.image_materializer.as_ref() else {
            return Ok((Vec::new(), None));
        };
        let image_ids = crate::domain::orchestration::task_image_attachment_ids(task.params.as_ref());
        if image_ids.is_empty() {
            return Ok((Vec::new(), None));
        }
        // Rolling-deploy gate: only a sidecar that advertises the image-input
        // capability understands `TaskAssignment.image_paths`. An older
        // still-running sidecar would verify the signed envelope and then ignore
        // the unknown field, running the CLI with the text prompt only. Fail
        // closed so the dispatch rolls back instead of silently dropping the
        // images; the operator rolls/restarts the agent and re-dispatches. Checked
        // BEFORE materializing, so a gate failure leaves nothing to compensate.
        if !participant_capabilities.iter().any(|c| c == agentforge_core::SIDECAR_IMAGE_INPUT_CAPABILITY) {
            return Err(crate::domain::instruction_image::sidecar_image_input_unsupported());
        }
        let agent = self.agents.find_by_id(scope, agent_id).await?;
        let workspace_id = agent.workspace_id.as_uuid();
        // Materialization is not atomic: a failure partway through may leave some
        // files behind. Remove them on error so a failed dispatch never orphans
        // a half-written directory.
        let paths = match materializer.materialize_for_dispatch(scope, &agent, task.id, &image_ids).await {
            Ok(paths) => paths,
            Err(err) => {
                self.compensate_materialized_images(scope, workspace_id, task.id);
                return Err(err);
            }
        };
        // Persist (in the dispatch tx — see the repo method) the workspace these
        // images landed in so the cleanup sweeper finds them even after the agent
        // is deleted, clearing any prior cleanup mark so a retried task's fresh
        // images are eligible again. If this write fails the tx will roll back, so
        // remove the just-materialized files now rather than orphan them.
        if let Err(err) =
            crate::repositories::orchestration::OrchestrationTaskRepository::set_task_images_workspace_in_tx(
                tx,
                scope,
                task.id,
                workspace_id,
            )
            .await
        {
            self.compensate_materialized_images(scope, workspace_id, task.id);
            return Err(err);
        }
        Ok((paths, Some(workspace_id)))
    }

    /// Best-effort removal of a task's materialized instruction images after the
    /// dispatch transaction was EXPLICITLY rolled back (a failed materialize, marker
    /// write, assignment build, or outbox insert). The DB cleanup marker rolled back
    /// with the tx, so without this the files would linger in the reused workspace
    /// with nothing for the sweeper to find. NOT called on a `commit` error, which is
    /// ambiguous — the assignment may be live and still need the files. A failure here
    /// is logged, not propagated — we are already returning the original dispatch
    /// error, and the next successful (re-)dispatch overwrites the directory anyway.
    fn compensate_materialized_images(&self, scope: &TenantScope, workspace_id: Uuid, task_id: Uuid) {
        if let Some(materializer) = self.image_materializer.as_ref()
            && let Err(err) = materializer.remove_materialized_images(scope.org_id().as_uuid(), workspace_id, task_id)
        {
            tracing::warn!(%task_id, error = %err, "failed to remove materialized images after dispatch rollback");
        }
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
            .map_err(|err| OrchestrationTransactionPolicy::begin_failed("assignment", err))?;
        let participant =
            ParticipantRepository::claim_for_task_in_tx(&mut tx, scope, task, participant.agent_id).await?;
        if let Err(err) =
            self.ensure_task_prerequisites_ready_in_tx(&mut tx, scope, task.params.as_ref(), task.parent_task_id).await
        {
            let _ = tx.rollback().await;
            return Err(err);
        }
        let assignment_agent = self.agents.find_by_id_in_tx(&mut tx, scope, participant.agent_id).await?;
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
            .ok_or_else(|| OrchestrationTransactionPolicy::missing_last_assignment_id(task.id))?;
        let idempotency_key = delivery_id.to_string();
        let resolved_context = match previewed_context {
            Some(resolved_context) => Some(resolved_context),
            None => match self.resolve_assignment_context(scope, &task, &participant).await {
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
        // Materialize instruction images into the agent workspace (symlink-safe)
        // and attach their container paths to the assignment. Fails closed: a
        // capability/workspace/kind violation rolls the dispatch back.
        let (image_paths, materialized_ws) = match self
            .resolve_assignment_images(&mut tx, scope, participant.agent_id, &task, &participant.capabilities)
            .await
        {
            Ok(resolved) => resolved,
            Err(err) => {
                let _ = tx.rollback().await;
                return Err(err);
            }
        };
        let mut assignment = match TaskAssignmentPolicy::build(
            task_assignment_snapshot(&task),
            context_envelope,
            assignment_agent.runtime_kind,
            assignment_agent.hmac_secret.as_deref(),
        ) {
            Ok(assignment) => assignment,
            Err(err) => {
                // Compensate BEFORE releasing the row lock: a retry already waiting on
                // this row must not acquire the lock and materialize fresh images that
                // this stale compensation would then delete.
                if let Some(ws) = materialized_ws {
                    self.compensate_materialized_images(scope, ws, task.id);
                }
                let _ = tx.rollback().await;
                return Err(err);
            }
        };
        assignment.image_paths = image_paths;
        // CN-4: stamp the dispatching request's trace onto the assignment so the
        // sidecar continues the same trace across the NATS hop. Captured here (at
        // enqueue) not in the outbox publisher, which runs later in a different
        // span. `None` when tracing is disabled or no span is active.
        assignment.trace_context = agentforge_telemetry::current_traceparent();
        if let Err(err) = insert_assignment_outbox_in_tx(&mut tx, scope.org_id().as_uuid(), task.id, &assignment).await
        {
            // Compensate BEFORE releasing the row lock (see the build-error arm).
            if let Some(ws) = materialized_ws {
                self.compensate_materialized_images(scope, ws, task.id);
            }
            let _ = tx.rollback().await;
            return Err(OrchestrationTransactionPolicy::insert_assignment_outbox_failed(err).into());
        }
        if let Err(err) = tx.commit().await {
            // Do NOT compensate on a commit error: it is ambiguous (the server may
            // have committed before the connection dropped and the ack was lost), so
            // deleting the directory could strand a LIVE assignment that points the
            // agent at these paths. A genuinely-aborted commit leaves the files to be
            // reclaimed by the next (re-)dispatch's overwrite instead.
            return Err(OrchestrationTransactionPolicy::commit_failed("assignment", err).into());
        }
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
        BlockedTaskPolicy::ensure_operator_can_dispatch(&task.status, task.blocked_reason.as_deref())?;
        self.ensure_task_prerequisites_ready(scope, &task).await?;

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
        BlockedTaskPolicy::ensure_operator_can_dispatch(&task.status, task.blocked_reason.as_deref())?;
        self.ensure_task_prerequisites_ready(scope, &task).await?;

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
            let Some(task) = self.task_repo.next_dispatchable(scope).await? else {
                break;
            };
            match self.try_auto_dispatch(scope, task).await {
                Ok(t) => {
                    let decision = DispatchSweepPolicy::after_dispatch_attempt(&t.status);
                    let action =
                        if decision == DispatchSweepDecision::ClaimedTask { "task.dispatched" } else { "task.blocked" };
                    self.broadcast_task_update_by_id(scope, t.id, action).await;
                    match decision {
                        DispatchSweepDecision::ClaimedTask => claimed += 1,
                        DispatchSweepDecision::Stop => break,
                    }
                }
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

        TaskLifecyclePolicy::ensure_no_active_delivery(&task.status, task.last_assignment_id)?;
        TaskLifecyclePolicy::ensure_can_complete(&task.status, task.blocked_reason.as_deref())?;

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
            .map_err(|err| OrchestrationTransactionPolicy::begin_failed("complete_task", err))?;
        let updated = OrchestrationTaskRepository::set_result_in_tx(
            &mut tx,
            scope,
            task_id,
            task.row_version,
            "completed",
            result,
        )
        .await?;
        self.task_run_repo.finish_current_in_tx(&mut tx, scope, task_id, "completed").await?;
        let unblocked_children =
            OrchestrationTaskRepository::unblock_children_of_in_tx(&mut tx, scope, task_id).await?;

        // Event-driven self-fix trigger: when this task is a self-fix task,
        // enqueue the PR-bridge job inside the SAME transaction as the result
        // write. The job commits atomically with the completion, so a crash
        // never loses the trigger and it never fires for an uncommitted
        // completion. `unique_key = task_id` makes re-completion idempotent
        // (ON CONFLICT DO NOTHING).
        if updated.self_fix {
            let payload = self_fix_pr_job_payload(task_id, scope.org_id().as_uuid())?;
            agentforge_jobs::queue::enqueue_in_tx(
                &mut tx,
                agentforge_core::SELF_FIX_PR_QUEUE,
                payload,
                0,
                None,
                Some(&task_id.to_string()),
                5,
            )
            .await
            .map_err(|err| agentforge_core::AppError::from(anyhow::Error::from(err)))?;
        }

        tx.commit().await.map_err(|err| OrchestrationTransactionPolicy::commit_failed("complete_task", err))?;

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
            && let Err(err) = self.participant_repo.release_if_idle(scope, agent_id).await
        {
            tracing::error!(error = ?err, agent_id = %agent_id, "Failed to release participant after completion");
        }
        // Released agent → re-sweep so a queued task picks them up immediately.
        // Outside the tx on purpose: sweep is best-effort and shouldn't fail completion.
        if let Err(err) = self.sweep_dispatchable(scope).await {
            tracing::error!(error = ?err, task_id = %task_id, "Post-completion sweep failed");
        }
        for child in &unblocked_children {
            self.broadcast_task_update_by_id(scope, child.id, "task.dependencies_ready").await;
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

        TaskLifecyclePolicy::ensure_no_active_delivery(&task.status, task.last_assignment_id)?;
        TaskLifecyclePolicy::ensure_can_fail(&task.status)?;

        if let Some(metadata) = QuotaBlockPolicy::metadata(&error) {
            let mut tx = self
                .task_repo
                .pool()
                .begin()
                .await
                .map_err(|err| OrchestrationTransactionPolicy::begin_failed("quota block", err))?;
            let updated = OrchestrationTaskRepository::mark_blocked_retryable_in_tx(
                &mut tx,
                scope,
                task_id,
                task.row_version,
                "quota_exceeded",
                metadata,
                error,
            )
            .await?;
            self.task_run_repo.finish_current_in_tx(&mut tx, scope, task_id, "failed").await?;
            upsert_task_owner_lifecycle_notification_in_tx(&mut tx, &updated, None, TaskOwnerNotificationKind::Blocked)
                .await?;
            tx.commit().await.map_err(|err| OrchestrationTransactionPolicy::commit_failed("quota block", err))?;
            if let Some(agent_id) = task.assigned_agent_id
                && let Err(err) = self.participant_repo.release_if_idle(scope, agent_id).await
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
            .map_err(|err| OrchestrationTransactionPolicy::begin_failed("fail_task", err))?;
        let updated =
            OrchestrationTaskRepository::set_result_in_tx(&mut tx, scope, task_id, task.row_version, "failed", error)
                .await?;
        self.task_run_repo.finish_current_in_tx(&mut tx, scope, task_id, "failed").await?;
        upsert_task_owner_lifecycle_notification_in_tx(&mut tx, &updated, None, TaskOwnerNotificationKind::Failed)
            .await?;
        tx.commit().await.map_err(|err| OrchestrationTransactionPolicy::commit_failed("fail_task", err))?;
        if let Some(agent_id) = task.assigned_agent_id
            && let Err(err) = self.participant_repo.release_if_idle(scope, agent_id).await
        {
            tracing::error!(error = ?err, agent_id = %agent_id, "Failed to release participant after failure");
        }
        if let Err(err) = self.sweep_dispatchable(scope).await {
            tracing::error!(error = ?err, task_id = %task_id, "Post-failure sweep failed");
        }
        Ok(updated)
    }

    /// Approve a task created with `requiresApproval=true`. Approval clears the
    /// human gate, then either queues the task or keeps it blocked until every
    /// parent and explicit prerequisite is complete. Queued tasks immediately
    /// re-enter auto-dispatch.
    pub async fn approve_task(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<OrchestrationTask> {
        let task = self.task_repo.find_by_id(scope, task_id).await?;
        TaskLifecyclePolicy::ensure_can_approve(&task.status, task.blocked_reason.as_deref(), task.requires_approval)?;

        let parent_status = if let Some(parent_id) = task.parent_task_id {
            Some(self.task_repo.find_by_id(scope, parent_id).await?.status)
        } else {
            None
        };
        TaskDependencyPolicy::ensure_within_limit(task.params.as_ref())?;
        let dependencies = TaskDependencyPolicy::from_params(task.params.as_ref());
        let dependencies_unresolved = self.dependencies_unresolved(scope, &dependencies).await?;
        let release_state = TaskCreationPolicy::initial_unassigned_state(
            &[],
            false,
            parent_status.as_deref(),
            if dependencies_unresolved { &dependencies } else { &[] },
        );
        let next_status =
            if release_state.initial_status == "backlog" { "queued" } else { release_state.initial_status };
        let approved = self
            .task_repo
            .approve_waiting_task(
                scope,
                task_id,
                task.row_version,
                scope.user_id(),
                next_status,
                release_state.initial_blocked_reason,
                release_state.initial_blocked_metadata,
            )
            .await?;
        let approved = self.release_ready_dependency_block(scope, approved, "queued").await?;
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
        let mut context_counts = self.context_injections.count_by_tasks(scope, &task_ids).await?;
        let wait_estimates = self.queued_wait_estimates(scope, &task_ids).await?;
        Ok(tasks
            .into_iter()
            .map(|t| {
                let name = t.assigned_agent_id.and_then(|a| names.get(&a.as_uuid()).cloned());
                let mut summary = task_summary(t, name);
                if let Some(counts) = context_counts.remove(&summary.id) {
                    summary.context_counts = counts.into();
                }
                if let Some(estimate) = wait_estimates.get(&summary.id) {
                    summary.wait_estimate = Some(estimate.clone());
                }
                summary
            })
            .collect())
    }

    /// Org-scoped queued-wait predictions for the given task ids.
    ///
    /// One queue snapshot + one median-duration query per read. Position is
    /// measured inside the task's own dispatch lane — the same agent for
    /// assigned tasks, the shared pool for unassigned ones — matching the
    /// order the auto-dispatcher actually drains (`urgent` first, then age).
    async fn queued_wait_estimates(
        &self,
        scope: &TenantScope,
        task_ids: &[Uuid],
    ) -> AppResult<HashMap<Uuid, TaskWaitEstimate>> {
        if task_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let queue = self.task_repo.queued_tasks_ordered(scope).await?;
        if queue.is_empty() {
            return Ok(HashMap::new());
        }
        let typical = self.task_repo.typical_wait_seconds(scope).await?;
        let wanted: std::collections::HashSet<Uuid> = task_ids.iter().copied().collect();
        let mut positions: HashMap<Option<Uuid>, u32> = HashMap::new();
        let mut estimates = HashMap::new();
        for key in &queue {
            let lane = key.assigned_agent_id.map(|a| a.as_uuid());
            let position = positions.entry(lane).or_insert(0);
            *position += 1;
            if wanted.contains(&key.id) {
                estimates.insert(key.id, TaskWaitEstimatePolicy::estimate(*position, typical));
            }
        }
        Ok(estimates)
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
        if let Some(counts) = self.context_injections.count_by_tasks(scope, &[summary.id]).await?.remove(&summary.id) {
            summary.context_counts = counts.into();
        }
        if let Some(estimate) = self.queued_wait_estimates(scope, &[summary.id]).await?.remove(&summary.id) {
            summary.wait_estimate = Some(estimate);
        }
        Ok(summary)
    }

    pub async fn list_task_runs(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<Vec<TaskRunSummary>> {
        self.task_repo.find_by_id(scope, task_id).await?;
        let runs = self.task_run_repo.list_by_task(scope, task_id).await?;
        Ok(runs.into_iter().map(task_run_summary).collect())
    }

    /// Human updates for a task, oldest first. Independent of execution
    /// attempts and lifecycle state.
    pub async fn list_task_comments(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<Vec<TaskCommentSummary>> {
        self.task_repo.find_by_id(scope, task_id).await?;
        let rows = self.task_comment_repo.list_by_task(scope, task_id).await?;
        Ok(rows.into_iter().map(task_comment_summary).collect())
    }

    /// Add a human update (comment / blocker / unblock) to a task.
    pub async fn create_task_comment(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        kind: Option<&str>,
        body: &str,
    ) -> AppResult<TaskCommentSummary> {
        let kind = kind.unwrap_or("comment");
        if !matches!(kind, "comment" | "blocker" | "unblock") {
            return Err(OrchestrationRepositoryPolicy::invalid_task_comment_kind(kind));
        }
        if body.trim().is_empty() {
            return Err(OrchestrationRepositoryPolicy::empty_task_comment_body());
        }
        self.task_repo.find_by_id(scope, task_id).await?;
        let row = self
            .task_comment_repo
            .create(scope, task_id, scope.user_id(), kind, body.trim())
            .await?
            .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(task_id))?;
        Ok(task_comment_summary(row))
    }

    /// Latest blocker / unblock signals for a set of tasks (board badges).
    /// Bounded: the board never sends more than 300 task ids.
    pub async fn latest_human_marks(
        &self,
        scope: &TenantScope,
        task_ids: &[Uuid],
    ) -> AppResult<Vec<HumanMarkerSummary>> {
        if task_ids.len() > 300 {
            return Err(OrchestrationRepositoryPolicy::task_marker_list_too_large(300));
        }
        Ok(self
            .task_comment_repo
            .latest_marker_by_tasks(scope, task_ids)
            .await?
            .into_iter()
            .map(human_marker_summary)
            .collect())
    }

    /// Human review checklist for a task (the current user's ticks).
    pub async fn list_task_review_checks(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
    ) -> AppResult<Vec<TaskReviewCheckSummary>> {
        self.task_repo.find_by_id(scope, task_id).await?;
        let rows = self.task_review_check_repo.list_by_task(scope, task_id, scope.user_id()).await?;
        Ok(rows.into_iter().map(task_review_check_summary).collect())
    }

    /// Required-acceptance gates for a task: which keys are required and
    /// whether they are all ticked by any reviewer.
    pub async fn review_gate_status(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        required: &[String],
    ) -> AppResult<ReviewGateStatus> {
        self.task_repo.find_by_id(scope, task_id).await?;
        let missing = self.task_review_check_repo.undone_required_gates(scope, task_id, required).await?;
        Ok(ReviewGateStatus { required_keys: required.to_vec(), satisfied: missing.is_empty(), missing })
    }

    /// Refuse a human 'mark completed' until every required review gate is
    /// ticked (a key ticked by any reviewer counts for the task).
    pub async fn assert_review_gates(&self, scope: &TenantScope, task_id: Uuid, required: &[String]) -> AppResult<()> {
        if required.is_empty() {
            return Ok(());
        }
        let missing = self.task_review_check_repo.undone_required_gates(scope, task_id, required).await?;
        if missing.is_empty() {
            return Ok(());
        }
        Err(ReviewGatePolicy::incomplete_error(&missing).into())
    }

    /// Set (or unset) one review check for the current user.
    pub async fn set_task_review_check(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        check_key: &str,
        done: bool,
    ) -> AppResult<TaskReviewCheckSummary> {
        let key = check_key.trim();
        if key.is_empty() || key.len() > 64 {
            return Err(OrchestrationRepositoryPolicy::invalid_task_review_check_key(check_key));
        }
        self.task_repo.find_by_id(scope, task_id).await?;
        let row = self
            .task_review_check_repo
            .set_check(scope, task_id, scope.user_id(), key, done)
            .await?
            .ok_or_else(|| OrchestrationRepositoryPolicy::task_not_found(task_id))?;
        Ok(task_review_check_summary(row))
    }

    /// Compliance export of task history as CSV (newest first, capped).
    pub async fn export_task_history_csv(&self, scope: &TenantScope, limit: Option<i64>) -> AppResult<(String, usize)> {
        let limit = limit.unwrap_or(500).clamp(1, 1000);
        let rows = self.task_repo.export_task_history(scope, limit).await?;
        let projections: Vec<TaskHistoryExportRowProjection> = rows.into_iter().map(task_history_projection).collect();
        let raw_count = projections.len();
        Ok((task_history_csv(&projections), raw_count))
    }

    /// Delete the caller's own comment on a task. 404 when the comment is
    /// missing (or belongs to another task); 403 when it belongs to another
    /// person.
    pub async fn delete_task_comment(&self, scope: &TenantScope, task_id: Uuid, comment_id: Uuid) -> AppResult<()> {
        self.task_repo.find_by_id(scope, task_id).await?;
        let row = self
            .task_comment_repo
            .find_with_author(scope, comment_id)
            .await?
            .ok_or_else(|| OrchestrationRepositoryPolicy::task_comment_not_found(comment_id))?;
        if row.task_id != task_id {
            return Err(OrchestrationRepositoryPolicy::task_comment_not_found(comment_id));
        }
        if row.author_user_id != scope.user_id() {
            return Err(OrchestrationRepositoryPolicy::forbidden());
        }
        if !self.task_comment_repo.delete(scope, comment_id).await? {
            return Err(OrchestrationRepositoryPolicy::task_comment_not_found(comment_id));
        }
        Ok(())
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

    /// Load a task by id (tenant-scoped), build its summary, and broadcast a
    /// `orchestration:task_update` frame so connected clients refresh in
    /// realtime. Best-effort: a missing task, a summarize failure, or a NATS
    /// hiccup is logged and swallowed — it never turns a successful mutation
    /// into a failed request. Used by surfaces that change a task OUTSIDE the
    /// orchestration routes (e.g. the self-fix approve→merge transition, which
    /// flips `review_status` to `merged`) and still want every operator's board
    /// and Review tab to reflect it without a manual refetch.
    pub(crate) async fn broadcast_task_update_by_id(&self, scope: &TenantScope, task_id: Uuid, action: &str) {
        // Attempt trace: failures below warn, but a successful broadcast is
        // otherwise silent. This debug line gives ops a thread to pull on when
        // diagnosing "I merged but my colleague's board didn't update" — it
        // shows the push WAS attempted, so the gap is downstream (NATS / client
        // subscription), not a missing emit.
        tracing::debug!(%task_id, %action, "broadcast_task_update_by_id: emitting task update");
        let task = match self.task_repo.find_by_id(scope, task_id).await {
            Ok(task) => task,
            Err(err) => {
                tracing::warn!(error = ?err, %task_id, %action, "broadcast_task_update_by_id: task load failed");
                return;
            }
        };
        let summary = match self.summarize_task(scope, task).await {
            Ok(summary) => summary,
            Err(err) => {
                tracing::warn!(error = ?err, %task_id, %action, "broadcast_task_update_by_id: summarize failed");
                return;
            }
        };
        self.broadcast_task_update(scope, action, &summary).await;
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
            .map_err(|err| OrchestrationTransactionPolicy::begin_failed("create assigned task", err))?;
        agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, agent_id.as_uuid()).await?;
        let participant = ParticipantRepository::find_by_agent_id_in_tx(&mut tx, scope, agent_id).await?;
        let assignment_agent = self.agents.find_by_id_in_tx(&mut tx, scope, agent_id).await?;
        if let Err(err) = ParticipantAvailabilityPolicy::ensure_available(
            &participant.name,
            &participant.status,
            ParticipantAvailabilityAction::AssignTask,
        ) {
            let _ = tx.rollback().await;
            return Err(err);
        }
        if let Err(err) =
            self.ensure_task_prerequisites_ready_in_tx(&mut tx, scope, params.as_ref(), parent_task_id).await
        {
            let _ = tx.rollback().await;
            return Err(err);
        }

        // The lookup above locks participant -> agent before any task FK lock.
        // Keep the uncommitted insert unassigned; the claim below owns assignment.
        let task = OrchestrationTaskRepository::create_in_tx(
            &mut tx,
            scope,
            CreateTaskRow {
                group_id,
                title,
                description,
                priority,
                params: params.as_ref(),
                assigned_agent_id: None,
                parent_task_id,
                initial_status: "queued",
                initial_blocked_reason: None,
                initial_blocked_metadata: None,
                requires_approval: false,
                self_fix: false,
            },
        )
        .await?;
        let participant = ParticipantRepository::claim_for_task_in_tx(&mut tx, scope, &task, agent_id).await?;
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
            .ok_or_else(|| OrchestrationTransactionPolicy::missing_last_assignment_id(task.id))?;
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
        // Materialize instruction images into the agent workspace (symlink-safe)
        // and attach their container paths to the assignment. Fails closed: a
        // capability/workspace/kind violation rolls the dispatch back.
        let (image_paths, materialized_ws) = match self
            .resolve_assignment_images(&mut tx, scope, participant.agent_id, &task, &participant.capabilities)
            .await
        {
            Ok(resolved) => resolved,
            Err(err) => {
                let _ = tx.rollback().await;
                return Err(err);
            }
        };
        let mut assignment = match TaskAssignmentPolicy::build(
            task_assignment_snapshot(&task),
            context_envelope,
            assignment_agent.runtime_kind,
            assignment_agent.hmac_secret.as_deref(),
        ) {
            Ok(assignment) => assignment,
            Err(err) => {
                // Compensate BEFORE releasing the row lock: a retry already waiting on
                // this row must not acquire the lock and materialize fresh images that
                // this stale compensation would then delete.
                if let Some(ws) = materialized_ws {
                    self.compensate_materialized_images(scope, ws, task.id);
                }
                let _ = tx.rollback().await;
                return Err(err);
            }
        };
        assignment.image_paths = image_paths;
        // CN-4: stamp the dispatching request's trace onto the assignment so the
        // sidecar continues the same trace across the NATS hop. Captured here (at
        // enqueue) not in the outbox publisher, which runs later in a different
        // span. `None` when tracing is disabled or no span is active.
        assignment.trace_context = agentforge_telemetry::current_traceparent();
        if let Err(err) = insert_assignment_outbox_in_tx(&mut tx, scope.org_id().as_uuid(), task.id, &assignment).await
        {
            // Compensate BEFORE releasing the row lock (see the build-error arm).
            if let Some(ws) = materialized_ws {
                self.compensate_materialized_images(scope, ws, task.id);
            }
            let _ = tx.rollback().await;
            return Err(OrchestrationTransactionPolicy::insert_assignment_outbox_failed(err).into());
        }
        if let Err(err) = tx.commit().await {
            // Do NOT compensate on a commit error: it is ambiguous (the server may
            // have committed before the connection dropped and the ack was lost), so
            // deleting the directory could strand a LIVE assignment that points the
            // agent at these paths. A genuinely-aborted commit leaves the files to be
            // reclaimed by the next (re-)dispatch's overwrite instead.
            return Err(OrchestrationTransactionPolicy::commit_failed("create assigned task", err).into());
        }
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
        let Some(context_envelopes) = &self.context_envelopes else {
            return Ok(None);
        };
        context_envelopes
            .build_from_resolved(&scope.scoped_read(), task.id, run.id, agent_id, resolved_context)
            .await
            .map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    #[test]
    fn task_run_image_summary_reads_captured_camel_case_identity() {
        let image = task_run_image_summary(&json!({
            "image": {
                "source": "ghcr.io/example/agent-codex@sha256:manifest",
                "imageId": "sha256:image",
                "manifestDigest": "sha256:manifest",
                "version": "1.2.3",
                "versionSource": "docker-label",
                "trust": "verified-signature"
            }
        }))
        .expect("valid image evidence");

        assert_eq!(image.image_id, "sha256:image");
        assert_eq!(image.manifest_digest.as_deref(), Some("sha256:manifest"));
        assert_eq!(image.trust.as_deref(), Some("verified-signature"));
        assert!(task_run_image_summary(&json!({ "capabilities": [] })).is_none());
    }

    #[test]
    fn run_projection_reads_canonical_nested_runtime_capability() {
        let profile = json!({
            "runtime_capability": {
                "runtime_kind": "container",
                "cli_tool": "codex",
                "max_context_tokens": 200_000
            }
        });

        assert_eq!(string_value(&profile, "runtime_kind").as_deref(), Some("container"));
        assert_eq!(string_value(&profile, "cli_tool").as_deref(), Some("codex"));
        assert_eq!(capability_value(&profile, "max_context_tokens").and_then(serde_json::Value::as_u64), Some(200_000));
        assert!(task_run_image_summary(&profile).is_none());
    }

    #[test]
    fn task_summary_projects_kanban_response_and_inlines_blocked_hint() {
        use agentforge_core::{OrgId, UserId};

        let task_id = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
        let agent_id = AgentId::from(Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap());
        let created_at = chrono::DateTime::parse_from_rfc3339("2026-05-18T10:00:00Z").unwrap().with_timezone(&Utc);
        let updated_at = chrono::DateTime::parse_from_rfc3339("2026-05-18T10:01:00Z").unwrap().with_timezone(&Utc);
        let task = OrchestrationTask {
            id: task_id,
            organization_id: OrgId::new(),
            group_id: None,
            title: "Title".to_string(),
            description: Some("Description".to_string()),
            status: "blocked".to_string(),
            priority: "high".to_string(),
            progress: 42,
            params: Some(json!({ "task": "Run", "message": "Use context" })),
            created_by: UserId::new(),
            assigned_agent_id: Some(agent_id),
            parent_task_id: None,
            result: None,
            error: Some(json!({ "message": "boom" })),
            blocked_reason: Some("waiting_input".to_string()),
            blocked_metadata: Some(json!({ "missing": ["api_key"] })),
            requires_approval: false,
            approved_at: None,
            approved_by: None,
            attempt: 1,
            lease_expires_at: None,
            failure_code: None,
            retryable: true,
            last_assignment_id: None,
            started_at: None,
            completed_at: None,
            canceled_at: None,
            created_at,
            updated_at,
            row_version: 0,
            self_fix: false,
            base_commit_sha: None,
            pr_number: None,
            pr_url: None,
            pr_head_sha: None,
            review_status: None,
            merge_attempts: 0,
            review_opened_at: None,
        };

        let summary = task_summary(task, Some("Atlas".to_string()));

        assert_eq!(summary.id, task_id);
        assert_eq!(summary.state, "blocked");
        assert_eq!(summary.method, "tasks/send");
        assert_eq!(summary.params.task, "Run");
        assert_eq!(summary.params.message, "Use context");
        assert_eq!(summary.priority, "high");
        assert_eq!(summary.progress, 42);
        assert_eq!(summary.assigned_to, Some(agent_id.as_uuid()));
        assert_eq!(summary.assigned_agent_name.as_deref(), Some("Atlas"));
        assert_eq!(summary.error.as_deref(), Some("boom"));
        assert_eq!(summary.blocked_reason.as_deref(), Some("waiting_input"));
        assert!(summary.blocked_hint.is_some(), "blocked tasks get a hint");
        assert_eq!(summary.created_at, "2026-05-18T10:00:00+00:00");
        assert_eq!(summary.updated_at, "2026-05-18T10:01:00+00:00");
        assert!(summary.completed_at.is_none(), "non-completed status drops completed_at");
        assert_eq!(summary.context_counts.total, 0, "context counts initialize empty");
    }

    #[test]
    fn task_assignment_snapshot_borrows_orchestration_task_fields() {
        use agentforge_core::{OrgId, UserId};

        let task_id = Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap();
        let agent_id = AgentId::from(Uuid::parse_str("55555555-5555-5555-5555-555555555555").unwrap());
        let delivery_id = Uuid::parse_str("66666666-6666-6666-6666-666666666666").unwrap();
        let lease_expires_at = Utc::now();
        let params = json!({ "task": "Execute", "message": "Use context" });
        let now = Utc::now();
        let task = OrchestrationTask {
            id: task_id,
            organization_id: OrgId::new(),
            group_id: None,
            title: "Snapshot title".to_string(),
            description: Some("Snapshot description".to_string()),
            status: "queued".to_string(),
            priority: "high".to_string(),
            progress: 0,
            params: Some(params.clone()),
            created_by: UserId::new(),
            assigned_agent_id: Some(agent_id),
            parent_task_id: None,
            result: None,
            error: None,
            blocked_reason: None,
            blocked_metadata: None,
            requires_approval: false,
            approved_at: None,
            approved_by: None,
            attempt: 2,
            lease_expires_at: Some(lease_expires_at),
            failure_code: None,
            retryable: true,
            last_assignment_id: Some(delivery_id),
            started_at: None,
            completed_at: None,
            canceled_at: None,
            created_at: now,
            updated_at: now,
            row_version: 0,
            self_fix: false,
            base_commit_sha: None,
            pr_number: None,
            pr_url: None,
            pr_head_sha: None,
            review_status: None,
            merge_attempts: 0,
            review_opened_at: None,
        };

        let snapshot = task_assignment_snapshot(&task);

        assert_eq!(snapshot.task_id, task_id);
        assert_eq!(snapshot.assigned_agent_id, Some(agent_id));
        assert_eq!(snapshot.last_assignment_id, Some(delivery_id));
        assert_eq!(snapshot.lease_expires_at, Some(lease_expires_at));
        assert_eq!(snapshot.attempt, 2);
        assert_eq!(snapshot.title, "Snapshot title");
        assert_eq!(snapshot.description, Some("Snapshot description"));
        assert_eq!(snapshot.params, Some(&params));
        assert_eq!(snapshot.priority, "high");
    }

    #[test]
    fn task_summary_copies_attempt_and_lease_expires_at_from_row() {
        use agentforge_core::{OrgId, UserId};

        let lease_ts = chrono::DateTime::parse_from_rfc3339("2026-06-22T09:00:00Z").unwrap().with_timezone(&Utc);
        let now = chrono::DateTime::parse_from_rfc3339("2026-06-22T08:00:00Z").unwrap().with_timezone(&Utc);

        let task = OrchestrationTask {
            id: Uuid::from_u128(1),
            organization_id: OrgId::new(),
            group_id: None,
            title: "Retry task".to_string(),
            description: None,
            status: "working".to_string(),
            priority: "normal".to_string(),
            progress: 0,
            params: Some(json!({ "task": "Do work", "message": "context" })),
            created_by: UserId::new(),
            assigned_agent_id: None,
            parent_task_id: None,
            result: None,
            error: None,
            blocked_reason: None,
            blocked_metadata: None,
            requires_approval: false,
            approved_at: None,
            approved_by: None,
            attempt: 3,
            lease_expires_at: Some(lease_ts),
            failure_code: None,
            retryable: true,
            last_assignment_id: None,
            started_at: None,
            completed_at: None,
            canceled_at: None,
            created_at: now,
            updated_at: now,
            row_version: 0,
            self_fix: false,
            base_commit_sha: None,
            pr_number: None,
            pr_url: None,
            pr_head_sha: None,
            review_status: None,
            merge_attempts: 0,
            review_opened_at: None,
        };

        let summary = task_summary(task, None);

        assert_eq!(summary.attempt, 3, "attempt must be copied from the row");
        assert_eq!(
            summary.lease_expires_at.as_deref(),
            Some("2026-06-22T09:00:00+00:00"),
            "lease_expires_at must be RFC3339 serialized"
        );
    }
}
