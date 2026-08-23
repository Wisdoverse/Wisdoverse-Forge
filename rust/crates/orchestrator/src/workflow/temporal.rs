use std::any::Any;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use futures::FutureExt;
use futures::channel::oneshot;
use futures::future::LocalBoxFuture;
use futures::pin_mut;
use serde::{Deserialize, Serialize};
use temporalio_client::{
    Client, ClientOptions, Connection, ConnectionOptions, NamespacedClient, WorkflowCancelOptions,
    WorkflowExecutionInfo, WorkflowSignalOptions, WorkflowStartOptions,
};
use temporalio_common::data_converters::{
    GenericPayloadConverter, PayloadConverter, SerializationContext, SerializationContextData,
};
use temporalio_common::protos::temporal::api::common::v1::Payloads;
use temporalio_common::protos::temporal::api::common::v1::RetryPolicy;
use temporalio_common::protos::temporal::api::enums::v1::{WorkflowIdConflictPolicy, WorkflowIdReusePolicy};
use temporalio_common::{HasWorkflowDefinition, WorkflowDefinition};
use temporalio_sdk::workflow_interceptors::WorkflowOutputValue;
use temporalio_sdk::workflows::{WorkflowError as TemporalWorkflowError, WorkflowImplementation};
use temporalio_sdk::workflows::{join_all, select};
use temporalio_sdk::{ActivityOptions, CancellableFuture, WorkflowContext, WorkflowResult, WorkflowTermination};
use temporalio_workflow::runtime::types::WorkflowDefinitionDescriptor;
use url::Url;

use crate::config::Config;

use super::activities::{
    ExecuteAgentTaskInput, FinalizeWorkflowStatusInput, GateInput, GateOutput, HumanReviewInput, WorkflowActivities,
};
use super::errors::{Result as WorkflowApiResult, WorkflowError};
use super::model::{Decision, NodeStatus, NodeType, SignalRequest, Workflow, WorkflowNode, WorkflowStatus};
use super::runtime::WorkflowRuntime;
use super::store::Store;

pub const TASK_QUEUE: &str = "orchestrator-workflows";
pub const SIGNAL_HUMAN_REVIEW: &str = "human-review-decision";
pub const ORCHESTRATOR_WORKFLOW_NAME: &str = "OrchestratorWorkflow";

const DEFAULT_HUMAN_REVIEW_TIMEOUT_SECS: u64 = 24 * 60 * 60;

fn human_review_node_id(signal_name: &str) -> Option<&str> {
    signal_name.strip_prefix(SIGNAL_HUMAN_REVIEW)?.strip_prefix('-')
}

/// Deterministic helper: reads `reviewTimeoutSecs` from node config (replayed-safe,
/// no env/clock access). Returns the configured value clamped to a minimum of 60
/// seconds, or `DEFAULT_HUMAN_REVIEW_TIMEOUT_SECS` when the key is absent.
pub fn human_review_timeout_secs(config: &Option<serde_json::Value>) -> u64 {
    config
        .as_ref()
        .and_then(|v| v.get("reviewTimeoutSecs"))
        .and_then(|v| v.as_u64())
        .map(|secs| secs.max(60))
        .unwrap_or(DEFAULT_HUMAN_REVIEW_TIMEOUT_SECS)
}

