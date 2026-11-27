use async_trait::async_trait;
use serde_json::Value;

use super::errors::Result;
use super::model::{MissingMark, NodeHistory, NodeStatus, Workflow, WorkflowNode, WorkflowStatus};

#[async_trait]
pub trait Store: Send + Sync {
    async fn create(&self, workflow: &mut Workflow, nodes: &mut Vec<WorkflowNode>) -> Result<()>;
    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<Workflow>;
    async fn get_nodes(&self, workflow_id: &str) -> Result<Vec<WorkflowNode>>;
    async fn list(&self, org_id: &str, limit: usize, offset: usize) -> Result<Vec<Workflow>>;
    async fn update_status(&self, id: &str, org_id: &str, status: WorkflowStatus) -> Result<()>;
    /// Record a freshly started execution. Also clears the reconciliation
    /// metadata of any earlier run (#1352). `namespace_id` is the
    /// server-assigned id of the Temporal namespace, when it could be read.
    async fn set_temporal_ids(
        &self,
        id: &str,
        org_id: &str,
        workflow_id: String,
        run_id: String,
        namespace_id: Option<String>,
    ) -> Result<()>;
    /// Finalizer write fenced by execution identity (#1352): only applies when
    /// the row still records `run_id` (the first run of the finalizing chain),
    /// so a late finalizer of an earlier run cannot overwrite a re-run.
    /// Returns false when the row belongs to another run.
    async fn update_status_for_run(&self, id: &str, org_id: &str, run_id: &str, status: WorkflowStatus)
    -> Result<bool>;
    /// Reconciliation candidates across all organizations: `running` rows with
    /// a Temporal execution, never-checked first, then least recently checked.
    async fn reconcile_candidates(&self, limit: usize) -> Result<Vec<Workflow>>;
    /// Reconciler bookkeeping: record that Temporal answered `observation`
    /// about `workflow` (`execution_checked_at`, `execution_observation`) and
    /// adjust the missing mark. Compare-and-set on organization, status,
    /// execution ids and `state_version`; returns false when the row changed.
    /// Only the reconciler writes the check time, so reads cannot reorder it.
    async fn record_execution_check(&self, workflow: &Workflow, mark: MissingMark, observation: &str) -> Result<bool>;
    /// Status read saw a proven-missing execution: set `execution_missing_since`
    /// if it is not set yet, under the same compare-and-set. Nothing else.
    async fn mark_execution_missing(&self, workflow: &Workflow) -> Result<bool>;
    /// Normal status change fenced by execution identity and `state_version`
    /// (used after a successful cancel), so it cannot overwrite a re-run or a
    /// transition that happened after `workflow` was read.
    async fn set_status_if_unchanged(&self, workflow: &Workflow, status: WorkflowStatus) -> Result<bool>;
    /// Set a reconciled terminal status with its cause, under the same
    /// compare-and-set as [`Store::record_execution_check`]. With
    /// `missing_for_secs`, it also requires the execution to have been missing
    /// at least that long. Returns false when nothing was written.
    async fn reconcile_status(
        &self,
        workflow: &Workflow,
        status: WorkflowStatus,
        cause: &str,
        missing_for_secs: Option<u64>,
    ) -> Result<bool>;
    async fn update_node_status(
        &self,
        node_id: &str,
        status: NodeStatus,
        err_msg: Option<String>,
        output: Option<Value>,
    ) -> Result<()>;
    /// Reset every node of a workflow back to `pending`, clearing
    /// started_at/completed_at/error/output. Used on the re-run-from-failed path
    /// so persisted node state agrees with the fresh Temporal run (F041).
    async fn reset_nodes(&self, workflow_id: &str, org_id: &str) -> Result<()>;
    async fn history(&self, workflow_id: &str) -> Result<Vec<NodeHistory>>;
}
