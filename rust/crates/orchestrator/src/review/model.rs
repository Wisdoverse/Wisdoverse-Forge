use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    Pending,
    InReview,
    Approved,
    ChangesRequested,
    Rejected,
}

impl ReviewState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InReview => "in_review",
            Self::Approved => "approved",
            Self::ChangesRequested => "changes_requested",
            Self::Rejected => "rejected",
        }
    }
}

impl fmt::Display for ReviewState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ReviewState {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "in_review" => Ok(Self::InReview),
            "approved" => Ok(Self::Approved),
            "changes_requested" => Ok(Self::ChangesRequested),
            "rejected" => Ok(Self::Rejected),
            other => Err(format!("unsupported review state: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeReview {
    pub id: String,
    pub task_id: String,
    pub session_id: String,
    pub diff_ref: String,
    pub diff_snapshot: Option<Value>,
    pub state: ReviewState,
    pub assigned_to: Option<String>,
    pub org_id: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewComment {
    pub id: String,
    pub review_id: String,
    pub author_id: String,
    pub body: String,
    pub file_path: Option<String>,
    pub line: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewWithComments {
    #[serde(flatten)]
    pub review: CodeReview,
    #[serde(default)]
    pub comments: Vec<ReviewComment>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReviewRequest {
    pub task_id: String,
    pub session_id: String,
    #[serde(default)]
    pub diff_ref: String,
    pub assigned_to: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCommentRequest {
    pub body: String,
    pub file_path: Option<String>,
    pub line: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct ReviewFilter {
    pub org_id: String,
    pub task_id: Option<String>,
    pub state: Option<ReviewState>,
    pub limit: usize,
    pub offset: usize,
}
