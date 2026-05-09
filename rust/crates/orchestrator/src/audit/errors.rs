#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, AuditError>;
