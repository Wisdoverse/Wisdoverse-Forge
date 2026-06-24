use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Error, Deserialize)]
#[error("{code}: {message}")]
pub struct ApiError {
    #[serde(rename = "error")]
    pub code: String,
    #[serde(default)]
    pub message: String,
    #[serde(skip, default)]
    pub status: u16,
}

impl ApiError {
    /// Maps an API error code to a process exit code.
    /// Matches `cli/internal/client/errors.go:ExitCodeForError` exactly.
    pub fn exit_code_for(code: &str) -> i32 {
        match code {
            "UNAUTHORIZED" | "INVALID_API_KEY" => 3,
            "NOT_FOUND" => 4,
            "FORBIDDEN" => 5,
            "VALIDATION_ERROR" => 2,
            _ => 1,
        }
    }

    pub fn exit_code(&self) -> i32 {
        Self::exit_code_for(&self.code)
    }
}
