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
}
