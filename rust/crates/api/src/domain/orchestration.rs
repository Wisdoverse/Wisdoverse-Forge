//! Orchestration domain rules.
//!
//! This module owns kanban task, participant, dispatch, and blocking policies
//! that are independent of SQL repositories, transactions, context injection,
//! and outbox delivery.

use agentforge_core::context_envelope::ContextEnvelope;
use agentforge_core::orchestration_protocol::TaskAssignment;
use agentforge_core::{AgentId, AppResult, ErrorKind};
use agentforge_db::entities::OrchestrationTask;
use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::domain::context_resolver::ResolvedContext;

const VALID_TASK_STATUSES: &[&str] = &["backlog", "queued", "working", "blocked", "completed", "failed", "canceled"];
const KANBAN_DROP_STATUSES: &[&str] = &["backlog", "queued", "working", "blocked", "completed"];
const VALID_PRIORITIES: &[&str] = &["low", "normal", "high", "urgent"];
const VALID_PARTICIPANT_STATUSES: &[&str] = &["available", "busy", "offline"];
const VALID_BLOCKED_REASONS: &[&str] =
    &["waiting_agent", "waiting_dependency", "waiting_input", "waiting_approval", "quota_exceeded"];

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

    /// Unassigned tasks land in `backlog` unless missing declared inputs,
    /// human approval, or an unfinished parent requires an initial block.
    pub(crate) fn initial_unassigned_state(
        missing_inputs: &[String],
        requires_approval: bool,
        parent_status: Option<&str>,
    ) -> TaskCreationInitialState {
        let dependency_blocked = BlockedTaskPolicy::needs_dependency_block(parent_status);
        let (initial_blocked_reason, initial_blocked_metadata) =
            BlockedTaskPolicy::initial_state(missing_inputs, requires_approval, dependency_blocked);
        let initial_status = if initial_blocked_reason.is_some() { "blocked" } else { "backlog" };

        TaskCreationInitialState { initial_status, initial_blocked_reason, initial_blocked_metadata }
    }
}

/// User-facing task instruction carried by summary responses and assignment
/// delivery. Structured params win, with legacy title/description fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskInstruction {
    task: String,
    message: String,
}

