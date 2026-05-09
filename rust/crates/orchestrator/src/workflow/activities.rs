use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use temporalio_macros::activities;
use temporalio_sdk::activities::{ActivityContext, ActivityError};

use crate::mcp::client::{CreateSessionArgs, OutboundMcp};

use super::model::{NodeStatus, WorkflowStatus};
use super::store::Store;

#[derive(Clone)]
pub struct WorkflowActivities {
    mcp: Arc<dyn OutboundMcp>,
    store: Arc<dyn Store>,
}

impl WorkflowActivities {
    pub fn new(mcp: Arc<dyn OutboundMcp>, store: Arc<dyn Store>) -> Self {
        Self { mcp, store }
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

        let project_id = config_string(input.config.as_ref(), "projectId").unwrap_or_default();
        let cli_tool = config_string(input.config.as_ref(), "cliTool").unwrap_or_else(|| "claude".to_string());
        let prompt = config_string(input.config.as_ref(), "prompt").unwrap_or_else(|| input.node_name.clone());

        let session = match self
            .mcp
            .session_create(CreateSessionArgs { project_id, cli_tool, name: Some(input.node_name.clone()) })
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
                return Err(anyhow!(err_msg).into());
            }
        };

        if let Err(err) = self.mcp.session_prompt(session.session_id(), &prompt).await {
            let err_msg = format!("session prompt failed: {err}");
            self.log_node_status_err(
                &input.node_id,
                self.store.update_node_status(&input.node_id, NodeStatus::Failed, Some(err_msg.clone()), None).await,
            );
            return Err(anyhow!(err_msg).into());
        }

        const MAX_CONSECUTIVE_FAILURES: usize = 10;
        let mut consecutive_failures = 0usize;
        loop {
            ctx.record_heartbeat(Vec::new());
            tokio::select! {
                _ = ctx.cancelled() => return Err(ActivityError::cancelled()),
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }

            let status = match self.mcp.session_status(session.session_id()).await {
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
                    return Err(anyhow!(err_msg).into());
                }
                _ => {}
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

        let mut ticker = tokio::time::interval(Duration::from_secs(10));
        loop {
            tokio::select! {
                _ = ctx.cancelled() => return Err(ActivityError::cancelled()),
                _ = ticker.tick() => ctx.record_heartbeat(Vec::new()),
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
            return Ok(GateOutput { passed: true, reason: evaluation.reason });
        }

        if evaluation.invalid {
            self.log_node_status_err(
                &input.node_id,
                self.store
                    .update_node_status(&input.node_id, NodeStatus::Failed, Some(evaluation.reason.clone()), None)
                    .await,
            );
            return Err(anyhow!(evaluation.reason).into());
        }

        self.log_node_status_err(
            &input.node_id,
            self.store
                .update_node_status(&input.node_id, NodeStatus::Failed, Some(evaluation.reason.clone()), None)
                .await,
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
            .map_err(|err| anyhow!(err.to_string()).into())
    }
}

impl WorkflowActivities {
    fn log_node_status_err(&self, node_id: &str, result: super::errors::Result<()>) {
        if let Err(err) = result {
            tracing::error!(%node_id, error = %err, "failed to persist workflow node status");
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
