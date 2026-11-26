use std::sync::Arc;
use std::time::Duration;

use super::dag::validate_dag;
use super::errors::{Result, WorkflowError};
use tokio::sync::Semaphore;

use super::model::{
    ExecutionObservation, ExecutionState, NodeHistory, NodeRequest, SignalRequest, Workflow, WorkflowNode,
    WorkflowStatus,
};
use super::runtime::WorkflowRuntime;
use super::store::Store;

/// Upper bound on ALL optional work in a status read (#1352): the live
/// Temporal lookup and its bookkeeping. A slow Temporal delays the read by at
/// most this much and never fails it.
const STATUS_READ_OBSERVE_TIMEOUT: Duration = Duration::from_secs(2);
/// Live lookups that status reads may run at once. A read that finds no free
/// permit skips the lookup instead of waiting.
const STATUS_READ_OBSERVE_PERMITS: usize = 4;

#[derive(Clone)]
pub struct WorkflowService {
    store: Arc<dyn Store>,
    runtime: Arc<dyn WorkflowRuntime>,
    observe_permits: Arc<Semaphore>,
}

impl WorkflowService {
    pub fn new(store: Arc<dyn Store>, runtime: Arc<dyn WorkflowRuntime>) -> Self {
        Self { store, runtime, observe_permits: Arc::new(Semaphore::new(STATUS_READ_OBSERVE_PERMITS)) }
    }

    /// Override how many status-read lookups may run at once (tests use 0).
    pub fn with_status_observe_permits(mut self, permits: usize) -> Self {
        self.observe_permits = Arc::new(Semaphore::new(permits));
        self
    }

