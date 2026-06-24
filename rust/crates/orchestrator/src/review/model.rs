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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_at: Option<DateTime<Utc>>,
    /// Set by the escalation reaper (#871) when an overdue review past its grace
    /// window is escalated. NULL = not yet escalated; a timestamp = escalated once
    /// (the idempotency guard). Never written by the verdict state machine.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escalated_at: Option<DateTime<Utc>>,
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

/// Returns `true` if transitioning from `from` to `to` is a legal state-machine step.
///
/// Legal transitions:
/// - `Pending -> InReview`
/// - `Pending -> Approved | ChangesRequested | Rejected`
/// - `InReview -> Approved | ChangesRequested | Rejected`
///
/// Terminal states (`Approved`, `ChangesRequested`, `Rejected`) accept no outbound transitions.
pub fn can_transition(from: ReviewState, to: ReviewState) -> bool {
    matches!(
        (from, to),
        (ReviewState::Pending, ReviewState::InReview)
            | (ReviewState::Pending, ReviewState::Approved)
            | (ReviewState::Pending, ReviewState::ChangesRequested)
            | (ReviewState::Pending, ReviewState::Rejected)
            | (ReviewState::InReview, ReviewState::Approved)
            | (ReviewState::InReview, ReviewState::ChangesRequested)
            | (ReviewState::InReview, ReviewState::Rejected)
    )
}

#[derive(Debug, Clone)]
pub struct ReviewFilter {
    pub org_id: String,
    pub task_id: Option<String>,
    pub state: Option<ReviewState>,
    pub limit: usize,
    pub offset: usize,
}

#[cfg(test)]
mod tests {
    use super::{ReviewState, can_transition};

    #[test]
    fn pending_to_in_review_allowed() {
        assert!(can_transition(ReviewState::Pending, ReviewState::InReview));
    }

    #[test]
    fn pending_to_approved_allowed() {
        assert!(can_transition(ReviewState::Pending, ReviewState::Approved));
    }

    #[test]
    fn pending_to_changes_requested_allowed() {
        assert!(can_transition(ReviewState::Pending, ReviewState::ChangesRequested));
    }

    #[test]
    fn pending_to_rejected_allowed() {
        assert!(can_transition(ReviewState::Pending, ReviewState::Rejected));
    }

    #[test]
    fn in_review_to_approved_allowed() {
        assert!(can_transition(ReviewState::InReview, ReviewState::Approved));
    }

    #[test]
    fn in_review_to_changes_requested_allowed() {
        assert!(can_transition(ReviewState::InReview, ReviewState::ChangesRequested));
    }

    #[test]
    fn in_review_to_rejected_allowed() {
        assert!(can_transition(ReviewState::InReview, ReviewState::Rejected));
    }

    #[test]
    fn in_review_to_pending_rejected() {
        assert!(!can_transition(ReviewState::InReview, ReviewState::Pending));
    }

    #[test]
    fn approved_to_anything_rejected() {
        assert!(!can_transition(ReviewState::Approved, ReviewState::Pending));
        assert!(!can_transition(ReviewState::Approved, ReviewState::InReview));
        assert!(!can_transition(ReviewState::Approved, ReviewState::ChangesRequested));
        assert!(!can_transition(ReviewState::Approved, ReviewState::Rejected));
    }

    #[test]
    fn changes_requested_to_anything_rejected() {
        assert!(!can_transition(ReviewState::ChangesRequested, ReviewState::Pending));
        assert!(!can_transition(ReviewState::ChangesRequested, ReviewState::InReview));
        assert!(!can_transition(ReviewState::ChangesRequested, ReviewState::Approved));
        assert!(!can_transition(ReviewState::ChangesRequested, ReviewState::Rejected));
    }

    #[test]
    fn rejected_to_anything_rejected() {
        assert!(!can_transition(ReviewState::Rejected, ReviewState::Pending));
        assert!(!can_transition(ReviewState::Rejected, ReviewState::InReview));
        assert!(!can_transition(ReviewState::Rejected, ReviewState::Approved));
        assert!(!can_transition(ReviewState::Rejected, ReviewState::ChangesRequested));
    }

    #[test]
    fn self_transitions_rejected() {
        assert!(!can_transition(ReviewState::Pending, ReviewState::Pending));
        assert!(!can_transition(ReviewState::InReview, ReviewState::InReview));
        assert!(!can_transition(ReviewState::Approved, ReviewState::Approved));
    }
}