impl TaskInstruction {
    pub(crate) fn from_params(title: &str, description: Option<&str>, params: Option<&serde_json::Value>) -> Self {
        params
            .map(|p| Self {
                task: p.get("task").and_then(|v| v.as_str()).unwrap_or(title).to_string(),
                message: p.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
            .unwrap_or_else(|| Self { task: title.to_string(), message: description.unwrap_or_default().to_string() })
    }

    pub(crate) fn into_parts(self) -> (String, String) {
        (self.task, self.message)
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

/// Assignment delivery protocol policy.
pub(crate) struct TaskAssignmentPolicy;

impl TaskAssignmentPolicy {
    pub(crate) fn build(
        snapshot: TaskAssignmentSnapshot<'_>,
        context_envelope: Option<ContextEnvelope>,
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

    /// Tasks dispatchable by the auto-pickup loop. `blocked` is included
    /// because `waiting_agent` blocks should auto-clear when an agent returns.
    /// `backlog` is intentionally excluded: it is the draft lane and must be
    /// explicitly promoted before the dispatcher can claim it.
    pub(crate) fn can_dispatch(status: &str) -> bool {
        matches!(status, "queued" | "blocked")
    }

    pub(crate) fn can_complete_or_fail(status: &str) -> bool {
        status == "working"
    }
}

/// Terminal and approval lifecycle guards for orchestration tasks.
pub(crate) struct TaskLifecyclePolicy;

impl TaskLifecyclePolicy {
    pub(crate) fn ensure_can_complete(status: &str) -> AppResult<()> {
        Self::ensure_working_action(status, "complete")
    }

    pub(crate) fn ensure_can_fail(status: &str) -> AppResult<()> {
        Self::ensure_working_action(status, "fail")
    }

    pub(crate) fn ensure_can_retry(
        status: &str,
        blocked_reason: Option<&str>,
        requires_approval: bool,
    ) -> AppResult<()> {
        if status == "blocked" && blocked_reason == Some("waiting_approval") && requires_approval {
            return Err(ErrorKind::Validation("approve or cancel approval-blocked tasks before retry".into()).into());
        }
        Ok(())
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

    pub(crate) fn ensure_current_allows_transition(
        current_status: &str,
        current_blocked_reason: Option<&str>,
        transition_state: Option<&str>,
    ) -> AppResult<()> {
        if current_status == "blocked"
            && !BlockedTaskPolicy::reason_allows_dispatch(current_blocked_reason)
            && !matches!(transition_state, Some("canceled"))
        {
            return Err(ErrorKind::Validation(format!(
                "task is blocked on {}; use its unblock path before dispatching",
                current_blocked_reason.unwrap_or("unknown")
            ))
            .into());
        }
        Ok(())
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

/// Blocked-task policy and blocked-card hint rendering.
pub(crate) struct BlockedTaskPolicy;

impl BlockedTaskPolicy {
    pub(crate) fn waiting_agent_reason() -> &'static str {
        "waiting_agent"
    }

    pub(crate) fn is_valid_reason(reason: &str) -> bool {
        VALID_BLOCKED_REASONS.contains(&reason)
    }

    pub(crate) fn reason_allows_dispatch(reason: Option<&str>) -> bool {
        reason.is_none() || reason == Some(Self::waiting_agent_reason())
    }

    pub(crate) fn can_enter_dispatch(status: &str, blocked_reason: Option<&str>) -> bool {
        TaskStatusPolicy::can_dispatch(status) && Self::reason_allows_dispatch(blocked_reason)
    }

    pub(crate) fn ensure_can_enter_dispatch(status: &str, blocked_reason: Option<&str>) -> AppResult<()> {
        if Self::can_enter_dispatch(status, blocked_reason) {
            return Ok(());
        }
        Err(ErrorKind::Validation(format!(
            "can only dispatch queued or waiting-agent tasks, current status: {status}, blocked reason: {}",
            blocked_reason.unwrap_or("none")
        ))
        .into())
    }

    pub(crate) fn no_available_participants_error() -> ErrorKind {
        ErrorKind::Validation("no available participants for dispatch".into())
    }

    pub(crate) fn waiting_agent_metadata(available: i64, busy: i64, offline: i64) -> serde_json::Value {
        json!({
            "available": available,
            "busy": busy,
            "offline": offline,
        })
    }

    /// Child tasks with an unfinished parent start in
    /// `blocked/waiting_dependency`. `failed` and `canceled` parents are also
    /// kept blocked until an operator explicitly decides what to do.
    pub(crate) fn needs_dependency_block(parent_status: Option<&str>) -> bool {
        match parent_status {
            None | Some("completed") => false,
            Some(_) => true,
        }
    }

    pub(crate) fn initial_state(
        missing_inputs: &[String],
        requires_approval: bool,
        dependency_blocked: bool,
    ) -> (Option<&'static str>, Option<serde_json::Value>) {
        if !missing_inputs.is_empty() {
            return (Some("waiting_input"), Some(json!({ "missing": missing_inputs })));
        }
        if requires_approval {
            return (Some("waiting_approval"), Some(json!({ "approver": "管理员" })));
        }
        if dependency_blocked {
            return (Some("waiting_dependency"), Some(json!({ "pending": 1 })));
        }
        (None, None)
    }

    pub(crate) fn approval_release_state(
        parent_status: Option<&str>,
    ) -> (&'static str, Option<&'static str>, Option<serde_json::Value>) {
        if Self::needs_dependency_block(parent_status) {
            return ("blocked", Some("waiting_dependency"), Some(json!({ "pending": 1 })));
        }
        ("queued", None, None)
    }

    pub(crate) fn should_auto_dispatch_after_approval(status: &str) -> bool {
        status == "queued"
    }

    pub(crate) fn missing_required_inputs(params: Option<&serde_json::Value>) -> Vec<String> {
        let Some(params) = params else {
            return Vec::new();
        };
        let required = params
            .get("requiredInputs")
            .or_else(|| params.get("required_inputs"))
            .and_then(|v| v.as_array())
            .map(|fields| fields.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
            .unwrap_or_default();

        required.into_iter().filter(|name| !input_value_present(params, name)).map(str::to_string).collect()
    }

    /// Render a human-readable hint describing what the task is waiting on.
    pub(crate) fn hint(reason: &str, metadata: Option<&serde_json::Value>) -> String {
        if !Self::is_valid_reason(reason) {
            return format!("阻塞: {reason}");
        }

        match reason {
            "waiting_agent" => {
                let busy = metadata.and_then(|m| m.get("busy")).and_then(|v| v.as_i64()).unwrap_or(0);
                let offline = metadata.and_then(|m| m.get("offline")).and_then(|v| v.as_i64()).unwrap_or(0);
                if busy + offline == 0 {
                    "等待 agent：当前组织内没有注册的 participant".into()
                } else {
                    format!("等待空闲 agent（{busy} 个忙碌, {offline} 个离线）")
                }
            }
            "waiting_dependency" => {
                let pending = metadata.and_then(|m| m.get("pending")).and_then(|v| v.as_i64()).unwrap_or(0);
                format!("等待 {pending} 个上游任务完成")
            }
            "waiting_input" => {
                let fields = metadata
                    .and_then(|m| m.get("missing"))
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
                    .unwrap_or_default();
                if fields.is_empty() { "等待补充输入".into() } else { format!("缺少输入: {fields}") }
            }
            "waiting_approval" => {
                let approver = metadata.and_then(|m| m.get("approver")).and_then(|v| v.as_str()).unwrap_or("管理员");
                format!("等待 {approver} 审批")
            }
            "quota_exceeded" => {
                let used = metadata.and_then(|m| m.get("used")).and_then(|v| v.as_i64()).unwrap_or(0);
                let limit = metadata.and_then(|m| m.get("limit")).and_then(|v| v.as_i64()).unwrap_or(0);
                format!("配额超限（{used}/{limit}）")
            }
            other => format!("阻塞: {other}"),
        }
    }
}

fn input_value_present(params: &serde_json::Value, name: &str) -> bool {
    ["inputs", "env", "apiKeys", "api_keys"]
        .iter()
        .filter_map(|key| params.get(*key))
        .any(|container| value_has_non_empty_field(container, name))
        || value_has_non_empty_field(params, name)
}

fn value_has_non_empty_field(value: &serde_json::Value, name: &str) -> bool {
    value.get(name).is_some_and(|v| match v {
        serde_json::Value::String(s) => !s.trim().is_empty(),
        serde_json::Value::Null => false,
        _ => true,
    })
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
    use super::*;

    fn validation_message(result: AppResult<()>) -> String {
        match result.unwrap_err().kind {
            ErrorKind::Validation(message) => message,
            other => panic!("expected validation error, got {other:?}"),
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
    fn task_creation_initial_state_keeps_ready_unassigned_tasks_in_backlog() {
        let state = TaskCreationPolicy::initial_unassigned_state(&[], false, None);

        assert_eq!(state.initial_status, "backlog");
        assert_eq!(state.initial_blocked_reason, None);
        assert_eq!(state.initial_blocked_metadata, None);
    }

    #[test]
    fn task_creation_initial_state_blocks_missing_inputs_first() {
        let missing_inputs = vec!["api_key".to_string(), "region".to_string()];
        let state = TaskCreationPolicy::initial_unassigned_state(&missing_inputs, true, Some("working"));

        assert_eq!(state.initial_status, "blocked");
        assert_eq!(state.initial_blocked_reason, Some("waiting_input"));
        assert_eq!(state.initial_blocked_metadata, Some(json!({ "missing": ["api_key", "region"] })));
    }

    #[test]
    fn task_creation_initial_state_blocks_approval_before_dependency() {
        let state = TaskCreationPolicy::initial_unassigned_state(&[], true, Some("working"));

        assert_eq!(state.initial_status, "blocked");
        assert_eq!(state.initial_blocked_reason, Some("waiting_approval"));
        assert_eq!(state.initial_blocked_metadata, Some(json!({ "approver": "管理员" })));
    }

    #[test]
    fn task_creation_initial_state_blocks_unfinished_parent() {
        let state = TaskCreationPolicy::initial_unassigned_state(&[], false, Some("working"));

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
        )
        .unwrap_err();

        match err.kind {
            ErrorKind::Internal(message) => assert!(message.to_string().contains("missing assigned_agent_id")),
            other => panic!("expected internal error, got {other:?}"),
        }
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
    fn task_lifecycle_policy_guards_complete_and_fail() {
        assert!(TaskLifecyclePolicy::ensure_can_complete("working").is_ok());
        assert!(TaskLifecyclePolicy::ensure_can_fail("working").is_ok());

        let complete_error = validation_message(TaskLifecyclePolicy::ensure_can_complete("queued"));
        let fail_error = validation_message(TaskLifecyclePolicy::ensure_can_fail("completed"));

        assert_eq!(complete_error, "can only complete working tasks, current status: queued");
        assert_eq!(fail_error, "can only fail working tasks, current status: completed");
    }

    #[test]
    fn task_lifecycle_policy_rejects_retrying_pending_approval() {
        assert!(TaskLifecyclePolicy::ensure_can_retry("failed", None, false).is_ok());
        assert!(TaskLifecyclePolicy::ensure_can_retry("blocked", Some("waiting_approval"), false).is_ok());

        let error =
            validation_message(TaskLifecyclePolicy::ensure_can_retry("blocked", Some("waiting_approval"), true));
        assert!(error.contains("approve or cancel approval-blocked tasks"));
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
