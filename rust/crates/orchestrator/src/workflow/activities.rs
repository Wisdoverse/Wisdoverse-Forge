use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use temporalio_macros::activities;
use temporalio_sdk::ApplicationFailure;
use temporalio_sdk::activities::{ActivityContext, ActivityError};

use crate::mcp::client::{CreateSessionArgs, OutboundMcp};
use crate::realtime::{Broadcaster, Event};

use super::model::{NodeStatus, WorkflowStatus};
use super::store::Store;

#[derive(Clone)]
pub struct WorkflowActivities {
    mcp: Arc<dyn OutboundMcp>,
    store: Arc<dyn Store>,
    broadcaster: Option<Arc<Broadcaster>>,
}

impl WorkflowActivities {
    pub fn new(mcp: Arc<dyn OutboundMcp>, store: Arc<dyn Store>, broadcaster: Option<Arc<Broadcaster>>) -> Self {
        Self { mcp, store, broadcaster }
    }
}

#[activities]
impl WorkflowActivities {
    #[activity(name = "ExecuteAgentTask")]
    pub async fn execute_agent_task(
        self: Arc<Self>,
        ctx: ActivityContext,
        input: ExecuteAgentTaskInput,
    ) -> Result<ExecuteAgentTaskOutput, ActivityError> {
        self.log_node_status_err(
            &input.node_id,
            self.store.update_node_status(&input.node_id, NodeStatus::Running, None, None).await,
        );
        self.emit_node_status(&input.org_id, &input.node_id, &input.node_name, NodeStatus::Running, None);

        let project_id = config_string(input.config.as_ref(), "projectId").unwrap_or_default();
        let cli_tool = config_string(input.config.as_ref(), "cliTool").unwrap_or_else(|| "claude".to_string());
        let prompt = config_string(input.config.as_ref(), "prompt").unwrap_or_else(|| input.node_name.clone());

        let session = match self
            .mcp
            .session_create(CreateSessionArgs {
                org_id: input.org_id.clone(),
                project_id,
                cli_tool,
                name: Some(input.node_name.clone()),
            })
            .await
        {
            Ok(session) => session,
            Err(err) => {
                let err_msg = format!("session create failed: {err}");
                self.log_node_status_err(
                    &input.node_id,
                    self.store
                        .update_node_status(&input.node_id, NodeStatus::Failed, Some(err_msg.clone()), None)
                        .await,
                );
                self.emit_node_status(
                    &input.org_id,
                    &input.node_id,
                    &input.node_name,
                    NodeStatus::Failed,
                    Some(&err_msg),
                );
                return Err(anyhow!(err_msg).into());
            }
        };

        if let Err(err) =
            self.mcp.session_prompt(&input.org_id, &session.workspace_id, session.session_id(), &prompt).await
        {
            let err_msg = format!("session prompt failed: {err}");
            self.log_node_status_err(
                &input.node_id,
                self.store.update_node_status(&input.node_id, NodeStatus::Failed, Some(err_msg.clone()), None).await,
            );
            self.emit_node_status(&input.org_id, &input.node_id, &input.node_name, NodeStatus::Failed, Some(&err_msg));
            return Err(anyhow!(err_msg).into());
        }

        const MAX_CONSECUTIVE_FAILURES: usize = 10;
        let mut consecutive_failures = 0usize;

        // F042: in-activity elapsed-time deadline, below the 30-min Temporal
        // start_to_close, so a wedged session fails with a diagnostic well before
        // the hard timeout. 50%/90% warns mirror `wait_for_human_review`.
        let started_at = tokio::time::Instant::now();
        let deadline_secs = agent_task_max_duration_secs(input.config.as_ref());
        let warn_50_secs = deadline_secs / 2;
        let warn_90_secs = deadline_secs * 9 / 10;
        let mut warned_50 = false;
        let mut warned_90 = false;
        // Distinct unrecognized status strings already warned about, so a
        // protocol drift logs once rather than every 5s poll.
        let mut warned_statuses: std::collections::HashSet<String> = std::collections::HashSet::new();

        loop {
            let _ = ctx.record_heartbeat(()).await;
            tokio::select! {
                _ = ctx.cancelled() => return Err(ActivityError::cancelled()),
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }

            // Enforce the deadline before polling, so a session whose status
            // endpoint is also wedged still fails on time.
            let elapsed_secs = started_at.elapsed().as_secs();
            if elapsed_secs >= deadline_secs {
                let err_msg = format!("agent task exceeded max duration of {deadline_secs}s");
                self.log_node_status_err(
                    &input.node_id,
                    self.store
                        .update_node_status(&input.node_id, NodeStatus::Failed, Some(err_msg.clone()), None)
                        .await,
                );
                self.emit_node_status(
                    &input.org_id,
                    &input.node_id,
                    &input.node_name,
                    NodeStatus::Failed,
                    Some(&err_msg),
                );
                // Non-retryable: a plain `anyhow!(..).into()` is a *retryable*
                // Temporal application failure, and `standard_activity_options`
                // allows 3 attempts — so the deadline would otherwise restart the
                // activity (new MCP session + re-prompt) up to 3×, stretching the
                // cap and duplicating agent work. Fail closed instead.
                return Err(non_retryable_activity_error("AgentTaskDeadlineExceeded", anyhow!(err_msg)));
            }
            if !warned_50 && elapsed_secs >= warn_50_secs {
                warned_50 = true;
                tracing::warn!(node_id = %input.node_id, elapsed_secs, deadline_secs, "agent task running — 50% of max duration elapsed");
                self.emit_node_status(
                    &input.org_id,
                    &input.node_id,
                    &input.node_name,
                    NodeStatus::Running,
                    Some("agent_task_deadline_50pct"),
                );
            }
            if !warned_90 && elapsed_secs >= warn_90_secs {
                warned_90 = true;
                tracing::warn!(node_id = %input.node_id, elapsed_secs, deadline_secs, "agent task running — 90% of max duration elapsed");
                self.emit_node_status(
                    &input.org_id,
                    &input.node_id,
                    &input.node_name,
                    NodeStatus::Running,
                    Some("agent_task_deadline_90pct"),
                );
            }

            let status = match self.mcp.session_status(&input.org_id, &session.workspace_id, session.session_id()).await
            {
                Ok(status) => {
                    consecutive_failures = 0;
                    status
                }
                Err(err) => {
                    consecutive_failures += 1;
                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        let err_msg =
                            format!("status polling failed {} consecutive times: {}", consecutive_failures, err);
                        self.log_node_status_err(
                            &input.node_id,
                            self.store
                                .update_node_status(&input.node_id, NodeStatus::Failed, Some(err_msg.clone()), None)
                                .await,
                        );
                        self.emit_node_status(
                            &input.org_id,
                            &input.node_id,
                            &input.node_name,
                            NodeStatus::Failed,
                            Some(&err_msg),
                        );
                        return Err(anyhow!(err_msg).into());
                    }
                    continue;
                }
            };

            match status.status.as_str() {
                "idle" | "completed" => {
                    let output = json!({
                        "sessionId": session.session_id(),
                        "status": status.status,
                    });
                    self.log_node_status_err(
                        &input.node_id,
                        self.store
                            .update_node_status(&input.node_id, NodeStatus::Completed, None, Some(output.clone()))
                            .await,
                    );
                    self.emit_node_status(&input.org_id, &input.node_id, &input.node_name, NodeStatus::Completed, None);
                    return Ok(ExecuteAgentTaskOutput {
                        session_id: session.session_id().to_string(),
                        status: status.status,
                    });
                }
                "error" | "failed" => {
                    let err_msg = format!("agent session ended with status: {}", status.status);
                    self.log_node_status_err(
                        &input.node_id,
                        self.store
                            .update_node_status(&input.node_id, NodeStatus::Failed, Some(err_msg.clone()), None)
                            .await,
                    );
                    self.emit_node_status(
                        &input.org_id,
                        &input.node_id,
                        &input.node_name,
                        NodeStatus::Failed,
                        Some(&err_msg),
                    );
                    return Err(anyhow!(err_msg).into());
                }
                other => {
                    // Recognized in-progress states keep polling silently. A
                    // status outside both the terminal and in-progress sets is a
                    // possible `session_status` protocol drift — warn once per
                    // distinct string so it surfaces without spamming each tick.
                    if !KNOWN_IN_PROGRESS_STATUSES.contains(&other) && warned_statuses.insert(other.to_string()) {
                        tracing::warn!(
                            node_id = %input.node_id,
                            session_status = %other,
                            "agent session reported an unrecognized status; continuing to poll (possible session_status protocol drift)"
                        );
                    }
                }
            }
        }
    }

    #[activity(name = "WaitForHumanReview")]
    pub async fn wait_for_human_review(
        self: Arc<Self>,
        ctx: ActivityContext,
        input: HumanReviewInput,
    ) -> Result<HumanReviewOutput, ActivityError> {
        self.log_node_status_err(
            &input.node_id,
            self.store.update_node_status(&input.node_id, NodeStatus::Running, None, None).await,
        );
        self.emit_node_status(&input.org_id, &input.node_id, &input.node_name, NodeStatus::Running, None);

        let started_at = tokio::time::Instant::now();
        // Defensive floor: callers derive the deadline via `human_review_timeout_secs`
        // (min 60s), but the activity must not fire both escalations on tick 1 if a
        // 0 ever reaches the struct boundary.
        let deadline_secs = input.timeout_secs.max(60);
        let warn_50_secs = deadline_secs / 2;
        let warn_90_secs = deadline_secs * 9 / 10;
        let mut warned_50 = false;
        let mut warned_90 = false;

        let mut ticker = tokio::time::interval(Duration::from_secs(10));
        loop {
            tokio::select! {
                _ = ctx.cancelled() => return Err(ActivityError::cancelled()),
                _ = ticker.tick() => {
                    let _ = ctx.record_heartbeat(()).await;

                    let elapsed_secs = started_at.elapsed().as_secs();

                    if !warned_50 && elapsed_secs >= warn_50_secs {
                        warned_50 = true;
                        tracing::warn!(
                            node_id = %input.node_id,
                            elapsed_secs,
                            deadline_secs,
                            "human review pending — 50% of deadline elapsed"
                        );
                        self.emit_node_status(
                            &input.org_id,
                            &input.node_id,
                            &input.node_name,
                            NodeStatus::Running,
                            Some("review_deadline_50pct"),
                        );
                    }

                    if !warned_90 && elapsed_secs >= warn_90_secs {
                        warned_90 = true;
                        tracing::warn!(
                            node_id = %input.node_id,
                            elapsed_secs,
                            deadline_secs,
                            "human review pending — 90% of deadline elapsed"
                        );
                        self.emit_node_status(
                            &input.org_id,
                            &input.node_id,
                            &input.node_name,
                            NodeStatus::Running,
                            Some("review_deadline_90pct"),
                        );
                    }
                }
            }
        }
    }

    #[activity(name = "EvaluateGate")]
    pub async fn evaluate_gate(
        self: Arc<Self>,
        _ctx: ActivityContext,
        input: GateInput,
    ) -> Result<GateOutput, ActivityError> {
        self.log_node_status_err(
            &input.node_id,
            self.store.update_node_status(&input.node_id, NodeStatus::Running, None, None).await,
        );
        self.emit_node_status(&input.org_id, &input.node_id, &input.node_name, NodeStatus::Running, None);

        let condition = config_string(input.config.as_ref(), "condition");
        let evaluation = evaluate_gate_condition(condition.as_deref(), &input.dep_results);
        if evaluation.passed {
            let output = json!({
                "passed": true,
                "reason": evaluation.reason,
            });
            self.log_node_status_err(
                &input.node_id,
                self.store.update_node_status(&input.node_id, NodeStatus::Completed, None, Some(output.clone())).await,
            );
            self.emit_node_status(&input.org_id, &input.node_id, &input.node_name, NodeStatus::Completed, None);
            return Ok(GateOutput { passed: true, reason: evaluation.reason });
        }

        if evaluation.invalid {
            self.log_node_status_err(
                &input.node_id,
                self.store
                    .update_node_status(&input.node_id, NodeStatus::Failed, Some(evaluation.reason.clone()), None)
                    .await,
            );
            self.emit_node_status(
                &input.org_id,
                &input.node_id,
                &input.node_name,
                NodeStatus::Failed,
                Some(&evaluation.reason),
            );
            return Err(anyhow!(evaluation.reason).into());
        }

        self.log_node_status_err(
            &input.node_id,
            self.store
                .update_node_status(&input.node_id, NodeStatus::Failed, Some(evaluation.reason.clone()), None)
                .await,
        );
        self.emit_node_status(
            &input.org_id,
            &input.node_id,
            &input.node_name,
            NodeStatus::Failed,
            Some(&evaluation.reason),
        );
        Ok(GateOutput { passed: false, reason: evaluation.reason })
    }

    #[activity(name = "FinalizeWorkflowStatus")]
    pub async fn finalize_workflow_status(
        self: Arc<Self>,
        _ctx: ActivityContext,
        input: FinalizeWorkflowStatusInput,
    ) -> Result<(), ActivityError> {
        self.store
            .update_status(&input.workflow_id, &input.org_id, input.status)
            .await
            .map_err(|err| ActivityError::from(anyhow!(err.to_string())))?;
        self.emit_workflow_status(&input.org_id, &input.workflow_id, input.status);
        Ok(())
    }
}

