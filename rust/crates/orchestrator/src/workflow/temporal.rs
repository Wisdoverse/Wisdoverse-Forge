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
use temporalio_common::protos::temporal::api::common::v1::RetryPolicy;
use temporalio_common::protos::temporal::api::common::v1::{Payload, Payloads};
use temporalio_common::{HasWorkflowDefinition, WorkflowDefinition};
use temporalio_sdk::workflows::{WorkflowError as TemporalWorkflowError, WorkflowImplementation, WorkflowImplementer};
use temporalio_sdk::workflows::{join_all, select};
use temporalio_sdk::{ActivityOptions, CancellableFuture, WorkflowContext, WorkflowResult, WorkflowTermination};
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
        let options = WorkflowStartOptions::new(TASK_QUEUE.to_string(), temporal_workflow_id.clone()).build();
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

impl WorkflowImplementer for OrchestratorWorkflow {
    fn register_all(defs: &mut temporalio_sdk::workflows::WorkflowDefinitions) {
        defs.register_workflow_run::<Self>();
    }
}

impl WorkflowImplementation for OrchestratorWorkflow {
    type Run = OrchestratorWorkflowRun;

    const HAS_INIT: bool = false;
    const INIT_TAKES_INPUT: bool = false;

    fn name() -> &'static str {
        ORCHESTRATOR_WORKFLOW_NAME
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
    ) -> LocalBoxFuture<'static, std::result::Result<Payload, WorkflowTermination>> {
        async move {
            let input = input.expect("workflow input should be provided to run");
            let payload_converter = ctx.payload_converter().clone();
            let result = run_orchestrator_workflow(ctx, input).await;
            match result {
                Ok(()) => payload_converter
                    .to_payload(
                        &SerializationContext {
                            data: &SerializationContextData::Workflow,
                            converter: &payload_converter,
                        },
                        &(),
                    )
                    .map_err(WorkflowTermination::from),
                Err(err) => Err(err),
            }
        }
        .boxed_local()
    }

    fn dispatch_signal(
        ctx: WorkflowContext<Self>,
        name: &str,
        payloads: Payloads,
        converter: &PayloadConverter,
    ) -> Option<LocalBoxFuture<'static, std::result::Result<(), TemporalWorkflowError>>> {
        let node_id = name.strip_prefix(&format!("{SIGNAL_HUMAN_REVIEW}-"))?.to_string();
        let signal = match converter.from_payloads::<HumanReviewSignalPayload>(
            &SerializationContext { data: &SerializationContextData::Workflow, converter },
            payloads.payloads,
        ) {
            Ok(signal) => signal,
            Err(err) => return Some(async move { Err(err.into()) }.boxed_local()),
        };

        Some(
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
            .boxed_local(),
        )
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
                    .start_activity(
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
                    .start_activity(
                        WorkflowActivities::evaluate_gate,
                        GateInput {
                            node_id: node.id.clone(),
                            node_name: node.name.clone(),
                            config: node.config.clone(),
                            dep_results,
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
                let review_activity = ctx.start_activity(
                    WorkflowActivities::wait_for_human_review,
                    HumanReviewInput {
                        node_id: node.id.clone(),
                        node_name: node.name.clone(),
                        config: node.config.clone(),
                    },
                    human_review_activity_options(),
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
    ctx.start_activity(
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

fn human_review_activity_options() -> ActivityOptions {
    ActivityOptions::with_start_to_close_timeout(Duration::from_secs(24 * 60 * 60))
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
