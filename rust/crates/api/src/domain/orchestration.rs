//! Orchestration domain rules.
//!
//! This module owns kanban task, participant, dispatch, and blocking policies
//! that are independent of SQL repositories, transactions, context injection,
//! and outbox delivery.

use std::collections::HashMap;

use agentforge_core::context_envelope::ContextEnvelope;
use agentforge_core::orchestration_protocol::{TaskAssignment, container_generation_fingerprint};
use agentforge_core::ws_protocol::{OrchestrationTaskUpdatePayload, ServerMessage};
use agentforge_core::{AgentId, AppError, AppResult, ErrorKind, OrgId, RuntimeKind, TenantScope, WorkspaceId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::context_resolver::ResolvedContext;

// MS-3 PR-E: the wire projection types (`TaskSummary`, `TaskContextCounts`,
// `TaskParams`) and the pure policies feeding them (`BlockedTaskPolicy`,
// `TaskInstruction`) moved to `agentforge_core` so the jobs WS projector builds
// the same shapes instead of hand-rolled mirrors. Re-exported here so api
// routes/services/tests keep their import paths.
pub(crate) use agentforge_core::orchestration_view::{BlockedTaskPolicy, TaskInstruction};
pub use agentforge_core::ws_protocol::{TaskContextCounts, TaskSummary, TaskWaitEstimate};

const VALID_TASK_STATUSES: &[&str] = &["backlog", "queued", "working", "blocked", "completed", "failed", "canceled"];
const KANBAN_DROP_STATUSES: &[&str] = &["backlog", "queued", "working", "blocked", "completed"];
const VALID_PRIORITIES: &[&str] = &["low", "normal", "high", "urgent"];
const VALID_PARTICIPANT_STATUSES: &[&str] = &["available", "busy", "offline"];

/// Batch retirement parameters: stale (never-started, untouched) tasks in a
/// group older than `older_than_days` can be retired to `canceled` in one go.
pub(crate) struct TaskRetirePolicy;

impl TaskRetirePolicy {
    pub(crate) const MIN_DAYS: i32 = 1;
    pub(crate) const MAX_DAYS: i32 = 90;
    pub(crate) const DEFAULT_DAYS: i32 = 7;
    pub(crate) const MIN_BATCH: i64 = 1;
    pub(crate) const MAX_BATCH: i64 = 500;
    pub(crate) const DEFAULT_BATCH: i64 = 100;

    /// Validate + normalise operator inputs (reject nonsense, fill defaults).
    pub(crate) fn validate(older_than_days: Option<i32>, batch_limit: Option<i64>) -> AppResult<(i32, i64)> {
        let days = older_than_days.unwrap_or(Self::DEFAULT_DAYS);
        if !(Self::MIN_DAYS..=Self::MAX_DAYS).contains(&days) {
            return Err(ErrorKind::Validation(format!(
                "olderThanDays must be between {MIN} and {MAX}",
                MIN = Self::MIN_DAYS,
                MAX = Self::MAX_DAYS
            ))
            .into());
        }
        let batch = batch_limit.unwrap_or(Self::DEFAULT_BATCH);
        if !(Self::MIN_BATCH..=Self::MAX_BATCH).contains(&batch) {
            return Err(ErrorKind::Validation(format!(
                "batchLimit must be between {MIN} and {MAX}",
                MIN = Self::MIN_BATCH,
                MAX = Self::MAX_BATCH
            ))
            .into());
        }
        Ok((days, batch))
    }
}

/// Validated pagination request for orchestration task lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TaskListPage {
    limit: i64,
    offset: i64,
}

impl TaskListPage {
    pub(crate) fn new(limit: i64, offset: i64) -> Self {
        Self { limit: limit.clamp(1, 100), offset: offset.max(0) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn offset(self) -> i64 {
        self.offset
    }
}

/// Pure queued-time prediction policy (units: seconds).
///
/// The wire estimate is `position x per-task time`, where per-task time is the
/// org median duration, or a declared default when no completed-task history
/// exists yet (`typical_seconds = 0` signals "no history" to the UI so the
/// tooltip can say the estimate is a rough guess).
pub(crate) struct TaskWaitEstimatePolicy;

impl TaskWaitEstimatePolicy {
    /// Assumed per-task time (s) when the org has no completed-task history.
    pub(crate) const DEFAULT_TYPICAL_SECONDS: u32 = 300;

    pub(crate) fn estimate(position: u32, typical_seconds: Option<u32>) -> TaskWaitEstimate {
        let typical = typical_seconds.unwrap_or(0);
        let per_task = if typical > 0 { typical } else { Self::DEFAULT_TYPICAL_SECONDS };
        TaskWaitEstimate { position, typical_seconds: typical, estimated_seconds: position.saturating_mul(per_task) }
    }
}

/// Task title value object.
pub(crate) struct TaskTitle;

impl TaskTitle {
    pub(crate) fn validate(title: &str) -> AppResult<()> {
        if title.is_empty() || title.len() > 500 {
            return Err(ErrorKind::Validation("title must be between 1 and 500 characters".into()).into());
        }
        Ok(())
    }
}

/// Review acceptance gate policy: which checklist keys are required before
/// a human may mark a task completed.
pub(crate) struct ReviewGatePolicy;

impl ReviewGatePolicy {
    /// Split the comma-separated `REVIEW_REQUIRED_GATES` config (unknown
    /// keys already fail at boot in core).
    pub(crate) fn parse(csv: Option<&str>) -> Vec<String> {
        csv.unwrap_or_default().split(',').map(str::trim).filter(|entry| !entry.is_empty()).map(str::to_owned).collect()
    }

    /// User-facing error when required gates are unfinished.
    pub(crate) fn incomplete_error(keys: &[String]) -> ErrorKind {
        ErrorKind::Validation(format!(
            "Finish the required review checks before completing this task: {}. Tick them in the review checklist, then try again.",
            keys.join(", "),
        ))
    }
}

/// Declared prerequisites ("wait for tasks"): up to 10 task ids carried in
/// `params.dependency_ids`; a task with unresolved prereqs starts blocked and
/// is released when every listed task is completed.
pub(crate) struct TaskDependencyPolicy;

impl TaskDependencyPolicy {
    pub(crate) const MAX: usize = 10;

    pub(crate) fn ensure_within_limit(params: Option<&serde_json::Value>) -> AppResult<()> {
        let count =
            params.and_then(|value| value.get("dependency_ids")).and_then(|value| value.as_array()).map_or(0, Vec::len);
        if count > Self::MAX {
            return Err(ErrorKind::Validation(format!("wait for at most {} prerequisite tasks", Self::MAX)).into());
        }
        Ok(())
    }

    pub(crate) fn from_params(params: Option<&serde_json::Value>) -> Vec<Uuid> {
        let Some(items) = params.and_then(|value| value.get("dependency_ids")).and_then(|value| value.as_array())
        else {
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|value| value.as_str())
            .filter_map(|text| Uuid::parse_str(text).ok())
            .take(Self::MAX)
            .collect()
    }

    /// A prerequisite is resolved only by `completed` (failed/canceled does not
    /// release dependents — a person must re-plan).
    pub(crate) fn unresolved(dependencies: &[Uuid], task_statuses: &[(Uuid, String)]) -> bool {
        dependencies
            .iter()
            .any(|id| !task_statuses.iter().any(|(candidate, status)| candidate == id && status == "completed"))
    }
}

/// Task priority policy.
pub(crate) struct TaskPriority;

impl TaskPriority {
    pub(crate) fn validate(priority: &str) -> AppResult<&str> {
        if !Self::is_valid(priority) {
            return Err(ErrorKind::Validation(format!("invalid priority: {priority}")).into());
        }
        Ok(priority)
    }

    pub(crate) fn is_valid(priority: &str) -> bool {
        VALID_PRIORITIES.contains(&priority)
    }
}

/// Task creation invariants.
pub(crate) struct TaskCreationPolicy;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TaskCreationInitialState {
    pub(crate) initial_status: &'static str,
    pub(crate) initial_blocked_reason: Option<&'static str>,
    pub(crate) initial_blocked_metadata: Option<serde_json::Value>,
}

impl TaskCreationPolicy {
    pub(crate) fn parent_task_not_found(parent_id: Uuid) -> ErrorKind {
        ErrorKind::Validation(format!("parent task {parent_id} not found"))
    }

    pub(crate) fn map_parent_lookup_error(parent_id: Uuid, err: AppError) -> AppError {
        match err.kind {
            ErrorKind::NotFound(_) => Self::parent_task_not_found(parent_id).into(),
            _ => err,
        }
    }

    pub(crate) fn ensure_approval_task_is_unassigned(
        requires_approval: bool,
        assigned_to: Option<AgentId>,
    ) -> AppResult<()> {
        if requires_approval && assigned_to.is_some() {
            return Err(ErrorKind::Validation(
                "requiresApproval tasks cannot be assigned before approval; approve then dispatch".into(),
            )
            .into());
        }
        Ok(())
    }

    pub(crate) fn ensure_assigned_task_can_start(
        missing_inputs: &[String],
        parent_status: Option<&str>,
        dependencies_unresolved: bool,
    ) -> AppResult<()> {
        if !missing_inputs.is_empty() {
            return Err(ErrorKind::Validation(format!(
                "add the missing inputs {} before assigning an Agent, or leave the task unassigned",
                missing_inputs.join(", ")
            ))
            .into());
        }
        if BlockedTaskPolicy::needs_dependency_block(parent_status) {
            return Err(ErrorKind::Validation(
                "wait for the parent task to finish before assigning an Agent, or leave this task unassigned".into(),
            )
            .into());
        }
        if dependencies_unresolved {
            return Err(ErrorKind::Validation(
                "wait for the prerequisite tasks to finish before assigning an Agent, or leave this task unassigned"
                    .into(),
            )
            .into());
        }
        Ok(())
    }

    /// Unassigned tasks land in `backlog` unless missing declared inputs,
    /// human approval, or an unfinished parent/prerequisite requires an initial block.
    pub(crate) fn initial_unassigned_state(
        missing_inputs: &[String],
        requires_approval: bool,
        parent_status: Option<&str>,
        prerequisites: &[Uuid],
    ) -> TaskCreationInitialState {
        let dependency_blocked = BlockedTaskPolicy::needs_dependency_block(parent_status);
        let (mut initial_blocked_reason, mut initial_blocked_metadata) =
            BlockedTaskPolicy::initial_state(missing_inputs, requires_approval, dependency_blocked);
        if initial_blocked_reason.is_none() && !prerequisites.is_empty() {
            initial_blocked_reason = Some("waiting_dependency");
            initial_blocked_metadata = Some(json!({ "dependency_ids": prerequisites }));
        }
        let initial_status = if initial_blocked_reason.is_some() { "blocked" } else { "backlog" };

        TaskCreationInitialState { initial_status, initial_blocked_reason, initial_blocked_metadata }
    }
}

pub(crate) struct OrchestrationTransactionPolicy;

impl OrchestrationTransactionPolicy {
    pub(crate) fn begin_failed(operation: &'static str, err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("begin {operation} tx: {err}"))
    }

    pub(crate) fn commit_failed(operation: &'static str, err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("commit {operation} tx: {err}"))
    }

    pub(crate) fn missing_last_assignment_id(task_id: Uuid) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("task {task_id} missing last_assignment_id"))
    }

