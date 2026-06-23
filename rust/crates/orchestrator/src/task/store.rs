use async_trait::async_trait;

use super::errors::Result;
use super::model::{Task, TaskDispatch, TaskFilter, TaskState, UpdateTaskRequest};

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

    /// Insert a new dispatch record with status='queued'. Returns the new dispatch id.
    async fn create_dispatch(&self, task_id: &str, org_id: &str) -> Result<String>;

    /// Update the status (and optionally last_error / session_id) of a dispatch record.
    async fn update_dispatch(
        &self,
        dispatch_id: &str,
        org_id: &str,
        status: &str,
        last_error: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<()>;

    /// Fetch the most recent dispatch record for a task (by org_id + task_id).
    async fn get_dispatch(&self, task_id: &str, org_id: &str) -> Result<TaskDispatch>;
}