    pub fn runtime_kind(&self) -> &'static str {
        self.runtime.kind()
    }

    /// The runtime behind this service, for the execution reconciler (#1352).
    pub fn runtime(&self) -> Arc<dyn WorkflowRuntime> {
        self.runtime.clone()
    }

    pub async fn start_workflow(&self, workflow_id: &str, org_id: &str) -> Result<Workflow> {
        let workflow = self.store.get_by_id(workflow_id, org_id).await?;
        if !matches!(workflow.status, WorkflowStatus::Draft | WorkflowStatus::Failed) {
            return Err(WorkflowError::Internal(format!(
                "workflow must be in draft or failed status to run, current: {}",
                workflow.status.as_str()
            )));
        }

        // Re-running from a failed state reuses the same Temporal workflow id, so
        // clear the persisted node execution state first: otherwise the UI shows a
        // mix of the prior run's completed/failed nodes alongside the fresh run
        // until each node is re-touched (F041).
        if matches!(workflow.status, WorkflowStatus::Failed) {
            self.store.reset_nodes(workflow_id, org_id).await?;
        }

        let nodes = self.store.get_nodes(workflow_id).await?;
        let layers = workflow_layers(&nodes)?;
        self.runtime.start_workflow(&workflow, &nodes, &layers).await?;
        self.store.get_by_id(workflow_id, org_id).await
    }

    /// Stored workflow and nodes, plus a bounded live look at its execution
    /// while the row says `running` (#1352). The look is optional: it is
    /// skipped when too many run at once, it is bounded by one timeout, and any
    /// failure in it returns the stored row with state `unknown`. It never ends
    /// the workflow and never moves the reconciler's check time; it may only
    /// record when the execution was first seen missing.
    pub async fn get_status(
        &self,
        workflow_id: &str,
        org_id: &str,
    ) -> Result<(Workflow, Vec<WorkflowNode>, Option<ExecutionState>)> {
        let workflow = self.store.get_by_id(workflow_id, org_id).await?;
        let nodes = self.store.get_nodes(workflow_id).await?;
        if workflow.status != WorkflowStatus::Running || workflow.temporal_workflow_id.is_none() {
            return Ok((workflow, nodes, None));
        }
        let Ok(_permit) = self.observe_permits.try_acquire() else {
            return Ok((workflow, nodes, Some(ExecutionState::Unknown)));
        };
        match tokio::time::timeout(STATUS_READ_OBSERVE_TIMEOUT, self.observe_for_read(&workflow)).await {
            Ok(Ok((observed, state))) => Ok((observed, nodes, Some(state))),
            Ok(Err(err)) => {
                tracing::debug!(error = %err, workflow_id, "status read: live execution lookup failed");
                Ok((workflow, nodes, Some(ExecutionState::Unknown)))
            }
            Err(_) => Ok((workflow, nodes, Some(ExecutionState::Unknown))),
        }
    }

    async fn observe_for_read(&self, workflow: &Workflow) -> Result<(Workflow, ExecutionState)> {
        let state = match self.runtime.observe_execution(workflow).await? {
            ExecutionObservation::Running => ExecutionState::Running,
            ExecutionObservation::Closed { .. } => ExecutionState::Closed,
            ExecutionObservation::Unverifiable => ExecutionState::Unverifiable,
            ExecutionObservation::Missing => {
                if workflow.execution_missing_since.is_none() && self.store.mark_execution_missing(workflow).await? {
                    return Ok((self.store.get_by_id(&workflow.id, &workflow.org_id).await?, ExecutionState::Missing));
                }
                ExecutionState::Missing
            }
        };
        Ok((workflow.clone(), state))
    }

    pub async fn cancel_workflow(&self, workflow_id: &str, org_id: &str) -> Result<()> {
        let workflow = self.store.get_by_id(workflow_id, org_id).await?;
        self.runtime.cancel_workflow(&workflow).await
    }

    pub async fn signal_workflow(&self, workflow_id: &str, org_id: &str, signal: SignalRequest) -> Result<()> {
        let workflow = self.store.get_by_id(workflow_id, org_id).await?;
        let nodes = self.store.get_nodes(workflow_id).await?;
        // Tenant-isolation gate (#884): the node-status write keyed off `node_id` is
        // global, so a signal must be rejected unless the node belongs to this
        // org-scoped workflow — otherwise a caller could flip another org's node by id.
        if let Some(node_id) = signal.node_id.as_deref()
            && !nodes.iter().any(|node| node.id == node_id)
        {
            return Err(WorkflowError::NotFound);
        }
        let layers = workflow_layers(&nodes)?;
        self.runtime.signal_workflow(&workflow, signal, &layers).await
    }

    pub async fn get_history(&self, workflow_id: &str, org_id: &str) -> Result<Vec<NodeHistory>> {
        let _ = self.store.get_by_id(workflow_id, org_id).await?;
        self.store.history(workflow_id).await
    }
}

