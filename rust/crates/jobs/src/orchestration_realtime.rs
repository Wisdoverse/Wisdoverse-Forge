use agentforge_core::OrgId;
use agentforge_db::entities::OrchestrationTask;
use anyhow::{Context, Result};
use async_nats::Client;
use serde_json::{Value, json};
use uuid::Uuid;

pub(crate) async fn publish_task_update(
    client: &Client,
    task: &OrchestrationTask,
    assigned_agent_name: Option<&str>,
    action: &str,
) -> Result<()> {
    if !realtime_projector_enabled() {
        tracing::debug!(%action, task_id = %task.id, "orchestration realtime projector disabled");
        return Ok(());
    }

    publish_broadcast(
        client,
        task.organization_id,
        json!({
            "type": "orchestration:task_update",
            "payload": {
                "action": action,
                "eventId": Uuid::now_v7(),
                "task": summarize_task_for_ws(task, assigned_agent_name),
            }
        }),
    )
    .await
}

pub(crate) async fn publish_broadcast(client: &Client, organization_id: OrgId, message: Value) -> Result<()> {
    let payload = serde_json::to_vec(&message).with_context(|| "serialize broadcast payload")?;
    client
        .publish(format!("broadcast.{}", organization_id.as_uuid()), payload.into())
        .await
        .with_context(|| format!("publish broadcast for org {}", organization_id))?;
    Ok(())
}

pub(crate) fn summarize_task_for_ws(task: &OrchestrationTask, assigned_agent_name: Option<&str>) -> Value {
    let (task_text, message) = task_instruction(task);
    let error = task.error.as_ref().map(error_message);
    let blocked_hint = match task.status.as_str() {
        "blocked" => task.blocked_reason.as_deref().map(|reason| blocked_hint(reason, task.blocked_metadata.as_ref())),
        _ => None,
    };

    json!({
        "id": task.id,
        "groupId": task.group_id.map(|id| id.to_string()).unwrap_or_default(),
        "state": task.status,
        "method": "tasks/send",
        "createdBy": task.created_by.as_uuid(),
        "assignedTo": task.assigned_agent_id.map(|agent_id| agent_id.as_uuid()),
        "assignedAgentName": assigned_agent_name,
        "progress": task.progress,
        "priority": task.priority,
        "params": {
            "task": task_text,
            "message": message,
        },
        "error": error,
        "result": task.result,
        "blockedReason": task.blocked_reason,
        "blockedHint": blocked_hint,
        "blockedMetadata": task.blocked_metadata,
        "createdAt": task.created_at.to_rfc3339(),
        "updatedAt": task.updated_at.to_rfc3339(),
        "completedAt": task.completed_at.map(|t| t.to_rfc3339()),
    })
}

pub(crate) fn task_instruction(task: &OrchestrationTask) -> (String, String) {
    task.params
        .as_ref()
        .map(|params| {
            (
                params.get("task").and_then(|v| v.as_str()).unwrap_or(&task.title).to_string(),
                params
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| task.description.as_deref().unwrap_or_default())
                    .to_string(),
            )
        })
        .unwrap_or_else(|| (task.title.clone(), task.description.clone().unwrap_or_default()))
}

pub(crate) fn realtime_projector_enabled() -> bool {
    env_flag("ORCHESTRATION_WS_PROJECTOR_ENABLED", true)
}

fn error_message(error: &Value) -> String {
    error.get("message").and_then(|v| v.as_str()).map(str::to_string).unwrap_or_else(|| error.to_string())
}

fn blocked_hint(reason: &str, metadata: Option<&Value>) -> String {
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

fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentforge_core::{AgentId, OrgId, UserId};
    use chrono::Utc;

    fn task_with(status: &str) -> OrchestrationTask {
        let now = Utc::now();
        OrchestrationTask {
            id: Uuid::now_v7(),
            organization_id: OrgId::from(Uuid::now_v7()),
            group_id: Some(Uuid::now_v7()),
            title: "Deploy staging".to_string(),
            description: Some("Ship the build".to_string()),
            status: status.to_string(),
            priority: "normal".to_string(),
            progress: 0,
            params: Some(json!({ "task": "Deploy staging", "message": "Ship the build" })),
            created_by: UserId::from(Uuid::now_v7()),
            assigned_agent_id: Some(AgentId::from(Uuid::now_v7())),
            parent_task_id: None,
            result: None,
            error: None,
            blocked_reason: None,
            blocked_metadata: None,
            requires_approval: false,
            approved_at: None,
            approved_by: None,
            attempt: 1,
            lease_expires_at: None,
            failure_code: None,
            retryable: false,
            last_assignment_id: Some(Uuid::now_v7()),
            started_at: Some(now),
            completed_at: None,
            canceled_at: None,
            created_at: now,
            updated_at: now,
            self_fix: false,
            base_commit_sha: None,
            pr_number: None,
            pr_url: None,
            pr_head_sha: None,
            review_status: None,
        }
    }

    #[test]
    fn task_ws_summary_includes_owner_for_inbox_notifications() {
        let task = task_with("failed");
        let owner = task.created_by.as_uuid();

        let summary = summarize_task_for_ws(&task, Some("Codex"));

        assert_eq!(summary["createdBy"], json!(owner));
        assert_eq!(summary["assignedAgentName"], "Codex");
    }

    #[test]
    fn blocked_task_ws_summary_includes_human_hint() {
        let mut task = task_with("blocked");
        task.blocked_reason = Some("waiting_agent".to_string());
        task.blocked_metadata = Some(json!({ "busy": 2, "offline": 1 }));

        let summary = summarize_task_for_ws(&task, None);

        assert_eq!(summary["blockedHint"], "等待空闲 agent（2 个忙碌, 1 个离线）");
    }
}
