//! Orchestration domain rules.
//!
//! This module owns kanban task, participant, dispatch, and blocking policies
//! that are independent of SQL repositories, transactions, context injection,
//! and outbox delivery.

use agentforge_core::{AgentId, AppResult, ErrorKind};
use serde_json::json;
use uuid::Uuid;

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
pub(crate) struct TaskPatchPolicy;

impl TaskPatchPolicy {
    pub(crate) fn touches_assignment(assigned_to: &Option<Option<AgentId>>) -> bool {
        assigned_to.is_some()
    }

    pub(crate) fn is_business_transition(state: Option<&str>, assigned_to: &Option<Option<AgentId>>) -> bool {
        matches!(state, Some("working" | "completed" | "failed" | "canceled")) || matches!(assigned_to, Some(Some(_)))
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
    pub(crate) fn is_valid_reason(reason: &str) -> bool {
        VALID_BLOCKED_REASONS.contains(&reason)
    }

    pub(crate) fn reason_allows_dispatch(reason: Option<&str>) -> bool {
        matches!(reason, None | Some("waiting_agent"))
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

    #[test]
    fn list_page_clamps_limit_and_offset() {
        assert_eq!(TaskListPage::new(0, -1).limit(), 1);
        assert_eq!(TaskListPage::new(101, 50).limit(), 100);
        assert_eq!(TaskListPage::new(20, -1).offset(), 0);
        assert_eq!(TaskListPage::new(20, 50).offset(), 50);
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
