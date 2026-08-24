//! Realtime WS projector for orchestration task/participant changes.
//!
//! MS-3 PR-E: this module used to hand-roll its `orchestration:task_update`
//! frame with a `json!` mirror of the api projection (which had drifted — no
//! `selfFix`/`pr*`/`contextCounts`/`attempt`/`leaseExpiresAt`, null-emitting
//! options, a duplicated Chinese blocked-hint renderer). It now hosts the ONE
//! canonical `OrchestrationTask` row → [`TaskSummary`] adapter (the api service
//! re-exports it) and publishes through the shared
//! [`agentforge_core::ws_protocol::ServerMessage`] enum.

use agentforge_core::OrgId;
use agentforge_core::orchestration_view::{BlockedTaskPolicy, TaskInstruction};
use agentforge_core::ws_protocol::{
    OrchestrationTaskUpdatePayload, ServerMessage, TaskContextCounts, TaskParams, TaskSummary,
};
use agentforge_db::entities::OrchestrationTask;
use anyhow::{Context, Result};
use async_nats::Client;
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

    let frame = ServerMessage::OrchestrationTaskUpdate {
        payload: OrchestrationTaskUpdatePayload {
            action: action.to_owned(),
            event_id: Uuid::now_v7(),
            task: task_summary(task, assigned_agent_name),
        },
    };
    publish_broadcast(client, task.organization_id, &frame).await
}

pub(crate) async fn publish_broadcast(client: &Client, organization_id: OrgId, message: &ServerMessage) -> Result<()> {
    let payload = serde_json::to_vec(message).with_context(|| "serialize broadcast payload")?;
    client
        .publish(format!("broadcast.{}", organization_id.as_uuid()), payload.into())
        .await
        .with_context(|| format!("publish broadcast for org {}", organization_id))?;
    Ok(())
}

/// Project a persisted `OrchestrationTask` row onto the kanban [`TaskSummary`].
///
/// The canonical adapter for BOTH `orchestration:task_update` producers: the
/// jobs projector calls it directly; the api service's `task_summary` is a thin
/// wrapper over it (the REST/context-injection path then overwrites
/// `context_counts` with real counts — this projector keeps the zero default,
/// as counting per broadcast would add a query to a hot path).
pub fn task_summary(task: &OrchestrationTask, assigned_agent_name: Option<&str>) -> TaskSummary {
    let blocked_hint = match task.status.as_str() {
        "blocked" => {
            task.blocked_reason.as_deref().map(|reason| BlockedTaskPolicy::hint(reason, task.blocked_metadata.as_ref()))
        }
        _ => None,
    };

    let (task_text, message) =
        TaskInstruction::from_params(&task.title, task.description.as_deref(), task.params.as_ref()).into_parts();
    let params = TaskParams { task: task_text, message };

    let error = task
        .error
        .as_ref()
        .map(|e| e.get("message").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| e.to_string()));

    let is_completed = task.status == "completed";

    TaskSummary {
        id: task.id,
        group_id: task.group_id,
        state: task.status.clone(),
        method: "tasks/send".into(),
        params,
        priority: task.priority.clone(),
        progress: task.progress,
        created_by: task.created_by.as_uuid(),
        assigned_to: task.assigned_agent_id.map(|a| a.as_uuid()),
        assigned_agent_name: assigned_agent_name.map(str::to_owned),
        error,
        result: task.result.clone(),
        blocked_reason: task.blocked_reason.clone(),
        blocked_hint,
        blocked_metadata: task.blocked_metadata.clone(),
        created_at: task.created_at.to_rfc3339(),
        updated_at: task.updated_at.to_rfc3339(),
        completed_at: if is_completed { task.completed_at.map(|t| t.to_rfc3339()) } else { None },
        self_fix: task.self_fix,
        pr_number: task.pr_number,
        pr_url: task.pr_url.clone(),
        pr_head_sha: task.pr_head_sha.clone(),
        review_status: task.review_status.clone(),
        context_counts: TaskContextCounts::default(),
        attempt: task.attempt,
        lease_expires_at: task.lease_expires_at.map(|t| t.to_rfc3339()),
        // Wait predictions are computed by the API service on its list-task
        // read path (org queue snapshot); the realtime projector keeps the
        // field absent so broadcast frames stay cheap and fixtures stable.
        wait_estimate: None,
    }
}

pub(crate) fn realtime_projector_enabled() -> bool {
    env_flag("ORCHESTRATION_WS_PROJECTOR_ENABLED", true)
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
    use serde_json::json;

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
            merge_attempts: 0,
            review_opened_at: None,
        }
    }

    #[test]
    fn task_ws_summary_includes_owner_for_inbox_notifications() {
        let task = task_with("failed");
        let owner = task.created_by.as_uuid();

        let summary = task_summary(&task, Some("Codex"));

        assert_eq!(summary.created_by, owner);
        assert_eq!(summary.assigned_agent_name.as_deref(), Some("Codex"));
        // The serialized frame carries the camelCase wire names.
        let value = serde_json::to_value(&summary).expect("summary serializes");
        assert_eq!(value["createdBy"], json!(owner));
        assert_eq!(value["assignedAgentName"], "Codex");
    }

    #[test]
    fn blocked_task_ws_summary_includes_human_hint() {
        let mut task = task_with("blocked");
        task.blocked_reason = Some("waiting_agent".to_string());
        task.blocked_metadata = Some(json!({ "busy": 2, "offline": 1 }));

        let summary = task_summary(&task, None);

        assert_eq!(summary.blocked_hint.as_deref(), Some("等待空闲 agent（2 个忙碌, 1 个离线）"));
    }
}
