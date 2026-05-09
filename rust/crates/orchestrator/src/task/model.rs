use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Pending,
    Assigned,
    Working,
    Review,
    Completed,
    Failed,
    ChangesRequested,
}

impl TaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Assigned => "assigned",
            Self::Working => "working",
            Self::Review => "review",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::ChangesRequested => "changes_requested",
        }
    }
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TaskState {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "assigned" => Ok(Self::Assigned),
            "working" => Ok(Self::Working),
            "review" => Ok(Self::Review),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "changes_requested" => Ok(Self::ChangesRequested),
            other => Err(format!("unsupported task state: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl TaskPriority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }
}

impl fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TaskPriority {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "low" => Ok(Self::Low),
            "normal" => Ok(Self::Normal),
            "high" => Ok(Self::High),
            "urgent" => Ok(Self::Urgent),
            other => Err(format!("unsupported task priority: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub workflow_id: Option<String>,
    pub title: String,
    pub description: String,
    pub state: TaskState,
    pub priority: TaskPriority,
    pub assigned_to: Option<String>,
    pub review_id: Option<String>,
    pub agentforge_session_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub created_by: String,
    pub org_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub priority: Option<TaskPriority>,
    pub assigned_to: Option<String>,
    pub workflow_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub agent_provider: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<TaskPriority>,
    pub assigned_to: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionRequest {
    pub state: TaskState,
    #[allow(dead_code)]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignRequest {
    pub participant_id: String,
    pub agent_provider: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TaskFilter {
    pub org_id: String,
    pub state: Option<TaskState>,
    pub assigned_to: Option<String>,
    pub limit: usize,
    pub offset: usize,
}
