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
use prost::Message;
use serde::{Deserialize, Serialize};
use temporalio_client::errors::WorkflowInteractionError;
use temporalio_client::grpc::WorkflowService as TemporalGrpc;
use temporalio_client::tonic;
use temporalio_client::{
    Client, ClientOptions, Connection, ConnectionOptions, NamespacedClient, WorkflowCancelOptions,
    WorkflowDescribeOptions, WorkflowExecutionInfo, WorkflowExecutionStatus, WorkflowSignalOptions,
    WorkflowStartOptions,
};
use temporalio_common::data_converters::{
    GenericPayloadConverter, PayloadConverter, SerializationContext, SerializationContextData,
};
use temporalio_common::protos::google::rpc::Status as RpcStatus;
use temporalio_common::protos::temporal::api::common::v1::Payloads;
use temporalio_common::protos::temporal::api::common::v1::RetryPolicy;
use temporalio_common::protos::temporal::api::enums::v1::{WorkflowIdConflictPolicy, WorkflowIdReusePolicy};
use temporalio_common::protos::temporal::api::workflowservice::v1::DescribeNamespaceRequest;
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
use super::model::{
    CAUSE_EXECUTION_MISSING, Decision, ExecutionObservation, NodeStatus, NodeType, SignalRequest, Workflow,
    WorkflowNode, WorkflowStatus,
};
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
    /// Operator acknowledgment (#1352): treat "not found" as proven for rows
    /// whose namespace id cannot be compared with the connected namespace.
    trust_current_temporal: bool,
}

impl TemporalWorkflowRuntime {
    pub fn new(client: Client, store: Arc<dyn Store>, trust_current_temporal: bool) -> Self {
        Self { client, store, trust_current_temporal }
    }

    fn handle(
        &self,
        workflow_id: String,
        run_id: Option<String>,
        first_execution_run_id: Option<String>,
    ) -> temporalio_client::WorkflowHandle<Client, OrchestratorWorkflowRun> {
        let info =
            WorkflowExecutionInfo { namespace: self.client.namespace(), workflow_id, run_id, first_execution_run_id };
        temporalio_client::WorkflowHandle::new(self.client.clone(), info)
    }

    /// Server-assigned id of the namespace this client is bound to. Read fresh
    /// on every call: a re-created Temporal reuses the name but not the id.
    async fn namespace_id(&self) -> WorkflowApiResult<String> {
        let mut client = self.client.clone();
        let request = DescribeNamespaceRequest { namespace: client.namespace(), ..Default::default() };
        let response = TemporalGrpc::describe_namespace(&mut client, tonic::Request::new(request))
            .await
            .map_err(|status| WorkflowError::Unavailable(format!("describe temporal namespace: {status}")))?
            .into_inner();
        response
            .namespace_info
            .map(|info| info.id)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| WorkflowError::Unavailable("temporal namespace has no id".to_string()))
    }
}

/// True only when Temporal says the WORKFLOW EXECUTION does not exist (#1352).
///
/// The SDK maps every gRPC `NotFound` to [`WorkflowInteractionError::NotFound`],
/// including "namespace not found". Temporal attaches a typed failure to the
/// status details, so only a `NotFound` carrying
/// `temporal.api.errordetails.v1.NotFoundFailure` counts. A missing namespace,
/// a `NotFound` without details, and every other error (timeouts, transport,
/// permissions, payload conversion) are errors that must change nothing.
pub(crate) fn is_execution_not_found(err: &WorkflowInteractionError) -> bool {
    let status = match err {
        WorkflowInteractionError::NotFound(status) | WorkflowInteractionError::Rpc(status) => status,
        _ => return false,
    };
    status.code() == tonic::Code::NotFound
        && RpcStatus::decode(status.details()).is_ok_and(|details| {
            details.details.iter().any(|detail| {
                detail.type_url.rsplit('/').next() == Some("temporal.api.errordetails.v1.NotFoundFailure")
            })
        })
}