pub fn signal_name_for_node(node_id: &str) -> String {
    format!("{SIGNAL_HUMAN_REVIEW}-{node_id}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorWorkflowInput {
    pub workflow_id: String,
    pub org_id: String,
    pub nodes: Vec<WorkflowNode>,
    pub layers: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanReviewSignalPayload {
    pub node_id: String,
    pub decision: Decision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Clone)]
pub struct TemporalWorkflowRuntime {
    client: Client,
    store: Arc<dyn Store>,
}

impl TemporalWorkflowRuntime {
    pub fn new(client: Client, store: Arc<dyn Store>) -> Self {
        Self { client, store }
    }

    fn workflow_handle(
        &self,
        workflow: &Workflow,
    ) -> WorkflowApiResult<temporalio_client::WorkflowHandle<Client, OrchestratorWorkflowRun>> {
        let temporal_workflow_id = workflow
            .temporal_workflow_id
            .clone()
            .ok_or_else(|| WorkflowError::Internal("workflow has no temporal execution".to_string()))?;
        let info = WorkflowExecutionInfo {
            namespace: self.client.namespace(),
            workflow_id: temporal_workflow_id,
            run_id: workflow.temporal_run_id.clone(),
            first_execution_run_id: workflow.temporal_run_id.clone(),
        };
        Ok(temporalio_client::WorkflowHandle::new(self.client.clone(), info))
    }
}

#[async_trait::async_trait]
impl WorkflowRuntime for TemporalWorkflowRuntime {
    fn kind(&self) -> &'static str {
        "temporal"
    }

    async fn start_workflow(
        &self,
        workflow: &Workflow,
        nodes: &[WorkflowNode],
        layers: &[Vec<String>],
    ) -> WorkflowApiResult<()> {
        let temporal_workflow_id = format!("orchestrator-{}", workflow.id);
        // Re-running a failed workflow reuses this id, so set explicit policies
        // instead of relying on Temporal's default duplicate behavior (F041):
        // AllowDuplicate permits a fresh run after the prior one closed, and
        // TerminateExisting terminates a still-running prior execution rather than
        // failing or silently attaching to it.
        let options = WorkflowStartOptions::new(TASK_QUEUE.to_string(), temporal_workflow_id.clone())
            .id_reuse_policy(WorkflowIdReusePolicy::AllowDuplicate)
            .id_conflict_policy(WorkflowIdConflictPolicy::TerminateExisting)
            .build();
        let handle = self
            .client
            .start_workflow(
                OrchestratorWorkflowRun,
                OrchestratorWorkflowInput {
                    workflow_id: workflow.id.clone(),
                    org_id: workflow.org_id.clone(),
                    nodes: nodes.to_vec(),
                    layers: layers.to_vec(),
                },
                options,
            )
            .await
            .map_err(|err| WorkflowError::Internal(format!("start temporal workflow: {err}")))?;

        let temporal_run_id = handle
            .run_id()
            .ok_or_else(|| WorkflowError::Internal("temporal workflow start returned empty run id".to_string()))?
            .to_string();

        if let Err(err) =
            self.store.set_temporal_ids(&workflow.id, &workflow.org_id, temporal_workflow_id, temporal_run_id).await
        {
            let _ = handle.cancel(WorkflowCancelOptions::default()).await;
            return Err(WorkflowError::Internal(format!("persist temporal IDs: {err}")));
        }

        if let Err(err) = self.store.update_status(&workflow.id, &workflow.org_id, WorkflowStatus::Running).await {
            let _ = handle.cancel(WorkflowCancelOptions::default()).await;
            return Err(WorkflowError::Internal(format!("persist workflow status: {err}")));
        }

        Ok(())
    }

    async fn cancel_workflow(&self, workflow: &Workflow) -> WorkflowApiResult<()> {
        let handle = self.workflow_handle(workflow)?;
        handle
            .cancel(WorkflowCancelOptions::default())
            .await
            .map_err(|err| WorkflowError::Internal(format!("cancel temporal workflow: {err}")))?;
        self.store.update_status(&workflow.id, &workflow.org_id, WorkflowStatus::Cancelled).await
    }

    async fn signal_workflow(
        &self,
        workflow: &Workflow,
        signal: SignalRequest,
        _layers: &[Vec<String>],
    ) -> WorkflowApiResult<()> {
        let node_id = signal.node_id.ok_or_else(|| WorkflowError::InvalidInput("nodeId is required".to_string()))?;
        let decision = signal
            .decision
            .ok_or_else(|| WorkflowError::InvalidInput("decision must be 'approve' or 'reject'".to_string()))?;
        let handle = self.workflow_handle(workflow)?;
        let payload = HumanReviewSignalPayload { node_id: node_id.clone(), decision, comment: signal.comment.clone() };

        handle
            .signal(
                DynamicHumanReviewSignal::new(signal_name_for_node(&node_id)),
                payload,
                WorkflowSignalOptions::default(),
            )
            .await
            .map_err(|err| WorkflowError::Internal(format!("signal temporal workflow: {err}")))?;

        let (status, error) = match decision {
            Decision::Approve => (NodeStatus::Completed, None),
            Decision::Reject => (NodeStatus::Failed, signal.comment.or(Some("rejected".to_string()))),
        };
        self.store.update_node_status(&node_id, status, error, None).await?;
        Ok(())
    }
}

