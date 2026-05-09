use async_trait::async_trait;
use serde_json::Value;

use super::errors::Result;
use super::model::{NodeHistory, NodeStatus, Workflow, WorkflowNode, WorkflowStatus};

#[async_trait]
pub trait Store: Send + Sync {
    async fn create(&self, workflow: &mut Workflow, nodes: &mut Vec<WorkflowNode>) -> Result<()>;
    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<Workflow>;
    async fn get_nodes(&self, workflow_id: &str) -> Result<Vec<WorkflowNode>>;
    async fn list(&self, org_id: &str, limit: usize, offset: usize) -> Result<Vec<Workflow>>;
    async fn update_status(&self, id: &str, org_id: &str, status: WorkflowStatus) -> Result<()>;
    async fn set_temporal_ids(&self, id: &str, org_id: &str, workflow_id: String, run_id: String) -> Result<()>;
    async fn update_node_status(
        &self,
        node_id: &str,
        status: NodeStatus,
        err_msg: Option<String>,
        output: Option<Value>,
    ) -> Result<()>;
    async fn history(&self, workflow_id: &str) -> Result<Vec<NodeHistory>>;
}
