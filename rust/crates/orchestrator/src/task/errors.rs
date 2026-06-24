use thiserror::Error;

pub type Result<T> = std::result::Result<T, TaskError>;

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("task not found")]
    NotFound,
    #[error("invalid task request: {0}")]
    InvalidInput(String),
    #[error("internal task error: {0}")]
    Internal(String),
}