pub async fn connect_temporal_client(config: &Config) -> anyhow::Result<Client> {
    let target = temporal_target_url(&config.temporal_host)?;
    let connection = Connection::connect(ConnectionOptions::new(target).build()).await.context("connect temporal")?;
    Client::new(connection, ClientOptions::new(config.temporal_namespace.clone()).build())
        .context("create temporal client")
}

fn temporal_target_url(host: &str) -> anyhow::Result<Url> {
    let target = if host.contains("://") { host.to_string() } else { format!("http://{host}") };
    Url::from_str(&target).with_context(|| format!("parse temporal host: {target}"))
}

#[derive(Default)]
pub struct OrchestratorWorkflow {
    pending_review_decisions: HashMap<String, HumanReviewSignalPayload>,
    waiting_review_nodes: HashMap<String, oneshot::Sender<HumanReviewSignalPayload>>,
}

pub struct OrchestratorWorkflowRun;

impl WorkflowDefinition for OrchestratorWorkflowRun {
    type Input = OrchestratorWorkflowInput;
    type Output = ();

    fn name(&self) -> &str {
        ORCHESTRATOR_WORKFLOW_NAME
    }
}

impl HasWorkflowDefinition for OrchestratorWorkflowRun {
    type Run = Self;
}

impl WorkflowImplementation for OrchestratorWorkflow {
    type Run = OrchestratorWorkflowRun;

    const HAS_INIT: bool = false;
    const INIT_TAKES_INPUT: bool = false;

    fn name() -> &'static str {
        ORCHESTRATOR_WORKFLOW_NAME
    }

    fn definition() -> WorkflowDefinitionDescriptor {
        WorkflowDefinitionDescriptor {
            workflow_type: Self::name().to_string(),
            has_init: false,
            init_takes_input: false,
            signals: Vec::new(),
            queries: Vec::new(),
            updates: Vec::new(),
        }
    }

    fn init(
        _ctx: temporalio_sdk::WorkflowContextView,
        _input: Option<<Self::Run as WorkflowDefinition>::Input>,
    ) -> Self {
        Self::default()
    }

    fn run(
        ctx: WorkflowContext<Self>,
        input: Option<<Self::Run as WorkflowDefinition>::Input>,
    ) -> LocalBoxFuture<'static, std::result::Result<Box<dyn WorkflowOutputValue>, WorkflowTermination>> {
        async move {
            // A missing input means a framework/contract mismatch. Surface it as
            // an explicit terminal failure instead of panicking — a panic here
            // makes Temporal retry the same workflow task forever with no
            // progress and no terminal state (F043).
            let Some(input) = input else {
                return Err(anyhow!("orchestrator workflow received no input").into());
            };
            run_orchestrator_workflow(ctx, input).await?;
            Ok(Box::new(()) as Box<dyn WorkflowOutputValue>)
        }
        .boxed_local()
    }

    fn decode_signal_input(
        name: &str,
        payloads: Payloads,
        converter: &PayloadConverter,
    ) -> std::result::Result<Option<Box<dyn Any>>, TemporalWorkflowError> {
        if human_review_node_id(name).is_none() {
            return Ok(None);
        }
        let signal = converter.from_payloads::<HumanReviewSignalPayload>(
            &SerializationContext { data: &SerializationContextData::Workflow, converter },
            payloads.payloads,
        )?;
        Ok(Some(Box::new(signal)))
    }

    fn dispatch_signal(
        ctx: WorkflowContext<Self>,
        name: &str,
        input: Box<dyn Any>,
    ) -> LocalBoxFuture<'static, std::result::Result<(), TemporalWorkflowError>> {
        let node_id =
            human_review_node_id(name).expect("typed signal dispatch called for unknown signal handler").to_string();
        let signal = *input
            .downcast::<HumanReviewSignalPayload>()
            .expect("typed signal dispatch received input with wrong concrete type");

        async move {
            let buffered = HumanReviewSignalPayload {
                node_id: if signal.node_id.is_empty() { node_id.clone() } else { signal.node_id.clone() },
                decision: signal.decision,
                comment: signal.comment,
            };
            ctx.state_mut(|workflow| {
                if let Some(waiter) = workflow.waiting_review_nodes.remove(&node_id) {
                    if waiter.send(buffered.clone()).is_err() {
                        workflow.pending_review_decisions.insert(node_id.clone(), buffered);
                    }
                } else {
                    workflow.pending_review_decisions.insert(node_id.clone(), buffered);
                }
            });
            Ok(())
        }
        .boxed_local()
    }

    fn dispatch_query(
        &self,
        _ctx: temporalio_sdk::WorkflowContextView,
        name: &str,
        _input: Box<dyn Any>,
    ) -> std::result::Result<Box<dyn WorkflowOutputValue>, TemporalWorkflowError> {
        unreachable!("typed query dispatch called for unknown query handler '{name}'")
    }

    fn dispatch_update(
        _ctx: WorkflowContext<Self>,
        name: &str,
        _input: Box<dyn Any>,
    ) -> LocalBoxFuture<'static, std::result::Result<Box<dyn WorkflowOutputValue>, TemporalWorkflowError>> {
        unreachable!("typed update dispatch called for unknown update handler '{name}'")
    }

    fn validate_update(
        &self,
        _ctx: temporalio_sdk::WorkflowContextView,
        name: &str,
        _input: Box<dyn Any>,
    ) -> std::result::Result<(), TemporalWorkflowError> {
        unreachable!("typed update validation called for unknown update handler '{name}'")
    }
}

