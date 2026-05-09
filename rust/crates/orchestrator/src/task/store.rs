use async_trait::async_trait;

use super::errors::Result;
use super::model::{Task, TaskFilter, TaskState, UpdateTaskRequest};

#[async_trait]
pub trait Store: Send + Sync {
    async fn create(&self, task: &mut Task) -> Result<()>;
    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<Task>;
    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>>;
    async fn update(&self, id: &str, org_id: &str, req: UpdateTaskRequest) -> Result<()>;
    async fn update_state(&self, id: &str, org_id: &str, state: TaskState) -> Result<()>;
    async fn set_assignee(&self, id: &str, org_id: &str, participant_id: Option<String>) -> Result<()>;
    async fn set_session_id(&self, id: &str, org_id: &str, session_id: String) -> Result<()>;
    async fn set_review_id(&self, id: &str, org_id: &str, review_id: String) -> Result<()>;
    async fn assign(&self, id: &str, org_id: &str, participant_id: String, state: TaskState) -> Result<()>;
}
