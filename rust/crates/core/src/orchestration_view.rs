//! Pure orchestration task policies shared by the api domain and the jobs
//! WS projector (MS-3 PR-E).
//!
//! These used to live in `api::domain::orchestration` while
//! `jobs::orchestration_realtime` kept hand-rolled mirrors (`task_instruction`,
//! `blocked_hint`) — the two `orchestration:task_update` producers drifted as a
//! result. They are hosted here (core depends on nothing internal) so both the
//! api services adapter and the jobs adapter build the wire projection
//! ([`crate::ws_protocol::TaskSummary`]) from the same policy code. The api
//! domain re-exports them, so its routes/services/tests are unchanged.

use serde_json::{Value, json};

use crate::{AppResult, ErrorKind};

const VALID_BLOCKED_REASONS: &[&str] = &[
    "waiting_agent",
    "waiting_dependency",
    "waiting_input",
    "waiting_approval",
    "quota_exceeded",
    "waiting_verification",
];

/// User-facing task instruction carried by summary responses and assignment
/// delivery. Structured params win, with legacy title/description fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInstruction {
    task: String,
    message: String,
}

impl TaskInstruction {
    pub fn from_params(title: &str, description: Option<&str>, params: Option<&Value>) -> Self {
        params
            .map(|p| Self {
                task: p.get("task").and_then(|v| v.as_str()).unwrap_or(title).to_string(),
                message: p.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
            .unwrap_or_else(|| Self { task: title.to_string(), message: description.unwrap_or_default().to_string() })
    }

    pub fn into_parts(self) -> (String, String) {
        (self.task, self.message)
    }
}

/// Blocked-task policy and blocked-card hint rendering.
pub struct BlockedTaskPolicy;

impl BlockedTaskPolicy {
    pub fn waiting_agent_reason() -> &'static str {
        "waiting_agent"
    }

    pub fn is_valid_reason(reason: &str) -> bool {
        VALID_BLOCKED_REASONS.contains(&reason)
    }

    pub fn reason_allows_dispatch(reason: Option<&str>) -> bool {
        reason.is_none() || reason == Some(Self::waiting_agent_reason())
    }

    /// Statuses the auto-pickup loop may dispatch from. `blocked` is included
    /// because `waiting_agent` blocks should auto-clear when an agent returns;
    /// `backlog` is intentionally excluded (draft lane, explicit promotion only).
    /// The api-domain `TaskStatusPolicy::can_dispatch` delegates here so the
    /// status set has exactly one owner.
    pub fn status_can_dispatch(status: &str) -> bool {
        matches!(status, "queued" | "blocked")
    }

    pub fn can_enter_dispatch(status: &str, blocked_reason: Option<&str>) -> bool {
        Self::status_can_dispatch(status) && Self::reason_allows_dispatch(blocked_reason)
    }

    pub fn ensure_can_enter_dispatch(status: &str, blocked_reason: Option<&str>) -> AppResult<()> {
        if Self::can_enter_dispatch(status, blocked_reason) {
            return Ok(());
        }
        Err(ErrorKind::Validation(format!(
            "can only dispatch queued or waiting-agent tasks, current status: {status}, blocked reason: {}",
            blocked_reason.unwrap_or("none")
        ))
        .into())
    }

    /// Guard for the EXPLICIT operator dispatch path (`POST /tasks/:id/dispatch`
    /// and a kanban drag to "working"). Same as `ensure_can_enter_dispatch` plus a
    /// #793/#875 carve-out: an operator may re-run a `waiting_verification` hold.
    /// The auto-sweep deliberately does NOT use this — it keeps `can_enter_dispatch`
    /// (and the `next_dispatchable` SQL filter), so a held task is never
    /// auto-claimed and stays human-gated.
    pub fn ensure_operator_can_dispatch(status: &str, blocked_reason: Option<&str>) -> AppResult<()> {
        if status == "blocked" && blocked_reason == Some("waiting_verification") {
            return Ok(());
        }
        Self::ensure_can_enter_dispatch(status, blocked_reason)
    }

    pub fn no_available_participants_error() -> ErrorKind {
        ErrorKind::Validation("no available participants for dispatch".into())
    }

    pub fn waiting_agent_metadata(available: i64, busy: i64, offline: i64) -> Value {
        json!({
            "available": available,
            "busy": busy,
            "offline": offline,
        })
    }

    /// Child tasks with an unfinished parent start in
    /// `blocked/waiting_dependency`. `failed` and `canceled` parents are also
    /// kept blocked until an operator explicitly decides what to do.
    pub fn needs_dependency_block(parent_status: Option<&str>) -> bool {
        match parent_status {
            None | Some("completed") => false,
            Some(_) => true,
        }
    }

    pub fn initial_state(
        missing_inputs: &[String],
        requires_approval: bool,
        dependency_blocked: bool,
    ) -> (Option<&'static str>, Option<Value>) {
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

    pub fn approval_release_state(parent_status: Option<&str>) -> (&'static str, Option<&'static str>, Option<Value>) {
        if Self::needs_dependency_block(parent_status) {
            return ("blocked", Some("waiting_dependency"), Some(json!({ "pending": 1 })));
        }
        ("queued", None, None)
    }

    pub fn should_auto_dispatch_after_approval(status: &str) -> bool {
        status == "queued"
    }

    pub fn missing_required_inputs(params: Option<&Value>) -> Vec<String> {
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
    pub fn hint(reason: &str, metadata: Option<&Value>) -> String {
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
            "waiting_verification" => "完成结果未通过 expectedResult 校验，已暂留待人工复核".into(),
            other => format!("阻塞: {other}"),
        }
    }
}

fn input_value_present(params: &Value, name: &str) -> bool {
    ["inputs", "env", "apiKeys", "api_keys"]
        .iter()
        .filter_map(|key| params.get(*key))
        .any(|container| value_has_non_empty_field(container, name))
        || value_has_non_empty_field(params, name)
}

fn value_has_non_empty_field(value: &Value, name: &str) -> bool {
    value.get(name).is_some_and(|v| match v {
        Value::String(s) => !s.trim().is_empty(),
        Value::Null => false,
        _ => true,
    })
}
