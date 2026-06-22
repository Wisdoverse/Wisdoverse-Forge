use async_trait::async_trait;

use super::errors::Result;
use super::model::{CodeReview, ReviewComment, ReviewFilter, ReviewState, ReviewWithComments};

#[async_trait]
pub trait Store: Send + Sync {
    async fn create(&self, review: &mut CodeReview) -> Result<()>;
    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<ReviewWithComments>;
    async fn list(&self, filter: ReviewFilter) -> Result<Vec<CodeReview>>;
    async fn update_state(&self, id: &str, org_id: &str, state: ReviewState) -> Result<()>;
    async fn add_comment(&self, review_id: &str, org_id: &str, comment: &mut ReviewComment) -> Result<()>;

    /// Atomically apply a review verdict and update the linked task state in a single
    /// database transaction.  Both the `code_reviews` and `tasks` rows are updated
    /// under the same transaction so that a partial failure cannot leave the review
    /// approved while the task remains in its prior state (or vice-versa).
    ///
    /// Returns `ReviewError::NotFound` if either the review or the task row is
    /// absent for the given `org_id`, rolling back the transaction so neither row
    /// is mutated.
    async fn apply_verdict(
        &self,
        review_id: &str,
        org_id: &str,
        new_review_state: ReviewState,
        task_id: &str,
        new_task_state: crate::task::TaskState,
    ) -> Result<()>;
}