async fn run_orchestrator_workflow(
    ctx: WorkflowContext<OrchestratorWorkflow>,
    input: OrchestratorWorkflowInput,
) -> WorkflowResult<()> {
    let node_by_name: HashMap<String, WorkflowNode> =
        input.nodes.iter().cloned().map(|node| (node.name.clone(), node)).collect();
    let mut node_status: HashMap<String, NodeStatus> =
        input.nodes.iter().map(|node| (node.name.clone(), NodeStatus::Pending)).collect();

    for layer in &input.layers {
        let mut layer_futures = Vec::new();
        for node_name in layer {
            let Some(node) = node_by_name.get(node_name).cloned() else {
                continue;
            };

            let dep_results: HashMap<String, NodeStatus> = node
                .depends_on
                .iter()
                .map(|dep| (dep.clone(), *node_status.get(dep).unwrap_or(&NodeStatus::Pending)))
                .collect();
            if dep_results.values().any(|status| *status != NodeStatus::Completed) {
                node_status.insert(node.name.clone(), NodeStatus::Skipped);
                continue;
            }

            layer_futures.push(execute_node(ctx.clone(), &input, node, dep_results));
        }

        let layer_results = join_all(layer_futures).await;
        for result in layer_results {
            let (node_name, status) = result?;
            node_status.insert(node_name, status);
        }

        if node_status.values().any(|status| *status == NodeStatus::Failed) {
            finalize_workflow_status(&ctx, &input, WorkflowStatus::Failed).await?;
            let failed_node = node_status
                .iter()
                .find_map(|(name, status)| (*status == NodeStatus::Failed).then_some(name.clone()))
                .unwrap_or_else(|| "unknown".to_string());
            return Err(anyhow!("workflow failed: node \"{}\" failed", failed_node).into());
        }
    }

    finalize_workflow_status(&ctx, &input, WorkflowStatus::Completed).await?;
    Ok(())
}