    pub(crate) fn insert_assignment_outbox_failed(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Internal(anyhow::anyhow!("insert assignment outbox: {err}"))
    }
}

pub(crate) struct OrchestrationRepositoryPolicy;

impl OrchestrationRepositoryPolicy {
    pub(crate) fn task_not_found(id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("orchestration task {id}")).into()
    }

    pub(crate) fn quota_block_conflict() -> AppError {
        ErrorKind::Conflict("task changed while recording the quota block".into()).into()
    }

    pub(crate) fn approval_conflict() -> AppError {
        ErrorKind::Conflict("task changed while approving; refresh and try again".into()).into()
    }

    pub(crate) fn patch_conflict() -> AppError {
        ErrorKind::Conflict("task changed while applying the update; refresh and try again".into()).into()
    }

    pub(crate) fn result_conflict() -> AppError {
        ErrorKind::Conflict("task changed while recording its result".into()).into()
    }

    pub(crate) fn cancel_conflict() -> AppError {
        ErrorKind::Conflict(
            "task changed or execution was already assigned; wait for the result or lease recovery".into(),
        )
        .into()
    }

    pub(crate) fn retry_conflict() -> AppError {
        ErrorKind::Conflict("task changed while retrying; refresh and try again".into()).into()
    }

    pub(crate) fn participant_not_found(agent_id: AgentId) -> AppError {
        ErrorKind::NotFound(format!("participant for agent {agent_id}")).into()
    }

    pub(crate) fn task_run_not_found(run_id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("task_run {run_id}")).into()
    }

    pub(crate) fn task_run_agent_not_found(agent_id: AgentId) -> AppError {
        ErrorKind::NotFound(format!("agent {} for task_run", agent_id.as_uuid())).into()
    }

    pub(crate) fn missing_assigned_agent_for_task_run(task_id: Uuid) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("task {task_id} missing assigned agent for task_run")).into()
    }

    pub(crate) fn invalid_terminal_task_run_status(status: &str) -> AppError {
        ErrorKind::Validation(format!("invalid terminal task_run status: {status}")).into()
    }

    pub(crate) fn invalid_context_item_kind(item_kind: &str) -> AppError {
        ErrorKind::Validation(format!("invalid context item kind: {item_kind}")).into()
    }

    pub(crate) fn invalid_context_ref_kind(ref_kind: &str) -> AppError {
        ErrorKind::Validation(format!("invalid context ref kind: {ref_kind}")).into()
    }

    pub(crate) fn task_comment_not_found(id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("task comment {id}")).into()
    }

    pub(crate) fn invalid_task_comment_kind(kind: &str) -> AppError {
        ErrorKind::Validation(format!("invalid task comment kind: {kind}")).into()
    }

    pub(crate) fn empty_task_comment_body() -> AppError {
        ErrorKind::Validation("task comment body must not be empty".into()).into()
    }

    pub(crate) fn task_marker_list_too_large(max: usize) -> AppError {
        ErrorKind::Validation(format!("task list for human marks exceeds {max}")).into()
    }

    pub(crate) fn invalid_task_review_check_key(key: &str) -> AppError {
        ErrorKind::Validation(format!("invalid task review check key: {key}")).into()
    }

    pub(crate) fn context_injection_capability_profile_serialize(err: impl std::fmt::Display) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("serialize context injection capability profile: {err}")).into()
    }

    pub(crate) fn context_injection_position_overflow(err: impl std::fmt::Display) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("context injection position overflow: {err}")).into()
    }

    pub(crate) fn context_injection_applied_snapshot_serialize(err: impl std::fmt::Display) -> AppError {
        ErrorKind::Internal(anyhow::anyhow!("serialize context injection applied snapshot: {err}")).into()
    }

    pub(crate) fn forbidden() -> AppError {
        ErrorKind::Forbidden("forbidden".into()).into()
    }

    pub(crate) fn ensure_exists_or_forbidden(exists: bool) -> AppResult<()> {
        if exists { Ok(()) } else { Err(Self::forbidden()) }
    }

    pub(crate) fn ensure_workspace(scope: &TenantScope, workspace_id: WorkspaceId) -> AppResult<()> {
        if scope.workspace_id() == Some(workspace_id) { Ok(()) } else { Err(Self::forbidden()) }
    }

    pub(crate) fn required_workspace(scope: &TenantScope) -> AppResult<WorkspaceId> {
        scope.workspace_id().ok_or_else(Self::forbidden)
    }

    pub(crate) fn ensure_run_scope(
        scope: &TenantScope,
        organization_id: OrgId,
        workspace_id: WorkspaceId,
    ) -> AppResult<()> {
        if organization_id != scope.org_id() {
            return Err(Self::forbidden());
        }
        if scope.scoped_read().contains_workspace(workspace_id) { Ok(()) } else { Err(Self::forbidden()) }
    }
}

/// Capability profile recorded on each task run.
pub(crate) struct TaskRunCapabilityProfile;

impl TaskRunCapabilityProfile {
    pub(crate) fn from_assignment(
        participant_capabilities: &[String],
        resolved_context: Option<&ResolvedContext>,
    ) -> serde_json::Value {
        match resolved_context {
            Some(resolved_context) => json!({
                "participant_capabilities": participant_capabilities,
                "runtime_capability": resolved_context.capability,
                "context_resolution": {
                    "envelope_version": resolved_context.envelope_version,
                    "applied": resolved_context.applied,
                    "suggested": resolved_context.suggested,
                    "degradation": resolved_context.degradation,
                }
            }),
            None => json!({
                "capabilities": participant_capabilities,
            }),
        }
    }
}

/// Minimal task snapshot needed to build the worker assignment protocol.
pub(crate) struct TaskAssignmentSnapshot<'a> {
    pub(crate) task_id: Uuid,
    pub(crate) assigned_agent_id: Option<AgentId>,
    pub(crate) last_assignment_id: Option<Uuid>,
    pub(crate) lease_expires_at: Option<DateTime<Utc>>,
    pub(crate) attempt: i32,
    pub(crate) title: &'a str,
    pub(crate) description: Option<&'a str>,
    pub(crate) params: Option<&'a serde_json::Value>,
    pub(crate) priority: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskRunSummary {
    pub id: Uuid,
    #[serde(rename = "agentId")]
    pub agent_id: Uuid,
    pub status: String,
    #[serde(rename = "startedAt")]
    pub started_at: String,
    #[serde(rename = "finishedAt", skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(rename = "runtimeKind", skip_serializing_if = "Option::is_none")]
    pub runtime_kind: Option<String>,
    #[serde(rename = "cliTool", skip_serializing_if = "Option::is_none")]
    pub cli_tool: Option<String>,
    #[serde(rename = "providerName", skip_serializing_if = "Option::is_none")]
    pub provider_name: Option<String>,
    #[serde(rename = "maxContextTokens", skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<TaskRunImageSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRunImageSummary {
    pub source: String,
    pub image_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub version_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust: Option<String>,
}

impl TaskRunImageSummary {
    pub(crate) fn from_capability_profile(capability_profile: &Value) -> Option<Self> {
        serde_json::from_value(capability_profile.get("image")?.clone()).ok()
    }
}

/// A human update (comment / blocker signal) shown in the task Updates tab.
/// First-class record, independent of execution attempts and lifecycle state.
#[derive(Debug, Clone, Serialize)]
pub struct TaskCommentSummary {
    pub id: Uuid,
    #[serde(rename = "taskId")]
    pub task_id: Uuid,
    pub kind: String,
    pub body: String,
    pub author: TaskCommentAuthor,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskCommentAuthor {
    pub id: Uuid,
    pub name: String,
}

/// One ticked human review check on a task (review checklist evidence).
#[derive(Debug, Clone, Serialize)]
pub struct TaskReviewCheckSummary {
    #[serde(rename = "checkKey")]
    pub check_key: String,
    pub done: bool,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// Latest human blocker/unblock signal per task (board surfacing).
#[derive(Debug, Clone, Serialize)]
pub struct HumanMarkerSummary {
    #[serde(rename = "taskId")]
    pub task_id: Uuid,
    pub kind: String,
    pub body: String,
    #[serde(rename = "authorName", skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

/// Kanban-state count snapshot returned by the stats endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct TaskStatsResponse {
    #[serde(rename = "byState")]
    pub by_state: HashMap<String, i64>,
}

/// Participant JSON shape returned to the UI orchestration surfaces.
#[derive(Debug, Clone, Serialize)]
pub struct ParticipantSummary {
    pub id: Uuid,
    #[serde(rename = "agentId")]
    pub agent_id: Uuid,
    pub name: String,
    pub status: String,
    pub capabilities: Vec<String>,
    /// Agent runtime kind (`container`/`cli`/`api`), surfaced so clients can gate
    /// runtime-specific affordances (e.g. only a container CLI can take task
    /// images). `None` when the participant's agent row could not be resolved.
    #[serde(rename = "runtimeKind", skip_serializing_if = "Option::is_none")]
    pub runtime_kind: Option<String>,
    #[serde(rename = "lastHeartbeatAt", skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<String>,
}

/// Structured task params accepted by legacy A2A clients.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CreateTaskParamsInput<'a> {
    pub(crate) task: Option<&'a str>,
    pub(crate) message: Option<&'a str>,
    pub(crate) required_inputs: &'a [String],
    pub(crate) inputs: Option<&'a Value>,
    pub(crate) env: Option<&'a Value>,
    pub(crate) api_keys: Option<&'a Value>,
    /// Opt-in completion contract (#793/#875). Stored verbatim as
    /// `params.expectedResult` so `core::ExpectedResult::from_params` can parse it
    /// at completion time. Kept as a raw `Value` on purpose: over-typing here would
    /// silently drop sub-keys a newer producer adds.
    pub(crate) expected_result: Option<&'a Value>,
    /// Attachment UUIDs of instruction images, stored as `params.imageAttachmentIds`
    /// and materialized into the agent workspace at dispatch.
    pub(crate) image_attachment_ids: &'a [String],
    /// Typed at the HTTP boundary, then stored under the existing Rust/runtime
    /// `dependency_ids` contract.
    pub(crate) dependency_ids: &'a [Uuid],
}

/// Read `params.imageAttachmentIds` back as a list of id strings (empty if
/// absent or malformed).
pub(crate) fn task_image_attachment_ids(params: Option<&Value>) -> Vec<String> {
    params
        .and_then(|p| p.get("imageAttachmentIds"))
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default()
}

pub(crate) fn create_task_request_parts(
    title: Option<&str>,
    description: Option<&str>,
    params: Option<CreateTaskParamsInput<'_>>,
) -> (String, Option<String>, Option<Value>) {
    let title = title.map(str::to_owned).or_else(|| params.and_then(|p| p.task.map(str::to_owned))).unwrap_or_default();
    let description = description.map(str::to_owned).or_else(|| params.and_then(|p| p.message.map(str::to_owned)));
    let params_value = params.map(|p| {
        let mut out = serde_json::Map::new();
        out.insert("task".into(), Value::String(p.task.unwrap_or_default().to_owned()));
        out.insert("message".into(), Value::String(p.message.unwrap_or_default().to_owned()));
        if !p.required_inputs.is_empty() {
            out.insert("requiredInputs".into(), json!(p.required_inputs));
        }
        if let Some(inputs) = p.inputs {
            out.insert("inputs".into(), inputs.clone());
        }
        if let Some(env) = p.env {
            out.insert("env".into(), env.clone());
        }
        if let Some(api_keys) = p.api_keys {
            out.insert("apiKeys".into(), api_keys.clone());
        }
        // #793/#875: persist the opt-in completion contract verbatim so the NATS
        // result consumer can read it back. Without this the verifier never fires
        // for API-created tasks because the contract is dropped before the row is
        // written.
        if let Some(expected_result) = p.expected_result {
            out.insert("expectedResult".into(), expected_result.clone());
        }
        if !p.image_attachment_ids.is_empty() {
            out.insert("imageAttachmentIds".into(), json!(p.image_attachment_ids));
        }
        if !p.dependency_ids.is_empty() {
            out.insert("dependency_ids".into(), json!(p.dependency_ids));
        }
        Value::Object(out)
    });
    (title, description, params_value)
}

pub(crate) fn orchestration_task_response(task: &TaskSummary) -> Value {
    json!({ "ok": true, "task": task })
}

pub(crate) fn orchestration_tasks_response(tasks: &[TaskSummary]) -> Value {
    json!({ "ok": true, "tasks": tasks })
}

pub(crate) fn orchestration_stats_response(stats: &TaskStatsResponse) -> Value {
    json!({ "ok": true, "stats": stats })
}

pub(crate) fn orchestration_task_context_response<T: Serialize>(context: &T) -> Value {
    json!({ "ok": true, "data": context })
}

pub(crate) fn orchestration_task_runs_response(runs: &[TaskRunSummary]) -> Value {
    json!({ "ok": true, "runs": runs })
}

pub(crate) fn orchestration_task_comments_response(comments: &[TaskCommentSummary]) -> Value {
    json!({ "ok": true, "comments": comments })
}

pub(crate) fn orchestration_task_comment_response(comment: &TaskCommentSummary) -> Value {
    json!({ "ok": true, "comment": comment })
}

pub(crate) fn orchestration_human_marks_response(marks: &[HumanMarkerSummary]) -> Value {
    json!({ "ok": true, "marks": marks })
}

pub(crate) fn orchestration_task_export_response(format: &str, content: &str, count: usize) -> Value {
    json!({ "ok": true, "format": format, "content": content, "count": count })
}

/// Audit payload for retiring stale tasks in a group.
pub(crate) fn retired_stale_audit_payload(count: i64) -> Value {
    json!({ "count": count })
}

/// Response for retiring stale tasks in a group.
pub(crate) fn retired_stale_response(count: i64, task_ids: Vec<Uuid>) -> Value {
    json!({ "ok": true, "count": count, "taskIds": task_ids })
}

/// Audit payload for a CSV task-history export.
pub(crate) fn task_history_export_audit_payload(rows: usize) -> Value {
    json!({ "format": "csv", "rows": rows })
}

pub(crate) fn orchestration_task_review_checks_response(checks: &[TaskReviewCheckSummary]) -> Value {
    json!({ "ok": true, "checks": checks })
}

pub(crate) fn orchestration_task_review_check_response(check: &TaskReviewCheckSummary) -> Value {
    json!({ "ok": true, "check": check })
}

/// Required-acceptance-gate read model for a task.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewGateStatus {
    pub(crate) required_keys: Vec<String>,
    pub(crate) satisfied: bool,
    pub(crate) missing: Vec<String>,
}

