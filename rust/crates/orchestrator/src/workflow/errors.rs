#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    Unavailable(String),
    /// Temporal has no running execution for this workflow (#1352). The client
    /// can fix this by reloading the workflow status; it is not a server fault.
    #[error("workflow has no running execution")]
    ExecutionNotRunning,
    #[error("{0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, WorkflowError>;