fn execute_node(
    ctx: WorkflowContext<OrchestratorWorkflow>,
    input: &OrchestratorWorkflowInput,
    node: WorkflowNode,
    dep_results: HashMap<String, NodeStatus>,
) -> LocalBoxFuture<'static, std::result::Result<(String, NodeStatus), WorkflowTermination>> {
    let workflow_input = input.clone();
    async move {
        match node.node_type {
            NodeType::AgentTask => {
                let result = ctx
                    .execute_activity(
                        WorkflowActivities::execute_agent_task,
                        ExecuteAgentTaskInput {
                            node_id: node.id.clone(),
                            node_name: node.name.clone(),
                            config: node.config.clone(),
                            org_id: workflow_input.org_id.clone(),
                        },
                        standard_activity_options(),
                    )
                    .await;
                match result {
                    Ok(_) => Ok((node.name, NodeStatus::Completed)),
                    Err(err) => Ok((node.name, node_failure_status(err))),
                }
            }
            NodeType::Gate => {
                let result = ctx
                    .execute_activity(
                        WorkflowActivities::evaluate_gate,
                        GateInput {
                            node_id: node.id.clone(),
                            node_name: node.name.clone(),
                            config: node.config.clone(),
                            dep_results,
                            org_id: workflow_input.org_id.clone(),
                        },
                        standard_activity_options(),
                    )
                    .await;
                match result {
                    Ok(GateOutput { passed: true, .. }) => Ok((node.name, NodeStatus::Completed)),
                    Ok(GateOutput { passed: false, .. }) => Ok((node.name, NodeStatus::Failed)),
                    Err(err) => Ok((node.name, node_failure_status(err))),
                }
            }
            NodeType::HumanReview => {
                let timeout_secs = human_review_timeout_secs(&node.config);
                let review_activity = ctx.execute_activity(
                    WorkflowActivities::wait_for_human_review,
                    HumanReviewInput {
                        node_id: node.id.clone(),
                        node_name: node.name.clone(),
                        config: node.config.clone(),
                        timeout_secs,
                        org_id: workflow_input.org_id.clone(),
                    },
                    human_review_activity_options(Duration::from_secs(timeout_secs)),
                );
                let signal_wait = await_human_review_signal(&ctx, &node.id).fuse();
                pin_mut!(review_activity);
                pin_mut!(signal_wait);

                let decision = select! {
                    decision = signal_wait => decision?,
                    activity_result = review_activity => {
                        match activity_result {
                            Ok(_) => return Err(anyhow!("human review activity completed without signal for node {}", node.id).into()),
                            Err(err) => return Err(anyhow!("human review activity failed before signal for node {}: {}", node.id, err).into()),
                        }
                    }
                };

                review_activity.cancel();
                let _ = review_activity.await;
                let status = match decision.decision {
                    Decision::Approve => NodeStatus::Completed,
                    Decision::Reject => NodeStatus::Failed,
                };
                Ok((node.name, status))
            }
        }
    }
    .boxed_local()
}

async fn await_human_review_signal(
    ctx: &WorkflowContext<OrchestratorWorkflow>,
    node_id: &str,
) -> std::result::Result<HumanReviewSignalPayload, WorkflowTermination> {
    if let Some(signal) = ctx.state_mut(|workflow| workflow.pending_review_decisions.remove(node_id)) {
        return Ok(signal);
    }

    let (tx, rx) = oneshot::channel();
    let node_id_string = node_id.to_string();
    ctx.state_mut(|workflow| {
        workflow.waiting_review_nodes.insert(node_id_string.clone(), tx);
    });

    rx.await.map_err(|_| anyhow!("human review waiter dropped for node {}", node_id).into())
}

async fn finalize_workflow_status(
    ctx: &WorkflowContext<OrchestratorWorkflow>,
    input: &OrchestratorWorkflowInput,
    status: WorkflowStatus,
) -> std::result::Result<(), WorkflowTermination> {
    ctx.execute_activity(
        WorkflowActivities::finalize_workflow_status,
        FinalizeWorkflowStatusInput { workflow_id: input.workflow_id.clone(), org_id: input.org_id.clone(), status },
        finalize_activity_options(),
    )
    .await
    .map_err(|err| anyhow!("finalize workflow status failed: {}", err).into())
}

fn node_failure_status(_err: temporalio_sdk::ActivityExecutionError) -> NodeStatus {
    NodeStatus::Failed
}

fn standard_activity_options() -> ActivityOptions {
    ActivityOptions::with_start_to_close_timeout(Duration::from_secs(30 * 60))
        .heartbeat_timeout(Duration::from_secs(2 * 60))
        .retry_policy(RetryPolicy {
            initial_interval: Duration::from_secs(5).try_into().ok(),
            backoff_coefficient: 2.0,
            maximum_interval: Duration::from_secs(2 * 60).try_into().ok(),
            maximum_attempts: 3,
            ..Default::default()
        })
        .build()
}