pub(crate) fn orchestration_task_review_gates_response(status: &ReviewGateStatus) -> Value {
    json!({ "ok": true, "gates": status })
}

/// Escaped CSV cell: wrap in quotes when the value contains a delimiter,
/// quote, or line break; doubles embedded quotes per RFC 4180.
fn csv_cell(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    let neutralized;
    let value = if matches!(value.as_bytes().first(), Some(b'=' | b'+' | b'-' | b'@' | b'\t' | b'\r')) {
        neutralized = format!("'{value}");
        &neutralized
    } else {
        value
    };
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// One compliance export of the team's task history, rendered as CSV.
/// Pure and testable: escaping rules live here, not in the route.
pub(crate) fn task_history_csv(rows: &[TaskHistoryExportRowProjection]) -> String {
    let mut out = String::from(
        "task_id,title,status,priority,progress_percent,creator,assigned_agent,runs_count,created_at,completed_at,updated_at,blocked_reason,requires_approval\n",
    );
    for row in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            csv_cell(row.id.to_string()),
            csv_cell(row.title.as_str()),
            csv_cell(row.status.as_str()),
            csv_cell(row.priority.as_str()),
            csv_cell(row.progress.to_string()),
            csv_cell(row.creator_name.as_deref().unwrap_or_default()),
            csv_cell(row.assigned_agent_name.as_deref().unwrap_or_default()),
            csv_cell(row.runs_count.to_string()),
            csv_cell(row.created_at.to_rfc3339()),
            csv_cell(row.completed_at.map(|t| t.to_rfc3339()).unwrap_or_default()),
            csv_cell(row.updated_at.to_rfc3339()),
            csv_cell(row.blocked_reason.as_deref().unwrap_or_default()),
            csv_cell(row.requires_approval.to_string()),
        ));
    }
    out
}

/// Board-agnostic CSV projection used by the exporter (kept in the domain so
/// the pure CSV function is testable without repository rows).
#[derive(Debug, Clone)]
pub(crate) struct TaskHistoryExportRowProjection {
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

pub(crate) fn orchestration_participant_response(participant: &ParticipantSummary) -> Value {
    json!({ "ok": true, "participant": participant })
}

pub(crate) fn orchestration_participants_response(participants: &[ParticipantSummary]) -> Value {
    json!({ "ok": true, "participants": participants })
}

pub(crate) fn orchestration_delete_response() -> Value {
    json!({ "ok": true })
}

pub(crate) fn task_update_broadcast_payload(action: &str, task: &TaskSummary) -> Value {
    // Built through the shared `ServerMessage` enum (MS-3 PR-E) so the wire
    // contract has a single compiler-checked source of truth. Serializing a
    // fixed-shape payload cannot fail.
    ServerMessage::OrchestrationTaskUpdate {
        payload: OrchestrationTaskUpdatePayload {
            action: action.to_owned(),
            event_id: Uuid::now_v7(),
            task: task.clone(),
        },
    }
    .to_frame_value()
    .expect("orchestration task_update frame serialization is infallible")
}

pub(crate) fn task_update_broadcast_subject(org_id: Uuid) -> String {
    format!("broadcast.{org_id}")
}

// The `OrchestrationTask` / `TaskRun` row -> projection adapters
// (`task_summary`, `task_run_summary`, `task_assignment_snapshot`, and the
// `string_value` helper) live in `services::orchestration`, keeping this domain
// module free of `agentforge_db` (DDD-2). The projection TYPES (`TaskSummary`,
// `TaskRunSummary`, `TaskAssignmentSnapshot`) and the assignment/instruction
// policies stay here as the pure shapes those adapters build.

/// Assignment delivery protocol policy.
pub(crate) struct TaskAssignmentPolicy;

impl TaskAssignmentPolicy {
    pub(crate) fn build(
        snapshot: TaskAssignmentSnapshot<'_>,
        context_envelope: Option<ContextEnvelope>,
        runtime_kind: RuntimeKind,
        hmac_secret: Option<&str>,
    ) -> AppResult<TaskAssignment> {
        let agent_id = snapshot.assigned_agent_id.ok_or_else(|| {
            ErrorKind::Internal(anyhow::anyhow!("task {} missing assigned_agent_id", snapshot.task_id))
        })?;
        let delivery_id = snapshot.last_assignment_id.ok_or_else(|| {
            ErrorKind::Internal(anyhow::anyhow!("task {} missing last_assignment_id", snapshot.task_id))
        })?;
        let lease_expires_at = snapshot.lease_expires_at.ok_or_else(|| {
            ErrorKind::Internal(anyhow::anyhow!("task {} missing lease_expires_at", snapshot.task_id))
        })?;
        let instruction = TaskInstruction::from_params(snapshot.title, snapshot.description, snapshot.params);
        let (task, message) = instruction.into_parts();
        let container_generation_fingerprint = match runtime_kind {
            RuntimeKind::Container => {
                let secret = hmac_secret.filter(|secret| !secret.trim().is_empty()).ok_or_else(|| {
                    ErrorKind::Internal(anyhow::anyhow!(
                        "refusing to dispatch container agent {agent_id} without an HMAC generation secret"
                    ))
                })?;
                Some(container_generation_fingerprint(secret.as_bytes()))
            }
            RuntimeKind::Cli | RuntimeKind::Api => None,
        };

        Ok(TaskAssignment {
            delivery_id: Some(delivery_id),
            attempt: Some(snapshot.attempt),
            lease_expires_at: Some(lease_expires_at),
            task_id: snapshot.task_id,
            agent_id: agent_id.as_uuid(),
            title: snapshot.title.to_string(),
            task,
            message,
            priority: snapshot.priority.to_string(),
            context_envelope,
            runtime_kind: Some(runtime_kind),
            container_generation_fingerprint,
            image_paths: Vec::new(),
            // CN-4: populated once the OTLP tracing layer is installed and this
            // path runs inside a recording span; None until then (and on the
            // wire it is simply omitted, preserving the pre-CN-4 signed shape).
            trace_context: None,
        })
    }
}

/// Task status policy.
pub(crate) struct TaskStatusPolicy;

impl TaskStatusPolicy {
    pub(crate) fn validate_filter(status: &str) -> AppResult<()> {
        if !Self::is_valid(status) {
            return Err(ErrorKind::Validation(format!(
                "invalid task status: {status}. Valid: {}",
                VALID_TASK_STATUSES.join(", ")
            ))
            .into());
        }
        Ok(())
    }

    pub(crate) fn validate_patch_state(state: &str) -> AppResult<()> {
        if !Self::is_patch_state(state) {
            return Err(ErrorKind::Validation(format!("invalid state: {state}")).into());
        }
        Ok(())
    }

    pub(crate) fn is_valid(status: &str) -> bool {
        VALID_TASK_STATUSES.contains(&status)
    }

    pub(crate) fn is_patch_state(state: &str) -> bool {
        KANBAN_DROP_STATUSES.contains(&state) || matches!(state, "canceled" | "failed")
    }

    // The auto-pickup dispatchable-status set (`queued` | `blocked`, `backlog`
    // deliberately excluded) moved to core as
    // `BlockedTaskPolicy::status_can_dispatch` (MS-3 PR-E) so the jobs projector
    // and this domain share one owner.

    pub(crate) fn can_complete_or_fail(status: &str) -> bool {
        status == "working"
    }
}

/// Terminal and approval lifecycle guards for orchestration tasks.
pub(crate) struct TaskLifecyclePolicy;

impl TaskLifecyclePolicy {
    /// HTTP/operator terminal actions cannot revoke a delivery already handed
    /// to the outbox. Only the matching Agent result or lease recovery may end
    /// that execution until a real Agent cancellation protocol exists.
    pub(crate) fn ensure_no_active_delivery(status: &str, last_assignment_id: Option<Uuid>) -> AppResult<()> {
        if status == "working" && last_assignment_id.is_some() {
            return Err(ErrorKind::Validation(
                "active Agent execution must finish through its result or lease recovery".into(),
            )
            .into());
        }
        Ok(())
    }

