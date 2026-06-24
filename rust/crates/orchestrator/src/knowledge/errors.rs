use thiserror::Error;

pub type Result<T> = std::result::Result<T, KnowledgeError>;

#[derive(Debug, Error)]
pub enum KnowledgeError {
    #[error("knowledge entry not found")]
    NotFound,

    #[error("invalid knowledge request: {0}")]
    InvalidInput(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

impl KnowledgeError {
    pub fn not_found() -> Self {
        Self::NotFound
    }
}
