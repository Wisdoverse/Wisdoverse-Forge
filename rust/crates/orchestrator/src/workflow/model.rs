use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Draft,
    Running,
    Completed,
    Failed,
    Cancelled,
    Paused,
}

impl WorkflowStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Paused => "paused",
        }
    }
}

impl FromStr for WorkflowStatus {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "paused" => Ok(Self::Paused),
            _ => Err(format!("invalid workflow status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    AgentTask,
    HumanReview,
    Gate,
}

impl NodeType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentTask => "agent_task",
            Self::HumanReview => "human_review",
            Self::Gate => "gate",
        }
    }
}

impl FromStr for NodeType {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "agent_task" => Ok(Self::AgentTask),
            "human_review" => Ok(Self::HumanReview),
            "gate" => Ok(Self::Gate),
            _ => Err(format!("invalid node type: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

impl NodeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

impl FromStr for NodeStatus {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "skipped" => Ok(Self::Skipped),
            _ => Err(format!("invalid node status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Approve,
    Reject,
}

impl Decision {
    pub fn audit_action(self) -> crate::audit::AuditAction {
        match self {
            Decision::Approve => crate::audit::AuditAction::WorkflowReviewApprove,
            Decision::Reject => crate::audit::AuditAction::WorkflowReviewReject,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: WorkflowStatus,
    pub org_id: String,
    pub created_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporal_workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporal_run_id: Option<String>,
    /// Server-assigned id of the Temporal namespace the execution was started
    /// in. Proves a later "not found" came from the same Temporal (#1352).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal_namespace_id: Option<String>,
    /// Why the current status was set by reconciliation instead of by the
    /// workflow itself (#1352): `execution_lost`, `execution_missing` or
    /// `reconciled_from_temporal`. Cleared by the next normal status change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_cause: Option<String>,
    /// When Temporal was first seen answering "not found" for this workflow's
    /// execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_missing_since: Option<DateTime<Utc>>,
    /// When the reconciler last got an answer from Temporal about this row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_checked_at: Option<DateTime<Utc>>,
    /// The reconciler's last answer from Temporal for this row: `running`,
    /// `closed`, `missing` or `unverifiable`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_observation: Option<String>,
    /// When reconciliation last set this workflow's status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciled_at: Option<DateTime<Utc>>,
    /// Compare-and-set token (#1352): the database trigger increments it on
    /// every change except reconciler bookkeeping. Internal, not serialized.
    #[serde(default, skip_serializing)]
    pub state_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Cause written when the reconciler fails a workflow whose execution stayed
/// missing for the whole grace period.
pub const CAUSE_EXECUTION_LOST: &str = "execution_lost";
/// Cause written when a cancel found no execution left to cancel.
pub const CAUSE_EXECUTION_MISSING: &str = "execution_missing";
/// Cause written when the row is brought in line with a closed execution.
pub const CAUSE_RECONCILED_FROM_TEMPORAL: &str = "reconciled_from_temporal";

/// What Temporal said about the CURRENT execution behind a workflow row
/// (#1352). A failed lookup is an error, never one of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionObservation {
    /// An execution exists and is not closed (running, paused, or a status
    /// this code does not know). Leave the row alone.
    Running,
    /// The current execution closed with a terminal outcome. `first_run_id`
    /// is the first run of its chain (continue-as-new, retry, reset).
    Closed { run_id: String, first_run_id: String, status: WorkflowStatus },
    /// Temporal answered "workflow execution not found", and the namespace it
    /// answered for is proven to be the one the workflow was started in (or
    /// the operator acknowledged the current Temporal).
    Missing,
    /// "Not found", but nothing proves this is the Temporal the workflow was
    /// started in. Never acted on.
    Unverifiable,
}

/// Execution state returned by a workflow status read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Running,
    Missing,
    Closed,
    Unverifiable,
    /// Temporal could not be asked in time.
    Unknown,
}

/// The one rule for "this execution is the row's" (#1352): it belongs to the
/// execution chain the row recorded when the row's recorded run id (the run
/// returned at start) is the execution's first run id or its own run id.
/// Used by the reconciler for closed executions and by the finalizer fence.
pub fn belongs_to_recorded_chain(recorded_run_id: Option<&str>, run_id: &str, first_run_id: &str) -> bool {
    recorded_run_id.is_some_and(|recorded| !recorded.is_empty() && (recorded == run_id || recorded == first_run_id))
}

impl ExecutionObservation {
    /// Value stored in `execution_observation`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Closed { .. } => "closed",
            Self::Missing => "missing",
            Self::Unverifiable => "unverifiable",
        }
    }
}

/// How a reconciliation check changes `execution_missing_since`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingMark {
    Keep,
    Clear,
    Set,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowNode {
    pub id: String,
    pub workflow_id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: NodeType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<Value>,
    pub position: i32,
    pub status: NodeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkflowRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub nodes: Vec<NodeRequest>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeRequest {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub node_type: Option<NodeType>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub config: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalRequest {
    pub node_id: Option<String>,
    pub decision: Option<Decision>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeHistory {
    pub node_id: String,
    pub node_name: String,
    pub node_type: NodeType,
    pub status: NodeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