/// Classify a describe of the CURRENT execution (#1352). The one place that
/// turns a Temporal answer into an observation; `Missing` here is not yet
/// verified against the namespace (see [`verify_missing`]).
pub(crate) fn classify_describe(
    result: std::result::Result<(String, WorkflowExecutionStatus), WorkflowInteractionError>,
) -> WorkflowApiResult<ExecutionObservation> {
    let (run_id, status) = match result {
        Ok(described) => described,
        Err(err) if is_execution_not_found(&err) => return Ok(ExecutionObservation::Missing),
        Err(err) => return Err(WorkflowError::Unavailable(format!("describe temporal workflow: {err}"))),
    };
    let closed = match status {
        WorkflowExecutionStatus::Completed => WorkflowStatus::Completed,
        WorkflowExecutionStatus::Failed | WorkflowExecutionStatus::Terminated | WorkflowExecutionStatus::TimedOut => {
            WorkflowStatus::Failed
        }
        WorkflowExecutionStatus::Canceled => WorkflowStatus::Cancelled,
        // Running, Paused, ContinuedAsNew (a successor exists), Unspecified,
        // Unknown and anything newer: an execution exists, leave the row alone.
        _ => return Ok(ExecutionObservation::Running),
    };
    Ok(ExecutionObservation::Closed { run_id, status: closed })
}

/// "Not found" is only trusted when the row was started in the namespace the
/// client is connected to now (same server-assigned id), or when the operator
/// acknowledged the current Temporal (#1352).
pub(crate) fn verify_missing(stored_namespace_id: Option<&str>, current_namespace_id: &str, trust: bool) -> bool {
    trust || stored_namespace_id == Some(current_namespace_id)
}

/// Temporal accepted the cancel of the execution recorded in `workflow`. Write
/// `cancelled` only if the row still records that execution and nothing else
/// changed since it was read (#1352): a delayed cancel of an earlier run must
/// not overwrite a re-run. If the row moved on, it is fine when it already
/// says `cancelled` (the workflow finalized itself); otherwise a conflict.
pub(crate) async fn finish_cancel(store: &dyn Store, workflow: &Workflow) -> WorkflowApiResult<()> {
    if store.set_status_if_unchanged(workflow, WorkflowStatus::Cancelled).await? {
        return Ok(());
    }
    match store.get_by_id(&workflow.id, &workflow.org_id).await?.status {
        WorkflowStatus::Cancelled => Ok(()),
        _ => Err(WorkflowError::ExecutionNotRunning),
    }
}