fn workflow_layers(nodes: &[WorkflowNode]) -> Result<Vec<Vec<String>>> {
    let requests: Vec<NodeRequest> = nodes
        .iter()
        .map(|node| NodeRequest {
            name: Some(node.name.clone()),
            node_type: Some(node.node_type),
            depends_on: node.depends_on.clone(),
            config: node.config.clone(),
        })
        .collect();
    validate_dag(&requests)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use chrono::Utc;
    use serde_json::Value;

    use super::WorkflowService;
    use crate::workflow::errors::{Result as WfResult, WorkflowError};
    use crate::workflow::model::{
        Decision, MissingMark, NodeHistory, NodeStatus, NodeType, SignalRequest, Workflow, WorkflowNode, WorkflowStatus,
    };
    use crate::workflow::runtime::WorkflowRuntime;
    use crate::workflow::store::Store;

    fn test_workflow(id: &str, org: &str) -> Workflow {
        Workflow {
            id: id.into(),
            name: "wf".into(),
            description: String::new(),
            status: WorkflowStatus::Running,
            org_id: org.into(),
            created_by: "user".into(),
            temporal_workflow_id: None,
            temporal_run_id: None,
            temporal_namespace_id: None,
            status_cause: None,
            execution_missing_since: None,
            execution_checked_at: None,
            execution_observation: None,
            reconciled_at: None,
            state_version: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn test_node(id: &str, workflow_id: &str) -> WorkflowNode {
        WorkflowNode {
            id: id.into(),
            workflow_id: workflow_id.into(),
            name: id.into(),
            node_type: NodeType::HumanReview,
            depends_on: vec![],
            config: None,
            position: 0,
            status: NodeStatus::Running,
            started_at: None,
            completed_at: None,
            error: None,
            output: None,
        }
    }

    /// Minimal single-workflow store. `update_node_status` is keyed only by `node_id`
    /// (global), exactly like the production DB write, so a foreign node id is not
    /// rejected at the store layer.
    struct InMemoryStore {
        workflow: Workflow,
        nodes: Mutex<Vec<WorkflowNode>>,
    }

    #[async_trait::async_trait]
    impl Store for InMemoryStore {
        async fn create(&self, _w: &mut Workflow, _n: &mut Vec<WorkflowNode>) -> WfResult<()> {
            Ok(())
        }
        async fn get_by_id(&self, id: &str, org_id: &str) -> WfResult<Workflow> {
            if id == self.workflow.id && org_id == self.workflow.org_id {
                Ok(self.workflow.clone())
            } else {
                Err(WorkflowError::NotFound)
            }
        }
        async fn get_nodes(&self, _workflow_id: &str) -> WfResult<Vec<WorkflowNode>> {
            Ok(self.nodes.lock().unwrap().clone())
        }
        async fn list(&self, _org: &str, _limit: usize, _offset: usize) -> WfResult<Vec<Workflow>> {
            Ok(vec![])
        }
        async fn update_status(&self, _id: &str, _org: &str, _status: WorkflowStatus) -> WfResult<()> {
            Ok(())
        }
        async fn set_temporal_ids(
            &self,
            _id: &str,
            _org: &str,
            _wf: String,
            _run: String,
            _namespace_id: Option<String>,
        ) -> WfResult<()> {
            Ok(())
        }
        async fn update_status_for_run(
            &self,
            _id: &str,
            _org: &str,
            _run: &str,
            _status: WorkflowStatus,
        ) -> WfResult<bool> {
            Ok(true)
        }
        async fn reconcile_candidates(&self, _limit: usize) -> WfResult<Vec<Workflow>> {
            Ok(vec![])
        }
        async fn record_execution_check(&self, _w: &Workflow, _mark: MissingMark, _o: &str) -> WfResult<bool> {
            Ok(false)
        }
        async fn mark_execution_missing(&self, _w: &Workflow) -> WfResult<bool> {
            Err(WorkflowError::Internal("bookkeeping store is down".to_string()))
        }
        async fn set_status_if_unchanged(&self, _w: &Workflow, _status: WorkflowStatus) -> WfResult<bool> {
            Ok(true)
        }
        async fn reconcile_status(
            &self,
            _w: &Workflow,
            _status: WorkflowStatus,
            _cause: &str,
            _missing_for_secs: Option<u64>,
        ) -> WfResult<bool> {
            Ok(false)
        }
        async fn update_node_status(
            &self,
            node_id: &str,
            status: NodeStatus,
            err_msg: Option<String>,
            _output: Option<Value>,
        ) -> WfResult<()> {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(node) = nodes.iter_mut().find(|node| node.id == node_id) {
                node.status = status;
                node.error = err_msg;
            }
            Ok(())
        }
        async fn reset_nodes(&self, _workflow_id: &str, _org_id: &str) -> WfResult<()> {
            for node in self.nodes.lock().unwrap().iter_mut() {
                node.status = NodeStatus::Pending;
                node.started_at = None;
                node.completed_at = None;
                node.error = None;
                node.output = None;
            }
            Ok(())
        }
        async fn history(&self, _workflow_id: &str) -> WfResult<Vec<NodeHistory>> {
            Ok(vec![])
        }
    }

    /// Mimics the Temporal runtime path: blindly writes node status by id with NO
    /// membership validation. Only the service layer can close the cross-tenant gap,
    /// so this double exposes it (the in-tree `MemoryWorkflowRuntime` would mask it).
    struct PermissiveRuntime {
        store: Arc<dyn Store>,
    }

    #[async_trait::async_trait]
    impl WorkflowRuntime for PermissiveRuntime {
        fn kind(&self) -> &'static str {
            "permissive-test"
        }
        async fn start_workflow(&self, _w: &Workflow, _n: &[WorkflowNode], _l: &[Vec<String>]) -> WfResult<()> {
            Ok(())
        }
        async fn cancel_workflow(&self, _w: &Workflow) -> WfResult<()> {
            Ok(())
        }
        async fn signal_workflow(&self, _w: &Workflow, signal: SignalRequest, _l: &[Vec<String>]) -> WfResult<()> {
            let node_id =
                signal.node_id.ok_or_else(|| WorkflowError::InvalidInput("nodeId is required".to_string()))?;
            let (status, err) = match signal.decision {
                Some(Decision::Reject) => (NodeStatus::Failed, signal.comment.or(Some("rejected".to_string()))),
                _ => (NodeStatus::Completed, None),
            };
            self.store.update_node_status(&node_id, status, err, None).await
        }
    }

    fn service_with(nodes: Vec<WorkflowNode>) -> (WorkflowService, Arc<InMemoryStore>) {
        let store = Arc::new(InMemoryStore { workflow: test_workflow("wf-1", "org-a"), nodes: Mutex::new(nodes) });
        let runtime = Arc::new(PermissiveRuntime { store: store.clone() });
        (WorkflowService::new(store.clone(), runtime), store)
    }

    #[tokio::test]
    async fn signal_workflow_rejects_node_not_in_workflow() {
        // Cross-tenant IDOR guard (#884): a node id that does not belong to this
        // org-scoped workflow must be rejected before any node-status write happens.
        let (service, _store) = service_with(vec![test_node("node-a", "wf-1")]);
        let signal = SignalRequest {
            node_id: Some("node-from-another-org".to_string()),
            decision: Some(Decision::Approve),
            comment: None,
        };
        let result = service.signal_workflow("wf-1", "org-a", signal).await;
        assert!(
            matches!(result, Err(WorkflowError::NotFound)),
            "a node id outside the workflow must be rejected, got {result:?}"
        );
    }

    #[tokio::test]
    async fn signal_workflow_accepts_node_in_workflow() {
        let (service, store) = service_with(vec![test_node("node-a", "wf-1")]);
        let signal =
            SignalRequest { node_id: Some("node-a".to_string()), decision: Some(Decision::Approve), comment: None };
        service.signal_workflow("wf-1", "org-a", signal).await.expect("own node must be accepted");
        assert_eq!(store.nodes.lock().unwrap()[0].status, NodeStatus::Completed);
    }

    #[tokio::test]
    async fn rerun_from_failed_resets_nodes_to_pending() {
        // A failed workflow carries completed/failed node state from the prior
        // run; re-running must reset every node to pending so the DB/UI agree
        // with the fresh Temporal run (F041).
        let mut workflow = test_workflow("wf-1", "org-a");
        workflow.status = WorkflowStatus::Failed;
        let mut node = test_node("node-a", "wf-1");
        node.status = NodeStatus::Completed;
        node.started_at = Some(Utc::now());
        node.completed_at = Some(Utc::now());
        node.output = Some(serde_json::json!({"done": true}));

        let store = Arc::new(InMemoryStore { workflow, nodes: Mutex::new(vec![node]) });
        let runtime = Arc::new(PermissiveRuntime { store: store.clone() });
        let service = WorkflowService::new(store.clone(), runtime);

        service.start_workflow("wf-1", "org-a").await.expect("re-run from failed should start");

        let nodes = store.nodes.lock().unwrap();
        assert_eq!(nodes[0].status, NodeStatus::Pending, "re-run must reset the node to pending");
        assert!(nodes[0].started_at.is_none(), "started_at must be cleared");
        assert!(nodes[0].completed_at.is_none(), "completed_at must be cleared");
        assert!(nodes[0].output.is_none(), "output must be cleared");
    }

    #[tokio::test]
    async fn fresh_draft_run_does_not_reset_nodes() {
        // A first run from draft has no prior state to clear, so nodes are left
        // as-is (reset only fires on the failed re-run path).
        let mut workflow = test_workflow("wf-1", "org-a");
        workflow.status = WorkflowStatus::Draft;
        let mut node = test_node("node-a", "wf-1");
        node.status = NodeStatus::Running;

        let store = Arc::new(InMemoryStore { workflow, nodes: Mutex::new(vec![node]) });
        let runtime = Arc::new(PermissiveRuntime { store: store.clone() });
        let service = WorkflowService::new(store.clone(), runtime);

        service.start_workflow("wf-1", "org-a").await.expect("draft run should start");

        assert_eq!(store.nodes.lock().unwrap()[0].status, NodeStatus::Running, "draft run must not reset nodes");
    }

    /// Temporal answers "proven missing" for every workflow; counts calls.
    struct MissingRuntime {
        calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait::async_trait]
    impl WorkflowRuntime for MissingRuntime {
        fn kind(&self) -> &'static str {
            "missing-test"
        }
        async fn start_workflow(&self, _w: &Workflow, _n: &[WorkflowNode], _l: &[Vec<String>]) -> WfResult<()> {
            Ok(())
        }
        async fn cancel_workflow(&self, _w: &Workflow) -> WfResult<()> {
            Ok(())
        }
        async fn signal_workflow(&self, _w: &Workflow, _s: SignalRequest, _l: &[Vec<String>]) -> WfResult<()> {
            Ok(())
        }
        async fn observe_execution(&self, _w: &Workflow) -> WfResult<crate::workflow::model::ExecutionObservation> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(crate::workflow::model::ExecutionObservation::Missing)
        }
    }

    fn running_with_execution() -> (Arc<InMemoryStore>, Arc<MissingRuntime>) {
        let mut workflow = test_workflow("wf-1", "org-a");
        workflow.temporal_workflow_id = Some("orchestrator-wf-1".to_string());
        workflow.temporal_run_id = Some("run-1".to_string());
        let store = Arc::new(InMemoryStore { workflow, nodes: Mutex::new(vec![]) });
        (store, Arc::new(MissingRuntime { calls: std::sync::atomic::AtomicUsize::new(0) }))
    }

    #[tokio::test]
    async fn status_read_without_a_free_permit_returns_stored_data() {
        let (store, runtime) = running_with_execution();
        let service = WorkflowService::new(store, runtime.clone()).with_status_observe_permits(0);
        let (workflow, _nodes, state) = service.get_status("wf-1", "org-a").await.expect("read succeeds");
        assert_eq!(workflow.status, WorkflowStatus::Running);
        assert_eq!(state, Some(crate::workflow::model::ExecutionState::Unknown));
        assert_eq!(runtime.calls.load(std::sync::atomic::Ordering::SeqCst), 0, "no permit, no Temporal call");
    }

    #[tokio::test]
    async fn status_read_succeeds_when_its_bookkeeping_fails() {
        // The test store's `mark_execution_missing` always errors.
        let (store, runtime) = running_with_execution();
        let service = WorkflowService::new(store, runtime.clone());
        let (workflow, _nodes, state) = service.get_status("wf-1", "org-a").await.expect("read succeeds");
        assert_eq!(runtime.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(workflow.status, WorkflowStatus::Running);
        assert!(workflow.execution_missing_since.is_none(), "the stored row is returned");
        assert_eq!(state, Some(crate::workflow::model::ExecutionState::Unknown));
    }
}
