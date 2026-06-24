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
