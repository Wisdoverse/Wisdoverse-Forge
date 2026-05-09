#[derive(Debug, thiserror::Error)]
pub enum TeamError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, TeamError>;