impl WorkflowActivities {
    fn log_node_status_err(&self, node_id: &str, result: super::errors::Result<()>) {
        if let Err(err) = result {
            tracing::error!(%node_id, error = %err, "failed to persist workflow node status");
        }
    }

    fn emit_node_status(&self, org_id: &str, node_id: &str, node_name: &str, status: NodeStatus, detail: Option<&str>) {
        if let Some(b) = &self.broadcaster {
            b.broadcast(Event {
                kind: "workflow:node_status".to_string(),
                org_id: org_id.to_string(),
                payload: json!({
                    "nodeId": node_id,
                    "nodeName": node_name,
                    "status": status,
                    "detail": detail,
                }),
            });
        }
    }

    fn emit_workflow_status(&self, org_id: &str, workflow_id: &str, status: WorkflowStatus) {
        if let Some(b) = &self.broadcaster {
            b.broadcast(Event {
                kind: "workflow:status".to_string(),
                org_id: org_id.to_string(),
                payload: json!({
                    "workflowId": workflow_id,
                    "status": status,
                }),
            });
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteAgentTaskInput {
    pub node_id: String,
    pub node_name: String,
    pub config: Option<Value>,
    pub org_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteAgentTaskOutput {
    pub session_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanReviewInput {
    pub node_id: String,
    pub node_name: String,
    pub config: Option<Value>,
    pub timeout_secs: u64,
    pub org_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanReviewOutput {
    pub decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateInput {
    pub node_id: String,
    pub node_name: String,
    pub config: Option<Value>,
    pub dep_results: HashMap<String, NodeStatus>,
    pub org_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateOutput {
    pub passed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalizeWorkflowStatusInput {
    pub workflow_id: String,
    pub org_id: String,
    pub status: WorkflowStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateEvaluation {
    pub passed: bool,
    pub reason: String,
    pub invalid: bool,
}

pub fn evaluate_gate_condition(condition: Option<&str>, dep_results: &HashMap<String, NodeStatus>) -> GateEvaluation {
    match condition.unwrap_or("all_success") {
        "all_success" => {
            for (name, status) in dep_results {
                if *status != NodeStatus::Completed {
                    return GateEvaluation {
                        passed: false,
                        reason: format!(
                            "dependency \"{}\" has status \"{}\" (expected completed)",
                            name,
                            status.as_str()
                        ),
                        invalid: false,
                    };
                }
            }
            GateEvaluation {
                passed: true,
                reason: "all dependencies completed successfully".to_string(),
                invalid: false,
            }
        }
        "any_success" => {
            for (name, status) in dep_results {
                if *status == NodeStatus::Completed {
                    return GateEvaluation {
                        passed: true,
                        reason: format!("dependency \"{}\" completed successfully", name),
                        invalid: false,
                    };
                }
            }
            GateEvaluation {
                passed: false,
                reason: "no dependencies completed successfully".to_string(),
                invalid: false,
            }
        }
        other => {
            GateEvaluation { passed: false, reason: format!("unknown gate condition: \"{}\"", other), invalid: true }
        }
    }
}

fn config_string(config: Option<&Value>, key: &str) -> Option<String> {
    config.and_then(|config| config.get(key)).and_then(Value::as_str).map(ToString::to_string)
}

/// Default in-activity deadline for an agent task: 25 minutes, deliberately
/// below the 30-minute Temporal `start_to_close` so a wedged session fails with
/// a clear diagnostic before the hard timeout burns the whole attempt (F042).
const DEFAULT_AGENT_TASK_MAX_DURATION_SECS: u64 = 25 * 60;

// The in-activity default must stay below the 30-min Temporal start_to_close
// (temporal.rs `standard_activity_options`) so this guard fires first.
const _: () = assert!(DEFAULT_AGENT_TASK_MAX_DURATION_SECS < 30 * 60);

/// Session statuses that mean "still working, keep polling". `idle`/`completed`
/// (success) and `error`/`failed` (failure) are terminal and handled
/// explicitly; anything outside both sets is treated as possible protocol drift
/// and warned about once. Kept broad to match the sidecar/MCP status vocabulary.
const KNOWN_IN_PROGRESS_STATUSES: &[&str] = &["running", "pending", "starting", "working", "active", "busy", "created"];

/// Build a NON-retryable Temporal activity error. The default `From<anyhow>`
/// conversion produces a *retryable* application failure, which under the
/// 3-attempt `standard_activity_options` retry policy would re-run the whole
/// activity. Use this for terminal conditions (e.g. the agent-task deadline)
/// that must fail closed without another attempt.
fn non_retryable_activity_error(type_name: &'static str, source: anyhow::Error) -> ActivityError {
    ActivityError::application(
        ApplicationFailure::builder(source).type_name(type_name.to_owned()).non_retryable(true).build(),
    )
}

/// Resolve the agent-task in-activity deadline (seconds) from the node config
/// key `agentTaskMaxDurationSecs`, clamped to a 60s floor, defaulting to
/// [`DEFAULT_AGENT_TASK_MAX_DURATION_SECS`]. Mirrors `human_review_timeout_secs`.
fn agent_task_max_duration_secs(config: Option<&Value>) -> u64 {
    config
        .and_then(|config| config.get("agentTaskMaxDurationSecs"))
        .and_then(Value::as_u64)
        .map(|secs| secs.max(60))
        .unwrap_or(DEFAULT_AGENT_TASK_MAX_DURATION_SECS)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;

    use crate::mcp::client::{CreateSessionArgs, CreateSessionResult, OutboundMcp, SessionStatusResult};
    use crate::realtime::Broadcaster;

    use super::super::repository::MemoryStore;
    use super::*;

    struct SuccessfulMcp {
        session_id: String,
    }

    #[async_trait]
    impl OutboundMcp for SuccessfulMcp {
        async fn session_create(&self, args: CreateSessionArgs) -> anyhow::Result<CreateSessionResult> {
            Ok(CreateSessionResult {
                agent_id: self.session_id.clone(),
                status: "created".to_string(),
                name: args.name.unwrap_or_else(|| self.session_id.clone()),
                workspace_id: "test-workspace".to_string(),
            })
        }

        async fn session_prompt(
            &self,
            _org_id: &str,
            _workspace_id: &str,
            _agent_id: &str,
            _prompt: &str,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn session_destroy(&self, _org_id: &str, _workspace_id: &str, _agent_id: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn session_status(
            &self,
            _org_id: &str,
            _workspace_id: &str,
            _agent_id: &str,
        ) -> anyhow::Result<SessionStatusResult> {
            Ok(SessionStatusResult { agent_id: self.session_id.clone(), status: "idle".to_string() })
        }
    }

    fn make_activities(broadcaster: Option<Arc<Broadcaster>>) -> WorkflowActivities {
        let mcp: Arc<dyn OutboundMcp> = Arc::new(SuccessfulMcp { session_id: "test-session".to_string() });
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        WorkflowActivities::new(mcp, store, broadcaster)
    }

    #[test]
    fn emit_node_status_none_broadcaster_no_panic() {
        let activities = make_activities(None);
        // Must not panic when broadcaster is None.
        activities.emit_node_status("org-1", "node-1", "Node A", NodeStatus::Running, None);
        activities.emit_node_status("org-1", "node-1", "Node A", NodeStatus::Failed, Some("err"));
    }

    #[test]
    fn emit_node_status_with_broadcaster_sends_event() {
        let broadcaster = Arc::new(Broadcaster::new());
        let (_client_id, mut rx) = broadcaster.subscribe("org-1");
        let activities = make_activities(Some(broadcaster));

        activities.emit_node_status("org-1", "node-42", "My Node", NodeStatus::Completed, None);

        let event = rx.try_recv().expect("expected an event on the channel");
        assert_eq!(event.kind, "workflow:node_status");
        assert_eq!(event.org_id, "org-1");
        assert_eq!(event.payload["nodeId"], "node-42");
        assert_eq!(event.payload["nodeName"], "My Node");
        assert_eq!(event.payload["status"], "completed");
    }

    #[test]
    fn emit_node_status_detail_is_included_when_some() {
        let broadcaster = Arc::new(Broadcaster::new());
        let (_client_id, mut rx) = broadcaster.subscribe("org-1");
        let activities = make_activities(Some(broadcaster));

        activities.emit_node_status("org-1", "node-1", "Gate", NodeStatus::Failed, Some("dep failed"));

        let event = rx.try_recv().expect("expected an event");
        assert_eq!(event.payload["detail"], "dep failed");
    }

    #[test]
    fn emit_node_status_not_delivered_to_other_org() {
        let broadcaster = Arc::new(Broadcaster::new());
        let (_client_id, mut rx_other) = broadcaster.subscribe("org-other");
        let activities = make_activities(Some(broadcaster));

        activities.emit_node_status("org-1", "node-1", "Node", NodeStatus::Running, None);

        assert!(rx_other.try_recv().is_err(), "event should not be delivered to a different org");
    }

    #[test]
    fn emit_workflow_status_none_broadcaster_no_panic() {
        let activities = make_activities(None);
        activities.emit_workflow_status("org-1", "wf-1", WorkflowStatus::Completed);
    }

    #[test]
    fn emit_workflow_status_with_broadcaster_sends_event() {
        let broadcaster = Arc::new(Broadcaster::new());
        let (_client_id, mut rx) = broadcaster.subscribe("org-1");
        let activities = make_activities(Some(broadcaster));

        activities.emit_workflow_status("org-1", "wf-99", WorkflowStatus::Failed);

        let event = rx.try_recv().expect("expected a workflow:status event");
        assert_eq!(event.kind, "workflow:status");
        assert_eq!(event.org_id, "org-1");
        assert_eq!(event.payload["workflowId"], "wf-99");
        assert_eq!(event.payload["status"], "failed");
    }

    #[test]
    fn evaluate_gate_condition_all_success_passes() {
        let mut dep_results = HashMap::new();
        dep_results.insert("a".to_string(), NodeStatus::Completed);
        dep_results.insert("b".to_string(), NodeStatus::Completed);
        let eval = evaluate_gate_condition(Some("all_success"), &dep_results);
        assert!(eval.passed);
        assert!(!eval.invalid);
    }

    #[test]
    fn evaluate_gate_condition_any_success_passes() {
        let mut dep_results = HashMap::new();
        dep_results.insert("a".to_string(), NodeStatus::Failed);
        dep_results.insert("b".to_string(), NodeStatus::Completed);
        let eval = evaluate_gate_condition(Some("any_success"), &dep_results);
        assert!(eval.passed);
    }

    #[test]
    fn evaluate_gate_condition_unknown_is_invalid() {
        let dep_results = HashMap::new();
        let eval = evaluate_gate_condition(Some("unknown_condition"), &dep_results);
        assert!(!eval.passed);
        assert!(eval.invalid);
    }

    #[test]
    fn agent_task_max_duration_defaults_when_unset() {
        assert_eq!(agent_task_max_duration_secs(None), DEFAULT_AGENT_TASK_MAX_DURATION_SECS);
        let empty = serde_json::json!({});
        assert_eq!(agent_task_max_duration_secs(Some(&empty)), DEFAULT_AGENT_TASK_MAX_DURATION_SECS);
    }

    #[test]
    fn agent_task_max_duration_reads_configured_value() {
        let cfg = serde_json::json!({ "agentTaskMaxDurationSecs": 600 });
        assert_eq!(agent_task_max_duration_secs(Some(&cfg)), 600);
    }

    #[test]
    fn agent_task_max_duration_clamps_to_floor() {
        let cfg = serde_json::json!({ "agentTaskMaxDurationSecs": 5 });
        assert_eq!(agent_task_max_duration_secs(Some(&cfg)), 60);
    }

    #[test]
    fn agent_task_max_duration_ignores_non_numeric() {
        let cfg = serde_json::json!({ "agentTaskMaxDurationSecs": "soon" });
        assert_eq!(agent_task_max_duration_secs(Some(&cfg)), DEFAULT_AGENT_TASK_MAX_DURATION_SECS);
    }

    #[test]
    fn deadline_error_is_non_retryable() {
        let err = non_retryable_activity_error("AgentTaskDeadlineExceeded", anyhow!("exceeded max duration"));
        match err {
            ActivityError::Application(failure) => {
                assert!(failure.is_non_retryable(), "deadline failure must not be retried by Temporal");
                assert_eq!(failure.type_name(), Some("AgentTaskDeadlineExceeded"));
            }
            other => panic!("expected a non-retryable Application failure, got {other:?}"),
        }
    }

    #[test]
    fn in_progress_set_excludes_terminal_and_drift_statuses() {
        for s in ["running", "pending", "starting", "working", "active", "busy", "created"] {
            assert!(KNOWN_IN_PROGRESS_STATUSES.contains(&s), "{s} should be treated as in-progress");
        }
        // Terminal states are handled explicitly (not in the in-progress set), and
        // an unknown string is drift — both fall outside, so a drift string warns.
        for s in ["idle", "completed", "error", "failed", "totally_unknown"] {
            assert!(!KNOWN_IN_PROGRESS_STATUSES.contains(&s), "{s} must not be in the in-progress set");
        }
    }
}
