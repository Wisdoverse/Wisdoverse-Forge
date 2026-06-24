use thiserror::Error;

pub type Result<T> = std::result::Result<T, ReviewError>;

#[derive(Debug, Error)]
pub enum ReviewError {
    #[error("review not found")]
    NotFound,
    #[error("invalid review request: {0}")]
    InvalidInput(String),
    #[error("internal review error: {0}")]
    Internal(String),
}
