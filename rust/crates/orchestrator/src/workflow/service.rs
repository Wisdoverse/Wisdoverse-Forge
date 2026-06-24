use std::sync::Arc;

use super::dag::validate_dag;
use super::errors::{Result, WorkflowError};
use super::model::{NodeHistory, NodeRequest, SignalRequest, Workflow, WorkflowNode, WorkflowStatus};
use super::runtime::WorkflowRuntime;
use super::store::Store;

#[derive(Clone)]
pub struct WorkflowService {
    store: Arc<dyn Store>,
    runtime: Arc<dyn WorkflowRuntime>,
}

impl WorkflowService {
    pub fn new(store: Arc<dyn Store>, runtime: Arc<dyn WorkflowRuntime>) -> Self {
        Self { store, runtime }
    }

    pub fn runtime_kind(&self) -> &'static str {
        self.runtime.kind()
    }

    pub async fn start_workflow(&self, workflow_id: &str, org_id: &str) -> Result<Workflow> {
        let workflow = self.store.get_by_id(workflow_id, org_id).await?;
        if !matches!(workflow.status, WorkflowStatus::Draft | WorkflowStatus::Failed) {
            return Err(WorkflowError::Internal(format!(
                "workflow must be in draft or failed status to run, current: {}",
                workflow.status.as_str()
            )));
        }

        let nodes = self.store.get_nodes(workflow_id).await?;
        let layers = workflow_layers(&nodes)?;
        self.runtime.start_workflow(&workflow, &nodes, &layers).await?;
        self.store.get_by_id(workflow_id, org_id).await
    }

    pub async fn get_status(&self, workflow_id: &str, org_id: &str) -> Result<(Workflow, Vec<WorkflowNode>)> {
        let workflow = self.store.get_by_id(workflow_id, org_id).await?;
        let nodes = self.store.get_nodes(workflow_id).await?;
        Ok((workflow, nodes))
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
        Decision, NodeHistory, NodeStatus, NodeType, SignalRequest, Workflow, WorkflowNode, WorkflowStatus,
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
        async fn set_temporal_ids(&self, _id: &str, _org: &str, _wf: String, _run: String) -> WfResult<()> {
            Ok(())
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
}