    /// Completion is normally only valid from `working`. The one exception is a
    /// `waiting_verification` hold (#793/#875): the agent already finished and the
    /// only thing standing between the task and `completed` is the operator
    /// accepting the suspect result. Without this carve-out the "mark it done" the
    /// FE copy advertises is a dead click — the held card stays `blocked`.
    pub(crate) fn ensure_can_complete(status: &str, blocked_reason: Option<&str>) -> AppResult<()> {
        if status == "blocked" && blocked_reason == Some("waiting_verification") {
            return Ok(());
        }
        Self::ensure_working_action(status, "complete")
    }

    pub(crate) fn ensure_can_fail(status: &str) -> AppResult<()> {
        Self::ensure_working_action(status, "fail")
    }

    pub(crate) fn ensure_can_cancel(status: &str) -> AppResult<()> {
        if status == "working" {
            return Err(ErrorKind::Validation(
                "cannot cancel active execution until the Agent cancellation protocol is available".into(),
            )
            .into());
        }
        if matches!(status, "completed" | "failed") {
            return Err(ErrorKind::Validation(format!("cannot cancel {status} tasks")).into());
        }
        Ok(())
    }

    pub(crate) fn ensure_can_retry(
        status: &str,
        blocked_reason: Option<&str>,
        requires_approval: bool,
        retryable: bool,
    ) -> AppResult<()> {
        if status == "blocked" && blocked_reason == Some("waiting_approval") && requires_approval {
            return Err(ErrorKind::Validation("approve or cancel approval-blocked tasks before retry".into()).into());
        }
        if matches!(status, "failed" | "canceled")
            || (status == "blocked" && (retryable || blocked_reason == Some("waiting_verification")))
        {
            return Ok(());
        }
        Err(ErrorKind::Validation(format!(
            "can only retry failed, canceled, or retryable blocked tasks, current status: {status}"
        ))
        .into())
    }

    pub(crate) fn ensure_can_approve(
        status: &str,
        blocked_reason: Option<&str>,
        requires_approval: bool,
    ) -> AppResult<()> {
        if status != "blocked" || blocked_reason != Some("waiting_approval") {
            return Err(ErrorKind::Validation("task is not waiting for approval".into()).into());
        }
        if !requires_approval {
            return Err(ErrorKind::Validation("task approval has already been consumed".into()).into());
        }
        Ok(())
    }

    fn ensure_working_action(status: &str, action: &str) -> AppResult<()> {
        if TaskStatusPolicy::can_complete_or_fail(status) {
            return Ok(());
        }
        Err(ErrorKind::Validation(format!("can only {action} working tasks, current status: {status}")).into())
    }
}

/// Participant display name value object.
pub(crate) struct ParticipantName;

impl ParticipantName {
    pub(crate) fn validate(name: &str) -> AppResult<()> {
        if name.is_empty() || name.len() > 255 {
            return Err(ErrorKind::Validation("name must be between 1 and 255 characters".into()).into());
        }
        Ok(())
    }
}

/// Participant status policy.
pub(crate) struct ParticipantStatusPolicy;

impl ParticipantStatusPolicy {
    pub(crate) fn validate_filter(status: &str) -> AppResult<()> {
        if !Self::is_valid(status) {
            return Err(ErrorKind::Validation(format!(
                "invalid participant status: {status}. Valid: {}",
                VALID_PARTICIPANT_STATUSES.join(", ")
            ))
            .into());
        }
        Ok(())
    }

    pub(crate) fn is_valid(status: &str) -> bool {
        VALID_PARTICIPANT_STATUSES.contains(&status)
    }

    pub(crate) fn should_sweep_after_heartbeat(status: &str) -> bool {
        status == "available"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DispatchSweepDecision {
    ClaimedTask,
    Stop,
}

/// Follow-up policy for the best-effort auto-dispatch sweep.
pub(crate) struct DispatchSweepPolicy;

impl DispatchSweepPolicy {
    pub(crate) fn after_dispatch_attempt(status: &str) -> DispatchSweepDecision {
        if status == "working" { DispatchSweepDecision::ClaimedTask } else { DispatchSweepDecision::Stop }
    }
}

/// User intent that requires an available participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParticipantAvailabilityAction {
    AssignTask,
    PreviewContext,
}

impl ParticipantAvailabilityAction {
    fn guidance(self) -> &'static str {
        match self {
            Self::AssignTask => "pick an available agent or leave unassigned",
            Self::PreviewContext => "preview an available agent",
        }
    }
}

/// Participant availability policy shared by task assignment and context preview.
pub(crate) struct ParticipantAvailabilityPolicy;

impl ParticipantAvailabilityPolicy {
    pub(crate) fn ensure_available(
        participant_name: &str,
        participant_status: &str,
        action: ParticipantAvailabilityAction,
    ) -> AppResult<()> {
        if participant_status == "available" {
            return Ok(());
        }
        Err(ErrorKind::Validation(format!(
            "participant {participant_name} is {participant_status} — {}",
            action.guidance()
        ))
        .into())
    }
}

/// PATCH semantics for kanban task updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskPatchAction {
    AssignToParticipant(AgentId),
    Dispatch,
    Complete,
    Fail,
    Cancel,
    Unassign,
    Patch,
}

pub(crate) struct TaskPatchPolicy;

impl TaskPatchPolicy {
    pub(crate) fn touches_assignment(assigned_to: &Option<Option<AgentId>>) -> bool {
        assigned_to.is_some()
    }

    pub(crate) fn is_business_transition(state: Option<&str>, assigned_to: &Option<Option<AgentId>>) -> bool {
        matches!(state, Some("working" | "completed" | "failed" | "canceled")) || matches!(assigned_to, Some(Some(_)))
    }

    pub(crate) fn requires_current_task(state: Option<&str>, assigned_to: &Option<Option<AgentId>>) -> bool {
        state.is_some() || Self::touches_assignment(assigned_to)
    }

    pub(crate) fn validate_progress(progress: Option<i16>) -> AppResult<()> {
        if let Some(progress) = progress
            && !(0..=100).contains(&progress)
        {
            return Err(ErrorKind::Validation("progress must be 0-100".into()).into());
        }
        Ok(())
    }

    /// Direct operator-resolution targets allowed out of a verification hold.
    /// Re-running uses Retry so assignment/run state is reset atomically.
    const VERIFICATION_HOLD_RESOLUTIONS: &'static [&'static str] = &["completed", "working"];

    pub(crate) fn ensure_current_allows_transition(
        current_status: &str,
        current_blocked_reason: Option<&str>,
        transition_state: Option<&str>,
    ) -> AppResult<()> {
        if matches!(current_status, "completed" | "failed" | "canceled")
            && transition_state.is_some_and(|next| next != current_status)
        {
            return Err(ErrorKind::Validation(format!(
                "{current_status} tasks are terminal and cannot change lanes; use Retry task for failed or canceled work"
            ))
            .into());
        }
        if transition_state == Some("blocked") {
            return Err(ErrorKind::Validation("blocked state requires a reason-specific workflow".into()).into());
        }
        if current_status == "working"
            && transition_state.is_some_and(|next| !matches!(next, "working" | "completed" | "failed" | "canceled"))
        {
            return Err(ErrorKind::Validation(
                "working tasks can only complete, fail, or cancel through lifecycle actions".into(),
            )
            .into());
        }
        if current_status != "blocked" || BlockedTaskPolicy::reason_allows_dispatch(current_blocked_reason) {
            return Ok(());
        }
        // Cancel is the universal escape hatch for any held task.
        if matches!(transition_state, Some("canceled")) {
            return Ok(());
        }
        // #793/#875: a `waiting_verification` hold is human-gated, not a dispatch
        // dead-end. Allow accept or explicit dispatch; re-run uses Retry so stale
        // assignment/run state is cleared. Auto-dispatch is unaffected: it keys off
        // `reason_allows_dispatch`, which still excludes `waiting_verification`, so
        // a held task stays human-gated and is never auto-claimed.
        if current_blocked_reason == Some("waiting_verification")
            && transition_state.is_some_and(|state| Self::VERIFICATION_HOLD_RESOLUTIONS.contains(&state))
        {
            return Ok(());
        }
        Err(ErrorKind::Validation(format!(
            "task is blocked on {}; use its unblock path before dispatching",
            current_blocked_reason.unwrap_or("unknown")
        ))
        .into())
    }

    pub(crate) fn plan(
        state: Option<&str>,
        priority: Option<&str>,
        progress: Option<i16>,
        assigned_to: &Option<Option<AgentId>>,
    ) -> AppResult<TaskPatchAction> {
        if Self::is_business_transition(state, assigned_to) && (priority.is_some() || progress.is_some()) {
            return Err(ErrorKind::Validation(
                "state/assignment transitions must not be combined with priority/progress edits".into(),
            )
            .into());
        }
        if Self::touches_assignment(assigned_to) && !matches!(state, None | Some("working")) {
            return Err(ErrorKind::Validation(
                "assignedTo changes must use dispatch semantics; combine only with state=working or omit state".into(),
            )
            .into());
        }

        match (state, *assigned_to) {
            (Some("working"), Some(Some(agent_id))) | (None, Some(Some(agent_id))) => {
                Ok(TaskPatchAction::AssignToParticipant(agent_id))
            }
            (Some("working"), Some(None)) => {
                Err(ErrorKind::Validation("cannot unassign while dispatching to working".into()).into())
            }
            (Some("working"), None) => Ok(TaskPatchAction::Dispatch),
            (Some("completed"), None) => Ok(TaskPatchAction::Complete),
            (Some("failed"), None) => Ok(TaskPatchAction::Fail),
            (Some("canceled"), None) => Ok(TaskPatchAction::Cancel),
            (None, Some(None)) => Ok(TaskPatchAction::Unassign),
            _ => Ok(TaskPatchAction::Patch),
        }
    }

    pub(crate) fn ensure_can_unassign(current_status: &str) -> AppResult<()> {
        if current_status == "working" {
            return Err(ErrorKind::Validation(
                "cannot unassign a working task; cancel, complete, or fail it first".into(),
            )
            .into());
        }
        Ok(())
    }

    pub(crate) fn manual_complete_result() -> serde_json::Value {
        json!({ "manual": true, "source": "kanban_patch" })
    }

    pub(crate) fn manual_failure_error() -> serde_json::Value {
        json!({
            "message": "manual failure via task patch",
            "source": "kanban_patch"
        })
    }

    pub(crate) fn should_auto_dispatch_after_patch(
        next_state: Option<&str>,
        assigned_agent_id: Option<AgentId>,
    ) -> bool {
        matches!(next_state, Some("queued") | Some("backlog")) && assigned_agent_id.is_none()
    }
}

/// Assignment field semantics for task PATCH requests.
pub(crate) struct TaskAssignmentPatchPolicy;

impl TaskAssignmentPatchPolicy {
    pub(crate) fn parse(raw: Option<&str>) -> AppResult<Option<Option<AgentId>>> {
        match raw {
            None => Ok(None),
            Some("") => Ok(Some(None)),
            Some(value) => Uuid::parse_str(value).map(|id| Some(Some(AgentId::from(id)))).map_err(|_| {
                ErrorKind::Validation(format!("assignedTo must be a UUID or empty string, got: {value}")).into()
            }),
        }
    }
}

