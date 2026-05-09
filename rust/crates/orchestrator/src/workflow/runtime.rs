use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use super::errors::{Result, WorkflowError};
use super::model::{Decision, NodeStatus, SignalRequest, Workflow, WorkflowNode, WorkflowStatus};
use super::store::Store;

#[async_trait]
pub trait WorkflowRuntime: Send + Sync {
    fn kind(&self) -> &'static str;

    async fn start_workflow(&self, workflow: &Workflow, nodes: &[WorkflowNode], layers: &[Vec<String>]) -> Result<()>;
    async fn cancel_workflow(&self, workflow: &Workflow) -> Result<()>;
    async fn signal_workflow(&self, workflow: &Workflow, signal: SignalRequest, layers: &[Vec<String>]) -> Result<()>;
}

#[derive(Clone)]
pub struct MemoryWorkflowRuntime {
    store: Arc<dyn Store>,
}

impl MemoryWorkflowRuntime {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    async fn advance(&self, workflow_id: &str, org_id: &str, layers: &[Vec<String>]) -> Result<()> {
        loop {
            let nodes = self.store.get_nodes(workflow_id).await?;
            let mut by_name = HashMap::new();
            for node in &nodes {
                by_name.insert(node.name.clone(), node.clone());
            }

            let mut progressed = false;
            for layer in layers {
                for name in layer {
                    let Some(node) = by_name.get(name) else {
                        continue;
                    };
                    if node.status != NodeStatus::Pending {
                        continue;
                    }
                    let dep_states: Vec<NodeStatus> =
                        node.depends_on.iter().filter_map(|dep| by_name.get(dep).map(|node| node.status)).collect();
                    if dep_states.contains(&NodeStatus::Failed) {
                        self.store.update_node_status(&node.id, NodeStatus::Skipped, None, None).await?;
                        progressed = true;
                        continue;
                    }
                    if dep_states.iter().all(|status| *status == NodeStatus::Completed) {
                        match node.node_type {
                            super::model::NodeType::AgentTask | super::model::NodeType::Gate => {
                                self.store.update_node_status(&node.id, NodeStatus::Completed, None, None).await?;
                                progressed = true;
                            }
                            super::model::NodeType::HumanReview => {
                                self.store.update_node_status(&node.id, NodeStatus::Running, None, None).await?;
                                progressed = true;
                            }
                        }
                    }
                }
            }

            let nodes = self.store.get_nodes(workflow_id).await?;
            if nodes.iter().any(|node| node.status == NodeStatus::Failed) {
                self.store.update_status(workflow_id, org_id, WorkflowStatus::Failed).await?;
                return Ok(());
            }
            if nodes.iter().all(|node| matches!(node.status, NodeStatus::Completed | NodeStatus::Skipped)) {
                self.store.update_status(workflow_id, org_id, WorkflowStatus::Completed).await?;
                return Ok(());
            }
            if !progressed {
                self.store.update_status(workflow_id, org_id, WorkflowStatus::Running).await?;
                return Ok(());
            }
        }
    }
}

#[async_trait]
impl WorkflowRuntime for MemoryWorkflowRuntime {
    fn kind(&self) -> &'static str {
        "memory"
    }

    async fn start_workflow(&self, workflow: &Workflow, _nodes: &[WorkflowNode], layers: &[Vec<String>]) -> Result<()> {
        self.store
            .set_temporal_ids(
                &workflow.id,
                &workflow.org_id,
                format!("orchestrator-{}", workflow.id),
                Uuid::now_v7().to_string(),
            )
            .await?;
        self.store.update_status(&workflow.id, &workflow.org_id, WorkflowStatus::Running).await?;
        self.advance(&workflow.id, &workflow.org_id, layers).await
    }

    async fn cancel_workflow(&self, workflow: &Workflow) -> Result<()> {
        self.store.update_status(&workflow.id, &workflow.org_id, WorkflowStatus::Cancelled).await
    }

    async fn signal_workflow(&self, workflow: &Workflow, signal: SignalRequest, layers: &[Vec<String>]) -> Result<()> {
        let nodes = self.store.get_nodes(&workflow.id).await?;
        let node_id = signal.node_id.ok_or_else(|| WorkflowError::InvalidInput("nodeId is required".to_string()))?;
        let decision = signal
            .decision
            .ok_or_else(|| WorkflowError::InvalidInput("decision must be 'approve' or 'reject'".to_string()))?;
        let target = nodes.into_iter().find(|node| node.id == node_id).ok_or(WorkflowError::NotFound)?;
        match decision {
            Decision::Approve => self.store.update_node_status(&target.id, NodeStatus::Completed, None, None).await?,
            Decision::Reject => {
                self.store
                    .update_node_status(
                        &target.id,
                        NodeStatus::Failed,
                        signal.comment.or(Some("rejected".to_string())),
                        None,
                    )
                    .await?
            }
        }

        self.advance(&workflow.id, &workflow.org_id, layers).await
    }
}