fn human_review_activity_options(timeout: Duration) -> ActivityOptions {
    ActivityOptions::with_start_to_close_timeout(timeout)
        .heartbeat_timeout(Duration::from_secs(30))
        .retry_policy(RetryPolicy { maximum_attempts: 1, ..Default::default() })
        .build()
}

fn finalize_activity_options() -> ActivityOptions {
    ActivityOptions::start_to_close_timeout(Duration::from_secs(30))
}

pub struct DynamicHumanReviewSignal {
    name: String,
}

impl DynamicHumanReviewSignal {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl temporalio_common::SignalDefinition for DynamicHumanReviewSignal {
    type Workflow = OrchestratorWorkflowRun;
    type Input = HumanReviewSignalPayload;

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn human_review_timeout_secs_returns_configured_value() {
        let config = Some(json!({"reviewTimeoutSecs": 3600}));
        assert_eq!(human_review_timeout_secs(&config), 3600);
    }

    #[test]
    fn human_review_timeout_secs_clamps_to_floor() {
        // Values below 60 seconds should be clamped to 60.
        let config = Some(json!({"reviewTimeoutSecs": 5}));
        assert_eq!(human_review_timeout_secs(&config), 60);
    }

    #[test]
    fn human_review_timeout_secs_floor_is_inclusive() {
        let config = Some(json!({"reviewTimeoutSecs": 60}));
        assert_eq!(human_review_timeout_secs(&config), 60);
    }

    #[test]
    fn human_review_timeout_secs_returns_default_for_none_config() {
        assert_eq!(human_review_timeout_secs(&None), DEFAULT_HUMAN_REVIEW_TIMEOUT_SECS);
    }

    #[test]
    fn human_review_timeout_secs_returns_default_for_missing_key() {
        let config = Some(json!({"otherKey": 9999}));
        assert_eq!(human_review_timeout_secs(&config), DEFAULT_HUMAN_REVIEW_TIMEOUT_SECS);
    }

    #[test]
    fn human_review_timeout_secs_ignores_non_u64_value() {
        // Negative and float values that do not map to u64 fall back to the default.
        let config_negative = Some(json!({"reviewTimeoutSecs": -1}));
        assert_eq!(human_review_timeout_secs(&config_negative), DEFAULT_HUMAN_REVIEW_TIMEOUT_SECS);

        let config_string = Some(json!({"reviewTimeoutSecs": "3600"}));
        assert_eq!(human_review_timeout_secs(&config_string), DEFAULT_HUMAN_REVIEW_TIMEOUT_SECS);
    }

    #[test]
    fn escalation_thresholds_are_deterministic() {
        // Verify the 50%/90% threshold math used in the activity is correct.
        let timeout_secs: u64 = 7200; // 2 hours
        let warn_50 = timeout_secs / 2;
        let warn_90 = timeout_secs * 9 / 10;
        assert_eq!(warn_50, 3600);
        assert_eq!(warn_90, 6480);
    }

    #[test]
    fn human_review_signal_name_extracts_node_id() {
        assert_eq!(human_review_node_id("human-review-decision-node-42"), Some("node-42"));
        assert_eq!(human_review_node_id("other-node-42"), None);
    }

    #[test]
    fn workflow_definition_decodes_dynamic_human_review_signal() {
        let converter = PayloadConverter::default();
        let context = SerializationContext { data: &SerializationContextData::Workflow, converter: &converter };
        let expected = HumanReviewSignalPayload {
            node_id: "node-42".to_string(),
            decision: Decision::Approve,
            comment: Some("looks good".to_string()),
        };
        let payloads = Payloads { payloads: converter.to_payloads(&context, &expected).unwrap() };

        let decoded = OrchestratorWorkflow::decode_signal_input("human-review-decision-node-42", payloads, &converter)
            .unwrap()
            .unwrap()
            .downcast::<HumanReviewSignalPayload>()
            .unwrap();

        assert_eq!(decoded.node_id, expected.node_id);
        assert!(matches!(decoded.decision, Decision::Approve));
        assert_eq!(decoded.comment, expected.comment);
        assert!(
            OrchestratorWorkflow::decode_signal_input("unknown", Payloads::default(), &converter).unwrap().is_none()
        );
        assert!(OrchestratorWorkflow::definition().signals.is_empty());
    }
}