/// Provider quota/rate-limit failure classifier.
pub(crate) struct QuotaBlockPolicy;

impl QuotaBlockPolicy {
    pub(crate) fn metadata(error: &serde_json::Value) -> Option<serde_json::Value> {
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
                m.contains("quota")
                    || m.contains("rate limit")
                    || m.contains("rate_limit")
                    || m.contains("billing limit")
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

#[cfg(test)]
mod tests {
    use agentforge_core::ws_protocol::TaskParams;

    use super::*;

    #[test]
    fn dependency_policy_parses_and_resolves() {
        let params = serde_json::json!({ "dependency_ids": ["00000000-0000-0000-0000-000000000001", "not-a-uuid", "00000000-0000-0000-0000-000000000002"] });
        let deps = TaskDependencyPolicy::from_params(Some(&params));
        assert_eq!(deps.len(), 2, "invalid ids are skipped");
        let done = vec![(deps[0], "completed".to_string()), (deps[1], "completed".to_string())];
        assert!(!TaskDependencyPolicy::unresolved(&deps, &done));
        let partial = vec![(deps[0], "completed".to_string()), (deps[1], "failed".to_string())];
        assert!(TaskDependencyPolicy::unresolved(&deps, &partial), "failed prereqs stay unresolved");
        assert!(TaskDependencyPolicy::from_params(None).is_empty());

        let too_many = json!({ "dependency_ids": vec![Uuid::nil().to_string(); TaskDependencyPolicy::MAX + 1] });
        assert!(TaskDependencyPolicy::ensure_within_limit(Some(&too_many)).is_err());
    }

    #[test]
    fn review_gates_parse_and_report_incomplete_errors() {
        let keys = ReviewGatePolicy::parse(Some(" no_secrets , result_matches_brief ,,"));
        assert_eq!(keys, vec!["no_secrets".to_string(), "result_matches_brief".to_string()]);
        assert!(ReviewGatePolicy::parse(None).is_empty());
        let error = ReviewGatePolicy::incomplete_error(&keys).to_string();
        assert!(error.contains("no_secrets"), "error was: {error}");
        assert!(error.contains("review checklist"), "error was: {error}");
    }

    #[test]
    fn task_history_csv_escapes_delimiters_quotes_and_newlines() {
        let rows = vec![TaskHistoryExportRowProjection {
            id: Uuid::new_v4(),
            title: "Deploy, now".into(),
            status: "completed".into(),
            priority: "normal".into(),
            progress: 100,
            creator_name: None,
            assigned_agent_name: Some("Build, \"G\"".into()),
            runs_count: 2,
            created_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).expect("timestamp"),
            completed_at: None,
            updated_at: DateTime::<Utc>::from_timestamp(1_700_000_100, 0).expect("timestamp"),
            blocked_reason: Some("waiting_agent".to_string()),
            requires_approval: true,
        }];
        let csv = task_history_csv(&rows);
        assert!(csv.starts_with("task_id,title,status,priority,progress_percent,creator,assigned_agent,runs_count,created_at,completed_at,updated_at,blocked_reason,requires_approval\n"));
        assert!(csv.contains("\"Deploy, now\""), "comma must be quoted");
        assert!(csv.contains("\"Build, \"\"G\"\"\""), "quotes must double up");
        assert!(csv.contains(",true\n"));
        assert!(!csv.contains("\n\n"), "no blank lines");
    }

    #[test]
    fn csv_cell_neutralizes_spreadsheet_formulas() {
        for value in ["=1+1", "+cmd", "-2+3", "@SUM(A1:A2)", "\t=1"] {
            assert!(csv_cell(value).starts_with('\''), "dangerous cell was not neutralized: {value:?}");
        }
        assert_eq!(csv_cell("\r=1"), "\"'\r=1\"");
        assert_eq!(csv_cell("ordinary"), "ordinary");
    }

    #[test]
    fn retire_policy_validates_and_defaults() {
        let (days, batch) = TaskRetirePolicy::validate(None, None).expect("defaults");
        assert_eq!(days, TaskRetirePolicy::DEFAULT_DAYS);
        assert_eq!(batch, TaskRetirePolicy::DEFAULT_BATCH);
        let (days, batch) = TaskRetirePolicy::validate(Some(14), Some(25)).expect("explicit");
        assert_eq!((days, batch), (14, 25));
        assert!(TaskRetirePolicy::validate(Some(0), None).is_err());
        assert!(TaskRetirePolicy::validate(Some(91), None).is_err());
        assert!(TaskRetirePolicy::validate(None, Some(0)).is_err());
        assert!(TaskRetirePolicy::validate(None, Some(501)).is_err());
    }

    #[test]
    fn wait_estimate_uses_org_median_or_declares_guess_without_history() {
        let est = TaskWaitEstimatePolicy::estimate(2, Some(90));
        assert_eq!(est.position, 2);
        assert_eq!(est.typical_seconds, 90);
        assert_eq!(est.estimated_seconds, 180, "position x median duration");

        let no_history = TaskWaitEstimatePolicy::estimate(1, None);
        assert_eq!(no_history.typical_seconds, 0, "0 signals no recent timings");
        assert_eq!(no_history.estimated_seconds, TaskWaitEstimatePolicy::DEFAULT_TYPICAL_SECONDS);
        assert_eq!(TaskWaitEstimatePolicy::estimate(4, None).estimated_seconds, 1200);
    }

    fn validation_message(result: AppResult<()>) -> String {
        match result.unwrap_err().kind {
            ErrorKind::Validation(message) => message,
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    fn sample_task_summary() -> TaskSummary {
        TaskSummary {
            id: Uuid::from_u128(1),
            group_id: None,
            state: "queued".to_owned(),
            method: "tasks/send".to_owned(),
            params: TaskParams { task: "Build feature".to_owned(), message: "with context".to_owned() },
            priority: "normal".to_owned(),
            progress: 0,
            created_by: Uuid::from_u128(2),
            assigned_to: None,
            assigned_agent_name: None,
            error: None,
            result: None,
            blocked_reason: None,
            blocked_hint: None,
            blocked_metadata: None,
            created_at: "2026-04-20T12:00:00Z".to_owned(),
            updated_at: "2026-04-20T12:00:00Z".to_owned(),
            row_version: None,
            completed_at: None,
            self_fix: false,
            pr_number: None,
            pr_url: None,
            pr_head_sha: None,
            review_status: None,
            context_counts: TaskContextCounts::default(),
            attempt: 1,
            lease_expires_at: None,
            wait_estimate: None,
        }
    }

    #[test]
    fn list_page_clamps_limit_and_offset() {
        assert_eq!(TaskListPage::new(0, -1).limit(), 1);
        assert_eq!(TaskListPage::new(101, 50).limit(), 100);
        assert_eq!(TaskListPage::new(20, -1).offset(), 0);
        assert_eq!(TaskListPage::new(20, 50).offset(), 50);
    }

    #[test]
    fn create_task_request_parts_prefers_top_level_fields() {
        let params = CreateTaskParamsInput {
            task: Some("Legacy task"),
            message: Some("Legacy message"),
            required_inputs: &[],
            inputs: None,
            env: None,
            api_keys: None,
            expected_result: None,
            image_attachment_ids: &[],
            dependency_ids: &[],
        };

        let (title, description, params_value) =
            create_task_request_parts(Some("Top title"), Some("Top description"), Some(params));

        assert_eq!(title, "Top title");
        assert_eq!(description.as_deref(), Some("Top description"));
        assert_eq!(params_value.unwrap()["task"], "Legacy task");
    }

    #[test]
    fn create_task_request_parts_preserves_legacy_a2a_params() {
        let required_inputs = vec!["ANTHROPIC_API_KEY".to_owned()];
        let inputs = json!({ "ticket": "WIS-1" });
        let env = json!({ "REGION": "eu" });
        let api_keys = json!({ "anthropic": "ref" });
        let params = CreateTaskParamsInput {
            task: Some("Deploy"),
            message: Some("prod"),
            required_inputs: &required_inputs,
            inputs: Some(&inputs),
            env: Some(&env),
            api_keys: Some(&api_keys),
            expected_result: None,
            image_attachment_ids: &[],
            dependency_ids: &[],
        };

        let (title, description, params_value) = create_task_request_parts(None, None, Some(params));
        let params_value = params_value.expect("params");

        assert_eq!(title, "Deploy");
        assert_eq!(description.as_deref(), Some("prod"));
        assert_eq!(params_value["requiredInputs"][0], "ANTHROPIC_API_KEY");
        assert_eq!(params_value["inputs"]["ticket"], "WIS-1");
        assert_eq!(params_value["env"]["REGION"], "eu");
        assert_eq!(params_value["apiKeys"]["anthropic"], "ref");
    }

    #[test]
    fn create_task_request_parts_persists_expected_result_verbatim() {
        // #793/#875 regression guard: a client POSTing `params.expectedResult` must
        // have it land in the stored params JSONB verbatim, otherwise the NATS
        // completion verifier (which reads `params.expectedResult`) never fires for
        // API-created tasks. This is the test that would have caught the drop bug.
        let expected_result = json!({ "contains": "tests passed" });
        let params = CreateTaskParamsInput {
            task: Some("Run suite"),
            message: Some("verify"),
            required_inputs: &[],
            inputs: None,
            env: None,
            api_keys: None,
            expected_result: Some(&expected_result),
            image_attachment_ids: &[],
            dependency_ids: &[],
        };

        let (_title, _description, params_value) = create_task_request_parts(None, None, Some(params));
        let params_value = params_value.expect("params");

        assert!(params_value.get("expectedResult").is_some(), "expectedResult must survive into stored params");
        assert_eq!(params_value["expectedResult"], expected_result, "contract must be stored verbatim");
        // And the persisted shape must be exactly what core::ExpectedResult parses.
        let parsed = agentforge_core::completion_verifier::ExpectedResult::from_params(Some(&params_value))
            .expect("stored contract must parse back into ExpectedResult");
        assert_eq!(parsed.contains.as_deref(), Some("tests passed"));
    }

    #[test]
    fn create_task_request_parts_omits_expected_result_when_absent() {
        let params = CreateTaskParamsInput {
            task: Some("Plain"),
            message: Some("no contract"),
            required_inputs: &[],
            inputs: None,
            env: None,
            api_keys: None,
            expected_result: None,
            image_attachment_ids: &[],
            dependency_ids: &[],
        };

        let (_title, _description, params_value) = create_task_request_parts(None, None, Some(params));
        let params_value = params_value.expect("params");
        assert!(params_value.get("expectedResult").is_none(), "no contract → no expectedResult key");
    }

    #[test]
    fn orchestration_response_helpers_preserve_legacy_envelopes() {
        let task = sample_task_summary();
        let tasks = vec![task.clone()];
        let stats = TaskStatsResponse { by_state: std::collections::HashMap::from([("queued".to_owned(), 1)]) };
        let participant = ParticipantSummary {
            id: Uuid::from_u128(3),
            agent_id: Uuid::from_u128(4),
            name: "worker-1".to_owned(),
            status: "available".to_owned(),
            capabilities: vec!["rust".to_owned()],
            runtime_kind: Some("container".to_owned()),
            last_heartbeat_at: Some("2026-04-20T12:00:00Z".to_owned()),
        };
        let participants = vec![participant.clone()];

        assert_eq!(orchestration_task_response(&task)["task"]["id"], task.id.to_string());
        assert_eq!(orchestration_tasks_response(&tasks)["tasks"][0]["id"], task.id.to_string());
        assert_eq!(orchestration_stats_response(&stats)["stats"]["byState"]["queued"], 1);
        assert_eq!(orchestration_task_context_response(&json!({ "items": [] }))["data"]["items"], json!([]));
        assert_eq!(
            orchestration_participant_response(&participant)["participant"]["agentId"],
            participant.agent_id.to_string()
        );
        assert_eq!(orchestration_participants_response(&participants)["participants"][0]["name"], "worker-1");
        assert_eq!(orchestration_delete_response()["ok"], true);
    }

    #[test]
    fn task_update_broadcast_payload_is_owned_by_domain() {
        let task = sample_task_summary();
        let body = task_update_broadcast_payload("task.created", &task);

        assert_eq!(body["type"], "orchestration:task_update");
        assert_eq!(body["payload"]["action"], "task.created");
        assert_eq!(body["payload"]["task"]["id"], task.id.to_string());
        assert!(body["payload"]["eventId"].as_str().is_some());
    }

    #[test]
    fn task_update_broadcast_subject_is_org_scoped() {
        let org_id = Uuid::parse_str("aaaaaaaa-1111-2222-3333-444444444444").unwrap();
        assert_eq!(task_update_broadcast_subject(org_id), "broadcast.aaaaaaaa-1111-2222-3333-444444444444");
    }

    #[test]
    fn task_creation_initial_state_keeps_ready_unassigned_tasks_in_backlog() {
        let state = TaskCreationPolicy::initial_unassigned_state(&[], false, None, &[]);

        assert_eq!(state.initial_status, "backlog");
        assert_eq!(state.initial_blocked_reason, None);
        assert_eq!(state.initial_blocked_metadata, None);
    }

    #[test]
    fn task_creation_initial_state_blocks_missing_inputs_first() {
        let missing_inputs = vec!["api_key".to_string(), "region".to_string()];
        let state =
            TaskCreationPolicy::initial_unassigned_state(&missing_inputs, true, Some("working"), &[Uuid::nil()]);

        assert_eq!(state.initial_status, "blocked");
        assert_eq!(state.initial_blocked_reason, Some("waiting_input"));
        assert_eq!(state.initial_blocked_metadata, Some(json!({ "missing": ["api_key", "region"] })));
    }

    #[test]
    fn task_creation_initial_state_blocks_approval_before_dependency() {
        let state = TaskCreationPolicy::initial_unassigned_state(&[], true, Some("working"), &[Uuid::nil()]);

        assert_eq!(state.initial_status, "blocked");
        assert_eq!(state.initial_blocked_reason, Some("waiting_approval"));
        assert_eq!(state.initial_blocked_metadata, Some(json!({ "approver": "管理员" })));
    }

    #[test]
    fn task_creation_initial_state_blocks_unfinished_parent() {
        let state = TaskCreationPolicy::initial_unassigned_state(&[], false, Some("working"), &[]);

        assert_eq!(state.initial_status, "blocked");
        assert_eq!(state.initial_blocked_reason, Some("waiting_dependency"));
        assert_eq!(state.initial_blocked_metadata, Some(json!({ "pending": 1 })));
    }

    #[test]
    fn task_instruction_prefers_structured_params() {
        let instruction = TaskInstruction::from_params(
            "Fallback title",
            Some("Fallback description"),
            Some(&json!({ "task": "Run the analysis", "message": "Use the deep model" })),
        );

        assert_eq!(instruction.into_parts(), ("Run the analysis".to_string(), "Use the deep model".to_string()));
    }

    #[test]
    fn task_instruction_uses_legacy_fallback_without_params() {
        let instruction = TaskInstruction::from_params("Fallback title", Some("Fallback description"), None);

        assert_eq!(instruction.into_parts(), ("Fallback title".to_string(), "Fallback description".to_string()));
    }

    #[test]
    fn task_instruction_keeps_empty_structured_message_when_params_exist() {
        let instruction =
            TaskInstruction::from_params("Fallback title", Some("Fallback description"), Some(&json!({})));

        assert_eq!(instruction.into_parts(), ("Fallback title".to_string(), String::new()));
    }

    #[test]
    fn task_run_capability_profile_records_plain_participant_capabilities_without_context() {
        let capabilities = vec!["coding".to_string(), "research".to_string()];

        assert_eq!(
            TaskRunCapabilityProfile::from_assignment(&capabilities, None),
            json!({ "capabilities": ["coding", "research"] })
        );
    }

    #[test]
    fn task_run_capability_profile_records_context_resolution_when_available() {
        let capabilities = vec!["coding".to_string()];
        let resolved_context = ResolvedContext {
            applied: Vec::new(),
            suggested: Vec::new(),
            capability: agentforge_core::RuntimeCapability::api_default("openai"),
            degradation: Vec::new(),
            envelope_version: "2026-05-17".to_string(),
        };

        let profile = TaskRunCapabilityProfile::from_assignment(&capabilities, Some(&resolved_context));

        assert_eq!(profile["participant_capabilities"], json!(["coding"]));
        assert_eq!(profile["runtime_capability"]["provider_name"], "openai");
        assert_eq!(profile["context_resolution"]["envelope_version"], "2026-05-17");
    }

    #[test]
    fn task_context_counts_new_sums_memory_and_skill_totals() {
        let counts = TaskContextCounts::new(3, 5);

        assert_eq!(counts.applied_memories, 3);
        assert_eq!(counts.applied_skills, 5);
        assert_eq!(counts.total, 8);
    }

    #[test]
    fn task_assignment_policy_builds_delivery_protocol_from_snapshot() {
        let agent_id = AgentId::from(Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap());
        let task_id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let delivery_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let lease_expires_at = Utc::now();
        let assignment = TaskAssignmentPolicy::build(
            TaskAssignmentSnapshot {
                task_id,
                assigned_agent_id: Some(agent_id),
                last_assignment_id: Some(delivery_id),
                lease_expires_at: Some(lease_expires_at),
                attempt: 3,
                title: "Fallback title",
                description: Some("Fallback description"),
                params: Some(&json!({ "task": "Execute", "message": "Use context" })),
                priority: "high",
            },
            None,
            RuntimeKind::Container,
            Some("container-hmac"),
        )
        .unwrap();

        assert_eq!(assignment.delivery_id, Some(delivery_id));
        assert_eq!(assignment.attempt, Some(3));
        assert_eq!(assignment.lease_expires_at, Some(lease_expires_at));
        assert_eq!(assignment.task_id, task_id);
        assert_eq!(assignment.agent_id, agent_id.as_uuid());
        assert_eq!(assignment.title, "Fallback title");
        assert_eq!(assignment.task, "Execute");
        assert_eq!(assignment.message, "Use context");
        assert_eq!(assignment.priority, "high");
        assert_eq!(assignment.runtime_kind, Some(RuntimeKind::Container));
        let expected_generation = container_generation_fingerprint(b"container-hmac");
        assert_eq!(assignment.container_generation_fingerprint.as_deref(), Some(expected_generation.as_str()));
    }

    #[test]
    fn task_assignment_policy_rejects_incomplete_claim_snapshots() {
        let task_id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let err = TaskAssignmentPolicy::build(
            TaskAssignmentSnapshot {
                task_id,
                assigned_agent_id: None,
                last_assignment_id: Some(Uuid::nil()),
                lease_expires_at: Some(Utc::now()),
                attempt: 1,
                title: "Task",
                description: None,
                params: None,
                priority: "normal",
            },
            None,
            RuntimeKind::Api,
            None,
        )
        .unwrap_err();

        match err.kind {
            ErrorKind::Internal(message) => assert!(message.to_string().contains("missing assigned_agent_id")),
            other => panic!("expected internal error, got {other:?}"),
        }
    }

    #[test]
    fn task_assignment_policy_requires_generation_only_for_containers() {
        let snapshot = || TaskAssignmentSnapshot {
            task_id: Uuid::now_v7(),
            assigned_agent_id: Some(AgentId::from(Uuid::now_v7())),
            last_assignment_id: Some(Uuid::now_v7()),
            lease_expires_at: Some(Utc::now()),
            attempt: 1,
            title: "Task",
            description: None,
            params: None,
            priority: "normal",
        };

        let err = TaskAssignmentPolicy::build(snapshot(), None, RuntimeKind::Container, None).unwrap_err();
        assert!(err.to_string().contains("HMAC generation secret"));

        let host_cli = TaskAssignmentPolicy::build(snapshot(), None, RuntimeKind::Cli, None).unwrap();
        assert_eq!(host_cli.runtime_kind, Some(RuntimeKind::Cli));
        assert!(host_cli.container_generation_fingerprint.is_none());
    }

    #[test]
    fn required_inputs_accept_nested_non_empty_values() {
        let params = json!({
            "requiredInputs": ["api_key", "model", "region"],
            "inputs": { "api_key": "sk-test" },
            "env": { "model": "claude" },
            "region": "us"
        });

        assert!(BlockedTaskPolicy::missing_required_inputs(Some(&params)).is_empty());
    }

    #[test]
    fn required_inputs_reject_missing_empty_and_null_values() {
        let params = json!({
            "required_inputs": ["api_key", "model", "region"],
            "inputs": { "api_key": "   " },
            "env": { "model": null }
        });

        assert_eq!(
            BlockedTaskPolicy::missing_required_inputs(Some(&params)),
            vec!["api_key".to_string(), "model".to_string(), "region".to_string()]
        );
    }

    #[test]
    fn blocked_task_policy_allows_dispatch_for_queued_or_waiting_agent_only() {
        assert!(BlockedTaskPolicy::ensure_can_enter_dispatch("queued", None).is_ok());
        assert!(BlockedTaskPolicy::ensure_can_enter_dispatch("blocked", Some("waiting_agent")).is_ok());
        assert!(BlockedTaskPolicy::ensure_can_enter_dispatch("blocked", Some("waiting_input")).is_err());
        assert!(BlockedTaskPolicy::ensure_can_enter_dispatch("completed", None).is_err());
        // #793/#875: the AUTO-sweep guard must NEVER admit a verification hold, so
        // a held task is never auto-claimed.
        assert!(BlockedTaskPolicy::ensure_can_enter_dispatch("blocked", Some("waiting_verification")).is_err());
        assert!(!BlockedTaskPolicy::can_enter_dispatch("blocked", Some("waiting_verification")));
    }

    #[test]
    fn operator_dispatch_allows_verification_hold_rerun_but_not_other_holds() {
        // #793/#875: an operator re-running a verification hold is allowed on the
        // EXPLICIT dispatch path...
        assert!(BlockedTaskPolicy::ensure_operator_can_dispatch("blocked", Some("waiting_verification")).is_ok());
        // ...while normal dispatchable states still pass...
        assert!(BlockedTaskPolicy::ensure_operator_can_dispatch("queued", None).is_ok());
        assert!(BlockedTaskPolicy::ensure_operator_can_dispatch("blocked", Some("waiting_agent")).is_ok());
        // ...and other holds still reject (no regression).
        assert!(BlockedTaskPolicy::ensure_operator_can_dispatch("blocked", Some("waiting_input")).is_err());
        assert!(BlockedTaskPolicy::ensure_operator_can_dispatch("blocked", Some("waiting_approval")).is_err());
    }

    #[test]
    fn blocked_task_policy_dispatch_error_includes_current_state() {
        let error =
            match &BlockedTaskPolicy::ensure_can_enter_dispatch("blocked", Some("waiting_input")).unwrap_err().kind {
                ErrorKind::Validation(message) => message.clone(),
                other => panic!("expected validation error, got {other:?}"),
            };

        assert!(error.contains("current status: blocked"));
        assert!(error.contains("blocked reason: waiting_input"));
    }

    #[test]
    fn blocked_task_policy_builds_waiting_agent_block() {
        let metadata = BlockedTaskPolicy::waiting_agent_metadata(0, 2, 1);

        assert_eq!(BlockedTaskPolicy::waiting_agent_reason(), "waiting_agent");
        assert_eq!(metadata["available"], 0);
        assert_eq!(metadata["busy"], 2);
        assert_eq!(metadata["offline"], 1);

        let error = BlockedTaskPolicy::no_available_participants_error();
        assert!(matches!(error, ErrorKind::Validation(message) if message == "no available participants for dispatch"));
    }

    #[test]
    fn blocked_task_policy_releases_approval_to_queue_or_dependency_block() {
        assert_eq!(BlockedTaskPolicy::approval_release_state(None), ("queued", None, None));
        assert_eq!(BlockedTaskPolicy::approval_release_state(Some("completed")), ("queued", None, None));

        let blocked = BlockedTaskPolicy::approval_release_state(Some("working"));
        assert_eq!(blocked.0, "blocked");
        assert_eq!(blocked.1, Some("waiting_dependency"));
        assert_eq!(blocked.2.as_ref().and_then(|m| m.get("pending")).and_then(|v| v.as_i64()), Some(1));
    }

    #[test]
    fn quota_block_policy_detects_nested_rate_limit_errors() {
        let metadata = QuotaBlockPolicy::metadata(&json!({
            "error": {
                "code": "rate_limit_exceeded",
                "status": 429,
                "used": 99,
                "limit": 100,
                "provider": "openai"
            }
        }))
        .expect("quota metadata");

        assert_eq!(metadata["code"], "rate_limit_exceeded");
        assert_eq!(metadata["status"], 429);
        assert_eq!(metadata["used"], 99);
        assert_eq!(metadata["limit"], 100);
        assert_eq!(metadata["provider"], "openai");
    }

    #[test]
    fn quota_block_policy_ignores_non_quota_errors() {
        assert!(QuotaBlockPolicy::metadata(&json!({"message": "tool failed"})).is_none());
    }

    #[test]
    fn participant_availability_policy_accepts_available_participant() {
        assert!(
            ParticipantAvailabilityPolicy::ensure_available(
                "Codex",
                "available",
                ParticipantAvailabilityAction::AssignTask
            )
            .is_ok()
        );
    }

    #[test]
    fn participant_availability_policy_renders_action_specific_guidance() {
        let assign_error = match &ParticipantAvailabilityPolicy::ensure_available(
            "Codex",
            "busy",
            ParticipantAvailabilityAction::AssignTask,
        )
        .unwrap_err()
        .kind
        {
            ErrorKind::Validation(message) => message.clone(),
            other => panic!("expected validation error, got {other:?}"),
        };
        let preview_error = match &ParticipantAvailabilityPolicy::ensure_available(
            "Codex",
            "offline",
            ParticipantAvailabilityAction::PreviewContext,
        )
        .unwrap_err()
        .kind
        {
            ErrorKind::Validation(message) => message.clone(),
            other => panic!("expected validation error, got {other:?}"),
        };

        assert!(assign_error.contains("pick an available agent or leave unassigned"));
        assert!(preview_error.contains("preview an available agent"));
    }

    #[test]
    fn participant_availability_policy_includes_participant_context() {
        let error = match &ParticipantAvailabilityPolicy::ensure_available(
            "Codex",
            "busy",
            ParticipantAvailabilityAction::AssignTask,
        )
        .unwrap_err()
        .kind
        {
            ErrorKind::Validation(message) => message.clone(),
            other => panic!("expected validation error, got {other:?}"),
        };

        assert!(error.contains("participant Codex is busy"));
    }

    #[test]
    fn participant_status_policy_sweeps_only_available_heartbeats() {
        assert!(ParticipantStatusPolicy::should_sweep_after_heartbeat("available"));
        assert!(!ParticipantStatusPolicy::should_sweep_after_heartbeat("busy"));
        assert!(!ParticipantStatusPolicy::should_sweep_after_heartbeat("offline"));
    }

    #[test]
    fn dispatch_sweep_policy_claims_working_tasks_and_stops_other_outcomes() {
        assert_eq!(DispatchSweepPolicy::after_dispatch_attempt("working"), DispatchSweepDecision::ClaimedTask);
        assert_eq!(DispatchSweepPolicy::after_dispatch_attempt("blocked"), DispatchSweepDecision::Stop);
        assert_eq!(DispatchSweepPolicy::after_dispatch_attempt("queued"), DispatchSweepDecision::Stop);
    }

    #[test]
    fn task_creation_policy_rejects_preassigned_approval_tasks() {
        assert!(TaskCreationPolicy::ensure_approval_task_is_unassigned(false, Some(AgentId::new())).is_ok());
        assert!(TaskCreationPolicy::ensure_approval_task_is_unassigned(true, None).is_ok());

        let error =
            validation_message(TaskCreationPolicy::ensure_approval_task_is_unassigned(true, Some(AgentId::new())));
        assert!(error.contains("cannot be assigned before approval"));
    }

    #[test]
    fn task_creation_policy_only_starts_assigned_tasks_with_ready_prerequisites() {
        assert!(TaskCreationPolicy::ensure_assigned_task_can_start(&[], None, false).is_ok());
        assert!(TaskCreationPolicy::ensure_assigned_task_can_start(&[], Some("completed"), false).is_ok());

        let missing = vec!["OPENAI_API_KEY".to_string(), "MODEL".to_string()];
        let error = validation_message(TaskCreationPolicy::ensure_assigned_task_can_start(&missing, None, false));
        assert!(error.contains("missing inputs OPENAI_API_KEY, MODEL"));
        assert!(error.contains("leave the task unassigned"));

        let parent =
            validation_message(TaskCreationPolicy::ensure_assigned_task_can_start(&[], Some("working"), false));
        assert!(parent.contains("parent task to finish"));

        let dependencies = validation_message(TaskCreationPolicy::ensure_assigned_task_can_start(&[], None, true));
        assert!(dependencies.contains("prerequisite tasks to finish"));
    }

    #[test]
    fn task_creation_policy_owns_parent_not_found_error() {
        let parent_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        assert!(format!("{}", TaskCreationPolicy::parent_task_not_found(parent_id)).contains("parent task"));
        assert!(matches!(
            TaskCreationPolicy::map_parent_lookup_error(parent_id, ErrorKind::NotFound("task".into()).into()).kind,
            ErrorKind::Validation(message) if message == format!("parent task {parent_id} not found")
        ));

        let internal: AppError = ErrorKind::Internal(anyhow::anyhow!("db failed")).into();
        assert!(matches!(
            TaskCreationPolicy::map_parent_lookup_error(parent_id, internal).kind,
            ErrorKind::Internal(message) if message.to_string().contains("db failed")
        ));
    }

    #[test]
    fn orchestration_transaction_policy_owns_tx_and_outbox_error_contracts() {
        let task_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        assert!(
            format!("{}", OrchestrationTransactionPolicy::begin_failed("assignment", "bad"))
                .contains("begin assignment tx")
        );
        assert!(
            format!("{}", OrchestrationTransactionPolicy::commit_failed("assignment", "bad"))
                .contains("commit assignment tx")
        );
        assert!(
            format!("{}", OrchestrationTransactionPolicy::missing_last_assignment_id(task_id))
                .contains("missing last_assignment_id")
        );
        assert!(
            format!("{}", OrchestrationTransactionPolicy::insert_assignment_outbox_failed("bad"))
                .contains("insert assignment outbox")
        );
    }

    #[test]
    fn orchestration_repository_policy_owns_lookup_and_scope_error_contracts() {
        let task_id = Uuid::new_v4();
        let run_id = Uuid::new_v4();
        let agent_id = AgentId::new();
        let workspace_id = WorkspaceId::new();
        let scope =
            TenantScope::with_axes(OrgId::new(), agentforge_core::UserId::new(), Some(workspace_id), None, None);
        let missing_workspace = crate::test_support::tenant_scope();

        assert!(matches!(
            OrchestrationRepositoryPolicy::task_not_found(task_id).kind,
            ErrorKind::NotFound(message) if message == format!("orchestration task {task_id}")
        ));
        assert!(matches!(
            OrchestrationRepositoryPolicy::participant_not_found(agent_id).kind,
            ErrorKind::NotFound(message) if message == format!("participant for agent {agent_id}")
        ));
        assert!(matches!(
            OrchestrationRepositoryPolicy::task_run_not_found(run_id).kind,
            ErrorKind::NotFound(message) if message == format!("task_run {run_id}")
        ));
        assert!(matches!(
            OrchestrationRepositoryPolicy::task_run_agent_not_found(agent_id).kind,
            ErrorKind::NotFound(message) if message == format!("agent {} for task_run", agent_id.as_uuid())
        ));
        assert!(matches!(
            OrchestrationRepositoryPolicy::missing_assigned_agent_for_task_run(task_id).kind,
            ErrorKind::Internal(_)
        ));
        assert!(matches!(
            OrchestrationRepositoryPolicy::invalid_terminal_task_run_status("paused").kind,
            ErrorKind::Validation(message) if message == "invalid terminal task_run status: paused"
        ));
        assert!(matches!(
            OrchestrationRepositoryPolicy::invalid_context_item_kind("bad").kind,
            ErrorKind::Validation(message) if message == "invalid context item kind: bad"
        ));
        assert!(matches!(
            OrchestrationRepositoryPolicy::invalid_context_ref_kind("bad").kind,
            ErrorKind::Validation(message) if message == "invalid context ref kind: bad"
        ));
        assert_eq!(OrchestrationRepositoryPolicy::required_workspace(&scope).unwrap(), workspace_id);
        assert!(matches!(
            OrchestrationRepositoryPolicy::required_workspace(&missing_workspace).unwrap_err().kind,
            ErrorKind::Forbidden(_)
        ));
        assert!(OrchestrationRepositoryPolicy::ensure_exists_or_forbidden(true).is_ok());
        assert!(matches!(
            OrchestrationRepositoryPolicy::ensure_exists_or_forbidden(false).unwrap_err().kind,
            ErrorKind::Forbidden(_)
        ));
    }

    #[test]
    fn task_lifecycle_policy_guards_complete_and_fail() {
        assert!(TaskLifecyclePolicy::ensure_can_complete("working", None).is_ok());
        assert!(TaskLifecyclePolicy::ensure_can_fail("working").is_ok());

        let complete_error = validation_message(TaskLifecyclePolicy::ensure_can_complete("queued", None));
        let fail_error = validation_message(TaskLifecyclePolicy::ensure_can_fail("completed"));

        assert_eq!(complete_error, "can only complete working tasks, current status: queued");
        assert_eq!(fail_error, "can only fail working tasks, current status: completed");
    }

    #[test]
    fn waiting_verification_hold_can_be_marked_done_by_operator() {
        // #793/#875: accepting a held result is the FE's "mark it done" — it must
        // pass ensure_can_complete even though the task is `blocked`, not `working`.
        assert!(TaskLifecyclePolicy::ensure_can_complete("blocked", Some("waiting_verification")).is_ok());
        // Other blocked reasons must NOT be completable directly (no regression).
        assert!(TaskLifecyclePolicy::ensure_can_complete("blocked", Some("waiting_input")).is_err());
        assert!(TaskLifecyclePolicy::ensure_can_complete("blocked", Some("waiting_approval")).is_err());
        assert!(TaskLifecyclePolicy::ensure_can_complete("blocked", None).is_err());
    }

    #[test]
    fn task_lifecycle_policy_rejects_retrying_pending_approval() {
        assert!(TaskLifecyclePolicy::ensure_can_retry("failed", None, false, false).is_ok());
        assert!(TaskLifecyclePolicy::ensure_can_retry("canceled", None, false, false).is_ok());
        assert!(TaskLifecyclePolicy::ensure_can_retry("blocked", Some("quota_exceeded"), false, true).is_ok());
        assert!(TaskLifecyclePolicy::ensure_can_retry("blocked", Some("waiting_input"), false, false).is_err());
        assert!(TaskLifecyclePolicy::ensure_can_retry("blocked", Some("waiting_dependency"), false, false).is_err());

        let error =
            validation_message(TaskLifecyclePolicy::ensure_can_retry("blocked", Some("waiting_approval"), true, false));
        assert!(error.contains("approve or cancel approval-blocked tasks"));
    }

    #[test]
    fn terminal_tasks_require_the_retry_path() {
        assert!(TaskLifecyclePolicy::ensure_can_cancel("working").is_err());
        assert!(TaskLifecyclePolicy::ensure_can_cancel("canceled").is_ok());
        assert!(TaskLifecyclePolicy::ensure_can_cancel("completed").is_err());
        assert!(TaskLifecyclePolicy::ensure_can_cancel("failed").is_err());
        assert!(TaskPatchPolicy::ensure_current_allows_transition("completed", None, Some("queued")).is_err());
        assert!(TaskPatchPolicy::ensure_current_allows_transition("failed", None, Some("backlog")).is_err());
        assert!(TaskPatchPolicy::ensure_current_allows_transition("canceled", None, Some("canceled")).is_ok());
        assert!(TaskLifecyclePolicy::ensure_no_active_delivery("working", Some(Uuid::new_v4())).is_err());
        assert!(TaskLifecyclePolicy::ensure_no_active_delivery("working", None).is_ok());
        assert!(TaskLifecyclePolicy::ensure_no_active_delivery("blocked", Some(Uuid::new_v4())).is_ok());
    }

    #[test]
    fn task_lifecycle_policy_guards_approval_state() {
        assert!(TaskLifecyclePolicy::ensure_can_approve("blocked", Some("waiting_approval"), true).is_ok());

        let wrong_state = validation_message(TaskLifecyclePolicy::ensure_can_approve("queued", None, true));
        let consumed =
            validation_message(TaskLifecyclePolicy::ensure_can_approve("blocked", Some("waiting_approval"), false));

        assert_eq!(wrong_state, "task is not waiting for approval");
        assert_eq!(consumed, "task approval has already been consumed");
    }

    #[test]
    fn task_patch_policy_validates_progress_range() {
        assert!(TaskPatchPolicy::validate_progress(Some(0)).is_ok());
        assert!(TaskPatchPolicy::validate_progress(Some(100)).is_ok());

        let error = match &TaskPatchPolicy::validate_progress(Some(101)).unwrap_err().kind {
            ErrorKind::Validation(message) => message.clone(),
            other => panic!("expected validation error, got {other:?}"),
        };
        assert_eq!(error, "progress must be 0-100");
    }

    #[test]
    fn task_patch_policy_plans_transition_actions() {
        let agent_id = AgentId::new();
        assert_eq!(
            TaskPatchPolicy::plan(Some("working"), None, None, &Some(Some(agent_id))).unwrap(),
            TaskPatchAction::AssignToParticipant(agent_id)
        );
        assert_eq!(TaskPatchPolicy::plan(Some("working"), None, None, &None).unwrap(), TaskPatchAction::Dispatch);
        assert_eq!(TaskPatchPolicy::plan(Some("completed"), None, None, &None).unwrap(), TaskPatchAction::Complete);
        assert_eq!(TaskPatchPolicy::plan(Some("failed"), None, None, &None).unwrap(), TaskPatchAction::Fail);
        assert_eq!(TaskPatchPolicy::plan(Some("canceled"), None, None, &None).unwrap(), TaskPatchAction::Cancel);
        assert_eq!(TaskPatchPolicy::plan(None, None, None, &Some(None)).unwrap(), TaskPatchAction::Unassign);
        assert_eq!(
            TaskPatchPolicy::plan(Some("queued"), Some("high"), Some(50), &None).unwrap(),
            TaskPatchAction::Patch
        );
    }

    #[test]
    fn task_patch_policy_rejects_incoherent_patch_shapes() {
        let mixed_error = match &TaskPatchPolicy::plan(Some("completed"), Some("high"), None, &None).unwrap_err().kind {
            ErrorKind::Validation(message) => message.clone(),
            other => panic!("expected validation error, got {other:?}"),
        };
        let assignment_scope_error =
            match &TaskPatchPolicy::plan(Some("queued"), None, None, &Some(None)).unwrap_err().kind {
                ErrorKind::Validation(message) => message.clone(),
                other => panic!("expected validation error, got {other:?}"),
            };
        let working_unassign_error =
            match &TaskPatchPolicy::plan(Some("working"), None, None, &Some(None)).unwrap_err().kind {
                ErrorKind::Validation(message) => message.clone(),
                other => panic!("expected validation error, got {other:?}"),
            };

        assert!(mixed_error.contains("must not be combined"));
        assert!(assignment_scope_error.contains("dispatch semantics"));
        assert!(working_unassign_error.contains("cannot unassign while dispatching"));
    }

    #[test]
    fn task_patch_policy_guards_blocked_transitions_and_unassign() {
        assert!(
            TaskPatchPolicy::ensure_current_allows_transition("blocked", Some("waiting_agent"), Some("working"))
                .is_ok()
        );
        assert!(
            TaskPatchPolicy::ensure_current_allows_transition("blocked", Some("waiting_input"), Some("canceled"))
                .is_ok()
        );
        assert!(TaskPatchPolicy::ensure_can_unassign("queued").is_ok());

        let blocked_error =
            match &TaskPatchPolicy::ensure_current_allows_transition("blocked", Some("waiting_input"), Some("working"))
                .unwrap_err()
                .kind
            {
                ErrorKind::Validation(message) => message.clone(),
                other => panic!("expected validation error, got {other:?}"),
            };
        let unassign_error = match &TaskPatchPolicy::ensure_can_unassign("working").unwrap_err().kind {
            ErrorKind::Validation(message) => message.clone(),
            other => panic!("expected validation error, got {other:?}"),
        };

        assert!(blocked_error.contains("task is blocked on waiting_input"));
        assert!(unassign_error.contains("cannot unassign a working task"));
    }

    #[test]
    fn waiting_verification_hold_allows_operator_resolution() {
        for target in ["completed", "working", "canceled"] {
            assert!(
                TaskPatchPolicy::ensure_current_allows_transition(
                    "blocked",
                    Some("waiting_verification"),
                    Some(target),
                )
                .is_ok(),
                "waiting_verification hold must allow operator transition to {target}",
            );
        }
        for target in ["queued", "backlog"] {
            assert!(
                TaskPatchPolicy::ensure_current_allows_transition(
                    "blocked",
                    Some("waiting_verification"),
                    Some(target),
                )
                .is_err(),
                "verification retry must use the dedicated Retry path",
            );
        }
    }

    #[test]
    fn waiting_verification_carve_out_does_not_regress_other_blocked_reasons() {
        // The carve-out is scoped to waiting_verification: other non-dispatch
        // blocked reasons must still reject the same operator-resolution targets
        // (only `canceled` stays universally allowed).
        for reason in ["waiting_input", "waiting_approval", "waiting_dependency", "quota_exceeded"] {
            for blocked_target in ["completed", "queued", "working", "backlog"] {
                assert!(
                    TaskPatchPolicy::ensure_current_allows_transition("blocked", Some(reason), Some(blocked_target))
                        .is_err(),
                    "{reason} must still reject → {blocked_target}",
                );
            }
            assert!(
                TaskPatchPolicy::ensure_current_allows_transition("blocked", Some(reason), Some("canceled")).is_ok(),
                "{reason} must still allow → canceled",
            );
        }
    }

    #[test]
    fn waiting_verification_is_retryable() {
        // The kanban re-run path goes through ensure_can_retry; a verification hold
        // is an explicit retryable operator hold even without `retryable=true`.
        assert!(TaskLifecyclePolicy::ensure_can_retry("blocked", Some("waiting_verification"), false, false).is_ok());
        assert!(TaskLifecyclePolicy::ensure_can_retry("blocked", Some("waiting_verification"), true, false).is_ok());
    }

    #[test]
    fn task_patch_policy_builds_manual_terminal_payloads() {
        assert_eq!(TaskPatchPolicy::manual_complete_result(), json!({ "manual": true, "source": "kanban_patch" }));
        assert_eq!(
            TaskPatchPolicy::manual_failure_error(),
            json!({
                "message": "manual failure via task patch",
                "source": "kanban_patch"
            })
        );
    }

    #[test]
    fn task_patch_policy_auto_dispatches_only_unassigned_dispatchable_lanes() {
        let agent_id = AgentId::from(Uuid::nil());

        assert!(TaskPatchPolicy::should_auto_dispatch_after_patch(Some("queued"), None));
        assert!(TaskPatchPolicy::should_auto_dispatch_after_patch(Some("backlog"), None));
        assert!(!TaskPatchPolicy::should_auto_dispatch_after_patch(Some("queued"), Some(agent_id)));
        assert!(!TaskPatchPolicy::should_auto_dispatch_after_patch(Some("working"), None));
        assert!(!TaskPatchPolicy::should_auto_dispatch_after_patch(None, None));
    }

    #[test]
    fn assignment_patch_policy_handles_absent_unassign_and_uuid() {
        assert!(matches!(TaskAssignmentPatchPolicy::parse(None), Ok(None)));
        assert!(matches!(TaskAssignmentPatchPolicy::parse(Some("")).unwrap(), Some(None)));
        assert!(matches!(
            TaskAssignmentPatchPolicy::parse(Some("00000000-0000-0000-0000-000000000001")).unwrap(),
            Some(Some(_))
        ));
    }

    #[test]
    fn assignment_patch_policy_rejects_invalid_uuid() {
        assert!(TaskAssignmentPatchPolicy::parse(Some("not-a-uuid")).is_err());
    }
}