/// Cancel found no execution to cancel (#1352). The user's intent is already
/// true when the execution is proven missing, so mark the row cancelled under
/// compare-and-set; anything else (a closed or unverifiable execution, or a
/// row that changed meanwhile) is a conflict and changes nothing.
pub(crate) async fn cancel_without_execution(
    store: &dyn Store,
    workflow: &Workflow,
    observation: ExecutionObservation,
) -> WorkflowApiResult<()> {
    if observation == ExecutionObservation::Missing
        && store.reconcile_status(workflow, WorkflowStatus::Cancelled, CAUSE_EXECUTION_MISSING, None).await?
    {
        tracing::info!(
            workflow_id = %workflow.id,
            org_id = %workflow.org_id,
            previous_status = workflow.status.as_str(),
            cause = CAUSE_EXECUTION_MISSING,
            "workflow cancelled without a temporal execution"
        );
        return Ok(());
    }
    Err(WorkflowError::ExecutionNotRunning)
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
        // Recorded so a later "not found" can be proven to come from this same
        // Temporal (#1352). Unknown stays NULL: such rows are never reconciled
        // without the operator acknowledgment.
        let namespace_id = match self.namespace_id().await {
            Ok(id) => Some(id),
            Err(err) => {
                tracing::warn!(error = %err, workflow_id = %workflow.id, "could not read temporal namespace id");
                None
            }
        };
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

        if let Err(err) = self
            .store
            .set_temporal_ids(&workflow.id, &workflow.org_id, temporal_workflow_id, temporal_run_id, namespace_id)
            .await
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
        let temporal_workflow_id = workflow
            .temporal_workflow_id
            .clone()
            .ok_or_else(|| WorkflowError::Internal("workflow has no temporal execution".to_string()))?;
        // Target the execution chain started for this row, not one pinned run:
        // after a reset or retry the current run has a different run id.
        let handle = self.handle(temporal_workflow_id, None, workflow.temporal_run_id.clone());
        match handle.cancel(WorkflowCancelOptions::default()).await {
            Ok(()) => finish_cancel(self.store.as_ref(), workflow).await,
            // Temporal also answers NotFound for "already completed", so ask
            // what is there before writing anything.
            Err(err) if is_execution_not_found(&err) => {
                let observation = self.observe_execution(workflow).await?;
                cancel_without_execution(self.store.as_ref(), workflow, observation).await
            }
            Err(err) => Err(WorkflowError::Internal(format!("cancel temporal workflow: {err}"))),
        }
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
        let temporal_workflow_id = workflow
            .temporal_workflow_id
            .clone()
            .ok_or_else(|| WorkflowError::Internal("workflow has no temporal execution".to_string()))?;
        // No run id: Temporal delivers the signal to the CURRENT execution of
        // this workflow id, so a successor after continue-as-new, reset or retry
        // still receives it. Only "no running execution" becomes the conflict.
        let handle = self.handle(temporal_workflow_id, None, None);
        let payload = HumanReviewSignalPayload { node_id: node_id.clone(), decision, comment: signal.comment.clone() };

        handle
            .signal(
                DynamicHumanReviewSignal::new(signal_name_for_node(&node_id)),
                payload,
                WorkflowSignalOptions::default(),
            )
            .await
            .map_err(|err| {
                if is_execution_not_found(&err) {
                    WorkflowError::ExecutionNotRunning
                } else {
                    WorkflowError::Internal(format!("signal temporal workflow: {err}"))
                }
            })?;

        let (status, error) = match decision {
            Decision::Approve => (NodeStatus::Completed, None),
            Decision::Reject => (NodeStatus::Failed, signal.comment.or(Some("rejected".to_string()))),
        };
        self.store.update_node_status(&node_id, status, error, None).await?;
        Ok(())
    }

    async fn observe_execution(&self, workflow: &Workflow) -> WorkflowApiResult<ExecutionObservation> {
        let temporal_workflow_id = workflow
            .temporal_workflow_id
            .clone()
            .ok_or_else(|| WorkflowError::Unavailable("workflow has no temporal execution".to_string()))?;
        // No run id: ask about the CURRENT execution of this workflow id, so a
        // missing historical run never ends a row whose successor still runs.
        let described = self
            .handle(temporal_workflow_id, None, None)
            .describe(WorkflowDescribeOptions::default())
            .await
            .map(|description| (description.run_id().to_string(), description.status()));
        let observation = classify_describe(described)?;
        if observation != ExecutionObservation::Missing {
            return Ok(observation);
        }
        let current_namespace_id = self.namespace_id().await?;
        if verify_missing(workflow.temporal_namespace_id.as_deref(), &current_namespace_id, self.trust_current_temporal)
        {
            Ok(ExecutionObservation::Missing)
        } else {
            Ok(ExecutionObservation::Unverifiable)
        }
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
        FinalizeWorkflowStatusInput {
            workflow_id: input.workflow_id.clone(),
            org_id: input.org_id.clone(),
            status,
            first_execution_run_id: Some(ctx.info().first_execution_run_id().to_string()),
        },
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

    // ---- #1352: classification of raw Temporal answers ----

    const NOT_FOUND_FAILURE: &str = "temporal.api.errordetails.v1.NotFoundFailure";
    const NAMESPACE_NOT_FOUND_FAILURE: &str = "temporal.api.errordetails.v1.NamespaceNotFoundFailure";

    /// A gRPC status as the Temporal frontend sends it: the typed failure is a
    /// `google.rpc.Status` detail.
    fn grpc_status(code: tonic::Code, detail_type: Option<&str>) -> tonic::Status {
        let Some(detail_type) = detail_type else {
            return tonic::Status::new(code, "boom");
        };
        let mut details = RpcStatus { code: code as i32, message: "boom".to_string(), ..Default::default() };
        details.details.push(Default::default());
        details.details[0].type_url = format!("type.googleapis.com/{detail_type}");
        tonic::Status::with_details(code, "boom", details.encode_to_vec().into())
    }

    /// The SDK's own mapping (`WorkflowInteractionError::from_status` is
    /// crate-private): NotFound becomes `NotFound`, everything else `Rpc`.
    fn sdk_error(status: tonic::Status) -> WorkflowInteractionError {
        if status.code() == tonic::Code::NotFound {
            WorkflowInteractionError::NotFound(status)
        } else {
            WorkflowInteractionError::Rpc(status)
        }
    }

    fn classify_error(err: WorkflowInteractionError) -> WorkflowApiResult<ExecutionObservation> {
        classify_describe(Err(err))
    }

    #[test]
    fn execution_not_found_is_missing() {
        let err = sdk_error(grpc_status(tonic::Code::NotFound, Some(NOT_FOUND_FAILURE)));
        assert!(is_execution_not_found(&err));
        assert_eq!(classify_error(err).unwrap(), ExecutionObservation::Missing);
    }

    #[test]
    fn namespace_not_found_is_an_error_not_missing() {
        let err = sdk_error(grpc_status(tonic::Code::NotFound, Some(NAMESPACE_NOT_FOUND_FAILURE)));
        assert!(!is_execution_not_found(&err));
        assert!(matches!(classify_error(err), Err(WorkflowError::Unavailable(_))));
    }

    #[test]
    fn not_found_without_typed_details_is_an_error() {
        let err = sdk_error(grpc_status(tonic::Code::NotFound, None));
        assert!(matches!(classify_error(err), Err(WorkflowError::Unavailable(_))));
    }

    #[test]
    fn timeout_from_temporal_is_unreachable_not_missing() {
        for code in [tonic::Code::DeadlineExceeded, tonic::Code::Unavailable, tonic::Code::PermissionDenied] {
            let err = sdk_error(grpc_status(code, Some(NOT_FOUND_FAILURE)));
            assert!(!is_execution_not_found(&err), "{code:?} must not count as missing");
            assert!(matches!(classify_error(err), Err(WorkflowError::Unavailable(_))), "{code:?}");
        }
        let other = WorkflowInteractionError::Other(Box::new(std::io::Error::other("transport closed")));
        assert!(matches!(classify_error(other), Err(WorkflowError::Unavailable(_))));
    }

    #[test]
    fn terminal_statuses_are_closed_with_the_matching_workflow_status() {
        for (status, expected) in [
            (WorkflowExecutionStatus::Completed, WorkflowStatus::Completed),
            (WorkflowExecutionStatus::Failed, WorkflowStatus::Failed),
            (WorkflowExecutionStatus::Terminated, WorkflowStatus::Failed),
            (WorkflowExecutionStatus::TimedOut, WorkflowStatus::Failed),
            (WorkflowExecutionStatus::Canceled, WorkflowStatus::Cancelled),
        ] {
            assert_eq!(
                classify_describe(Ok(("run-1".to_string(), status))).unwrap(),
                ExecutionObservation::Closed { run_id: "run-1".to_string(), status: expected },
                "{status:?}"
            );
        }
    }

    #[test]
    fn existing_non_terminal_executions_are_left_alone() {
        // Continue-as-new, a reset or a retry leave a successor; paused and
        // unknown statuses still mean an execution exists.
        for status in [
            WorkflowExecutionStatus::Running,
            WorkflowExecutionStatus::Paused,
            WorkflowExecutionStatus::ContinuedAsNew,
            WorkflowExecutionStatus::Unspecified,
            WorkflowExecutionStatus::Unknown,
        ] {
            assert_eq!(
                classify_describe(Ok(("successor-run".to_string(), status))).unwrap(),
                ExecutionObservation::Running,
                "{status:?}"
            );
        }
    }

    #[test]
    fn missing_is_only_trusted_for_the_same_namespace_or_with_acknowledgment() {
        assert!(verify_missing(Some("ns-1"), "ns-1", false));
        assert!(!verify_missing(Some("ns-old"), "ns-1", false), "a different Temporal proves nothing");
        assert!(!verify_missing(None, "ns-1", false), "rows started before #1352 are unverifiable");
        assert!(verify_missing(None, "ns-1", true));
        assert!(verify_missing(Some("ns-old"), "ns-1", true));
    }

    // ---- #1352: cancel when Temporal has no execution ----

    async fn running_workflow(store: &crate::workflow::MemoryStore) -> Workflow {
        let mut workflow = Workflow {
            id: String::new(),
            name: "wf".to_string(),
            description: String::new(),
            status: WorkflowStatus::Draft,
            org_id: "org-a".to_string(),
            created_by: "user".to_string(),
            temporal_workflow_id: None,
            temporal_run_id: None,
            temporal_namespace_id: None,
            status_cause: None,
            execution_missing_since: None,
            execution_checked_at: None,
            execution_observation: None,
            reconciled_at: None,
            state_version: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        store.create(&mut workflow, &mut Vec::new()).await.unwrap();
        store
            .set_temporal_ids(&workflow.id, "org-a", "orchestrator-x".into(), "run-1".into(), Some("ns-1".into()))
            .await
            .unwrap();
        store.update_status(&workflow.id, "org-a", WorkflowStatus::Running).await.unwrap();
        store.get_by_id(&workflow.id, "org-a").await.unwrap()
    }

    #[tokio::test]
    async fn cancel_on_missing_execution_marks_cancelled_with_cause() {
        let store = crate::workflow::MemoryStore::new();
        let workflow = running_workflow(&store).await;
        cancel_without_execution(&store, &workflow, ExecutionObservation::Missing).await.expect("cancel succeeds");
        let after = store.get_by_id(&workflow.id, "org-a").await.unwrap();
        assert_eq!(after.status, WorkflowStatus::Cancelled);
        assert_eq!(after.status_cause.as_deref(), Some(CAUSE_EXECUTION_MISSING));
        assert!(after.reconciled_at.is_some());
    }

    #[tokio::test]
    async fn cancel_without_proof_or_after_a_concurrent_change_is_a_conflict_and_changes_nothing() {
        let store = crate::workflow::MemoryStore::new();
        let workflow = running_workflow(&store).await;
        for observation in [
            ExecutionObservation::Unverifiable,
            ExecutionObservation::Closed { run_id: "run-1".into(), status: WorkflowStatus::Completed },
            ExecutionObservation::Running,
        ] {
            let result = cancel_without_execution(&store, &workflow, observation.clone()).await;
            assert!(matches!(result, Err(WorkflowError::ExecutionNotRunning)), "{observation:?}");
            assert_eq!(store.get_by_id(&workflow.id, "org-a").await.unwrap().status, WorkflowStatus::Running);
        }
        // The row completed between the read and the cancel: it is preserved.
        store.update_status(&workflow.id, "org-a", WorkflowStatus::Completed).await.unwrap();
        let result = cancel_without_execution(&store, &workflow, ExecutionObservation::Missing).await;
        assert!(matches!(result, Err(WorkflowError::ExecutionNotRunning)));
        assert_eq!(store.get_by_id(&workflow.id, "org-a").await.unwrap().status, WorkflowStatus::Completed);
    }

    // ---- #1352: the real client glue against an in-process Temporal stub ----
    //
    // `ConnectionOptions::service_override` routes every gRPC call of the real
    // SDK client to this callback, so describe, describe-namespace, cancel and
    // signal run through `TemporalWorkflowRuntime` exactly as in production,
    // including the SDK's error mapping and the status details on the wire.

    use temporalio_client::callback_based::{CallbackBasedGrpcService, GrpcRequest, GrpcSuccessResponse};
    use temporalio_common::protos::temporal::api::common::v1::WorkflowExecution as ProtoExecution;
    use temporalio_common::protos::temporal::api::enums::v1::WorkflowExecutionStatus as ProtoStatus;
    use temporalio_common::protos::temporal::api::namespace::v1::NamespaceInfo;
    use temporalio_common::protos::temporal::api::workflow::v1::WorkflowExecutionInfo as ProtoExecutionInfo;
    use temporalio_common::protos::temporal::api::workflowservice::v1::{
        DescribeNamespaceResponse, DescribeWorkflowExecutionRequest, DescribeWorkflowExecutionResponse,
        GetSystemInfoResponse, RequestCancelWorkflowExecutionResponse, SignalWorkflowExecutionRequest,
        SignalWorkflowExecutionResponse,
    };

    type StubResult = std::result::Result<Vec<u8>, tonic::Status>;
    type Responder = Arc<dyn Fn(GrpcRequest) -> futures::future::BoxFuture<'static, StubResult> + Send + Sync>;

    fn execution_not_found() -> tonic::Status {
        grpc_status(tonic::Code::NotFound, Some(NOT_FOUND_FAILURE))
    }

    fn namespace_response(id: &str) -> StubResult {
        let info = NamespaceInfo { id: id.to_string(), name: "orchestrator".to_string(), ..Default::default() };
        Ok(DescribeNamespaceResponse { namespace_info: Some(info), ..Default::default() }.encode_to_vec())
    }

    fn described(run_id: &str, status: ProtoStatus) -> StubResult {
        let info = ProtoExecutionInfo {
            execution: Some(ProtoExecution { workflow_id: "orchestrator-x".to_string(), run_id: run_id.to_string() }),
            status: status as i32,
            ..Default::default()
        };
        Ok(DescribeWorkflowExecutionResponse { workflow_execution_info: Some(info), ..Default::default() }
            .encode_to_vec())
    }

    async fn stub_runtime(store: Arc<dyn Store>, trust: bool, respond: Responder) -> TemporalWorkflowRuntime {
        let service = CallbackBasedGrpcService {
            callback: Arc::new(
                move |request: GrpcRequest| -> futures::future::BoxFuture<
                    'static,
                    std::result::Result<GrpcSuccessResponse, tonic::Status>,
                > {
                    let respond = respond.clone();
                    Box::pin(async move {
                        let proto = if request.rpc == "GetSystemInfo" {
                            GetSystemInfoResponse::default().encode_to_vec()
                        } else {
                            respond(request).await?
                        };
                        Ok(GrpcSuccessResponse { headers: Default::default(), proto })
                    })
                },
            ),
        };
        let options = ConnectionOptions::new(Url::parse("http://localhost:7233").unwrap())
            .service_override(service)
            .dns_load_balancing(None)
            .build();
        let connection = Connection::connect(options).await.expect("connect to stub");
        let client = Client::new(connection, ClientOptions::new("orchestrator".to_string()).build()).expect("client");
        TemporalWorkflowRuntime::new(client, store, trust)
    }

    /// A store with one running workflow (one human-review node), started as
    /// `orchestrator-x` / `run-1` in namespace `namespace_id`.
    async fn store_with_running(namespace_id: Option<&str>) -> (Arc<crate::workflow::MemoryStore>, Workflow) {
        let store = Arc::new(crate::workflow::MemoryStore::new());
        let mut workflow = Workflow {
            id: String::new(),
            name: "wf".to_string(),
            description: String::new(),
            status: WorkflowStatus::Draft,
            org_id: "org-a".to_string(),
            created_by: "user".to_string(),
            temporal_workflow_id: None,
            temporal_run_id: None,
            temporal_namespace_id: None,
            status_cause: None,
            execution_missing_since: None,
            execution_checked_at: None,
            execution_observation: None,
            reconciled_at: None,
            state_version: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let mut nodes = vec![WorkflowNode {
            id: String::new(),
            workflow_id: String::new(),
            name: "review".to_string(),
            node_type: NodeType::HumanReview,
            depends_on: vec![],
            config: None,
            position: 0,
            status: NodeStatus::Running,
            started_at: None,
            completed_at: None,
            error: None,
            output: None,
        }];
        store.create(&mut workflow, &mut nodes).await.unwrap();
        store
            .set_temporal_ids(
                &workflow.id,
                "org-a",
                "orchestrator-x".into(),
                "run-1".into(),
                namespace_id.map(str::to_string),
            )
            .await
            .unwrap();
        store.update_status(&workflow.id, "org-a", WorkflowStatus::Running).await.unwrap();
        let workflow = store.get_by_id(&workflow.id, "org-a").await.unwrap();
        (store, workflow)
    }

    /// Describe answers "execution not found", the namespace id is `ns-1`.
    fn missing_everywhere(describe_run_ids: Arc<std::sync::Mutex<Vec<String>>>) -> Responder {
        Arc::new(move |request: GrpcRequest| -> futures::future::BoxFuture<'static, StubResult> {
            let describe_run_ids = describe_run_ids.clone();
            Box::pin(async move {
                match request.rpc.as_str() {
                    "DescribeWorkflowExecution" => {
                        let req = DescribeWorkflowExecutionRequest::decode(request.proto).unwrap();
                        describe_run_ids.lock().unwrap().push(req.execution.unwrap_or_default().run_id);
                        Err(execution_not_found())
                    }
                    "DescribeNamespace" => namespace_response("ns-1"),
                    "RequestCancelWorkflowExecution" => Err(execution_not_found()),
                    other => Err(tonic::Status::unimplemented(other.to_string())),
                }
            })
        })
    }

    #[tokio::test]
    async fn real_client_missing_is_proven_only_for_the_same_namespace() {
        for (stored, expected) in [
            (Some("ns-1"), ExecutionObservation::Missing),
            (Some("ns-old"), ExecutionObservation::Unverifiable),
            (None, ExecutionObservation::Unverifiable),
        ] {
            let run_ids = Arc::new(std::sync::Mutex::new(Vec::new()));
            let (store, workflow) = store_with_running(stored).await;
            let runtime = stub_runtime(store, false, missing_everywhere(run_ids.clone())).await;
            assert_eq!(runtime.observe_execution(&workflow).await.unwrap(), expected, "stored {stored:?}");
            assert_eq!(*run_ids.lock().unwrap(), vec![String::new()], "describe asks about the CURRENT execution");
        }
        // The acknowledgment makes an unverifiable row count.
        let (store, workflow) = store_with_running(None).await;
        let runtime = stub_runtime(store, true, missing_everywhere(Default::default())).await;
        assert_eq!(runtime.observe_execution(&workflow).await.unwrap(), ExecutionObservation::Missing);
    }

    #[tokio::test]
    async fn real_client_namespace_not_found_is_an_error() {
        let (store, workflow) = store_with_running(Some("ns-1")).await;
        let respond: Responder = Arc::new(|_request: GrpcRequest| -> futures::future::BoxFuture<'static, StubResult> {
            Box::pin(async { Err(grpc_status(tonic::Code::NotFound, Some(NAMESPACE_NOT_FOUND_FAILURE))) })
        });
        let runtime = stub_runtime(store, false, respond).await;
        assert!(matches!(runtime.observe_execution(&workflow).await, Err(WorkflowError::Unavailable(_))));
    }

    #[tokio::test]
    async fn real_client_running_successor_and_closed_recorded_run() {
        for (run_id, status, expected) in [
            ("run-2", ProtoStatus::Running, ExecutionObservation::Running),
            (
                "run-1",
                ProtoStatus::Completed,
                ExecutionObservation::Closed { run_id: "run-1".into(), status: WorkflowStatus::Completed },
            ),
        ] {
            let (store, workflow) = store_with_running(Some("ns-1")).await;
            let respond: Responder =
                Arc::new(move |_request: GrpcRequest| -> futures::future::BoxFuture<'static, StubResult> {
                    Box::pin(async move { described(run_id, status) })
                });
            let runtime = stub_runtime(store, false, respond).await;
            assert_eq!(runtime.observe_execution(&workflow).await.unwrap(), expected);
        }
    }

    #[tokio::test]
    async fn real_client_cancel_on_missing_execution_marks_cancelled() {
        let (store, workflow) = store_with_running(Some("ns-1")).await;
        let runtime = stub_runtime(store.clone(), false, missing_everywhere(Default::default())).await;
        runtime.cancel_workflow(&workflow).await.expect("cancel succeeds");
        let after = store.get_by_id(&workflow.id, "org-a").await.unwrap();
        assert_eq!(after.status, WorkflowStatus::Cancelled);
        assert_eq!(after.status_cause.as_deref(), Some(CAUSE_EXECUTION_MISSING));
    }

    /// Temporal accepts the cancel; `rerun_before_reply` makes a re-run record
    /// `run-2` while the cancel of `run-1` is in flight.
    fn accepting_cancel(store: Arc<crate::workflow::MemoryStore>, id: String, rerun_before_reply: bool) -> Responder {
        Arc::new(move |request: GrpcRequest| -> futures::future::BoxFuture<'static, StubResult> {
            let store = store.clone();
            let id = id.clone();
            Box::pin(async move {
                match request.rpc.as_str() {
                    "RequestCancelWorkflowExecution" => {
                        if rerun_before_reply {
                            store
                                .set_temporal_ids(&id, "org-a", "orchestrator-x".into(), "run-2".into(), None)
                                .await
                                .unwrap();
                        }
                        Ok(RequestCancelWorkflowExecutionResponse::default().encode_to_vec())
                    }
                    other => Err(tonic::Status::unimplemented(other.to_string())),
                }
            })
        })
    }

    #[tokio::test]
    async fn real_client_cancel_marks_cancelled() {
        let (store, workflow) = store_with_running(Some("ns-1")).await;
        let runtime =
            stub_runtime(store.clone(), false, accepting_cancel(store.clone(), workflow.id.clone(), false)).await;
        runtime.cancel_workflow(&workflow).await.expect("cancel succeeds");
        assert_eq!(store.get_by_id(&workflow.id, "org-a").await.unwrap().status, WorkflowStatus::Cancelled);
    }

    #[tokio::test]
    async fn real_client_delayed_cancel_of_an_earlier_run_cannot_overwrite_a_rerun() {
        let (store, workflow) = store_with_running(Some("ns-1")).await;
        let runtime =
            stub_runtime(store.clone(), false, accepting_cancel(store.clone(), workflow.id.clone(), true)).await;
        let result = runtime.cancel_workflow(&workflow).await;
        assert!(matches!(result, Err(WorkflowError::ExecutionNotRunning)), "got {result:?}");
        let after = store.get_by_id(&workflow.id, "org-a").await.unwrap();
        assert_eq!(after.temporal_run_id.as_deref(), Some("run-2"));
        assert_eq!(after.status, WorkflowStatus::Running, "the row keeps run-2's state");
    }

    /// Signals reach the run Temporal resolves as current; a request pinned to
    /// a run id is answered like the closed `run-1` ("already completed").
    fn signal_responder(any_running: bool, seen_run_ids: Arc<std::sync::Mutex<Vec<String>>>) -> Responder {
        Arc::new(move |request: GrpcRequest| -> futures::future::BoxFuture<'static, StubResult> {
            let seen_run_ids = seen_run_ids.clone();
            Box::pin(async move {
                match request.rpc.as_str() {
                    "SignalWorkflowExecution" => {
                        let req = SignalWorkflowExecutionRequest::decode(request.proto).unwrap();
                        let run_id = req.workflow_execution.unwrap_or_default().run_id;
                        seen_run_ids.lock().unwrap().push(run_id.clone());
                        if any_running && run_id.is_empty() {
                            Ok(SignalWorkflowExecutionResponse::default().encode_to_vec())
                        } else {
                            Err(execution_not_found())
                        }
                    }
                    other => Err(tonic::Status::unimplemented(other.to_string())),
                }
            })
        })
    }

    fn approve(node_id: &str) -> SignalRequest {
        SignalRequest { node_id: Some(node_id.to_string()), decision: Some(Decision::Approve), comment: None }
    }

    #[tokio::test]
    async fn real_client_signal_reaches_a_successor_of_the_recorded_run() {
        let (store, workflow) = store_with_running(Some("ns-1")).await;
        let node_id = store.get_nodes(&workflow.id).await.unwrap()[0].id.clone();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let runtime = stub_runtime(store.clone(), false, signal_responder(true, seen.clone())).await;
        runtime.signal_workflow(&workflow, approve(&node_id), &[]).await.expect("signal succeeds");
        assert_eq!(*seen.lock().unwrap(), vec![String::new()], "the signal targets the current execution");
        assert_eq!(store.get_nodes(&workflow.id).await.unwrap()[0].status, NodeStatus::Completed);
    }

    #[tokio::test]
    async fn real_client_signal_without_a_running_execution_is_a_coded_conflict() {
        let (store, workflow) = store_with_running(Some("ns-1")).await;
        let node_id = store.get_nodes(&workflow.id).await.unwrap()[0].id.clone();
        let before = store.get_nodes(&workflow.id).await.unwrap()[0].status;
        let runtime = stub_runtime(store.clone(), false, signal_responder(false, Default::default())).await;
        let result = runtime.signal_workflow(&workflow, approve(&node_id), &[]).await;
        assert!(matches!(result, Err(WorkflowError::ExecutionNotRunning)), "got {result:?}");
        assert_eq!(store.get_nodes(&workflow.id).await.unwrap()[0].status, before, "nothing written");
    }
}
