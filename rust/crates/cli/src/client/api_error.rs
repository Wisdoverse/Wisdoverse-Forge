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

/// API error codes with a dedicated process exit code; every other code exits 1.
/// Single source for `exit_code_for`, the root `--help` text and the exit-code
/// table in docs/guides/cli-platform-support.md (kept in sync by tests).
pub const API_ERROR_EXIT_CODES: &[(&str, i32)] =
    &[("VALIDATION_ERROR", 2), ("UNAUTHORIZED", 3), ("INVALID_API_KEY", 3), ("NOT_FOUND", 4), ("FORBIDDEN", 5)];

impl ApiError {
    /// Maps an API error code to a process exit code.
    /// Matches `cli/internal/client/errors.go:ExitCodeForError` exactly.
    pub fn exit_code_for(code: &str) -> i32 {
        API_ERROR_EXIT_CODES.iter().find(|(known, _)| *known == code).map_or(1, |(_, exit)| *exit)
    }

    pub fn exit_code(&self) -> i32 {
        Self::exit_code_for(&self.code)
    }
}
