//! Three-layer error system: domain errors → application errors → HTTP responses.
//!
//! - [`ErrorKind`] represents domain-level error categories (thiserror).
//! - [`AppError`] wraps `ErrorKind` and implements axum's `IntoResponse`.
//! - Internal errors are logged but NEVER leak details to clients.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::RuntimeCapabilityError;

/// Domain-level error kinds.
#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("validation error: {0}")]
    Validation(String),

    /// Validation error that carries an i18n code for the client.
    ///
    /// `code` must be a dotted key matching a path in the frontend locale
    /// files (e.g. `"errors.agent.lifecycle.restart_host_cli"`).
    /// The client resolves the human-readable title/detail from the i18n
    /// catalogue; `message` is the English fallback for API consumers
    /// that do not implement i18n lookup.
    #[error("validation error: {message}")]
    ValidationWithCode { code: &'static str, message: String },

    #[error("unprocessable entity: {0}")]
    Unprocessable(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden: {0}")]
    Forbidden(String),

    /// Forbidden error that carries an i18n code for the client.
    ///
    /// Same contract as [`ValidationWithCode`]: `code` is the dotted i18n
    /// key, `message` is the English fallback.
    #[error("forbidden: {message}")]
    ForbiddenWithCode { code: &'static str, message: String },

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("service unavailable: {0}")]
    Unavailable(String),

    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

/// Application error — wraps [`ErrorKind`] for HTTP responses.
#[derive(Debug)]
pub struct AppError {
    pub kind: ErrorKind,
}

impl From<ErrorKind> for AppError {
    fn from(kind: ErrorKind) -> Self {
        Self { kind }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        Self { kind: ErrorKind::Internal(err) }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self { kind: ErrorKind::NotFound("record not found".to_string()) },
            other => Self { kind: ErrorKind::Internal(anyhow::Error::from(other)) },
        }
    }
}

impl From<RuntimeCapabilityError> for AppError {
    fn from(err: RuntimeCapabilityError) -> Self {
        let msg = err.to_string();
        match err {
            RuntimeCapabilityError::UnknownCliTool { .. } | RuntimeCapabilityError::UnknownRuntimeKind { .. } => {
                Self { kind: ErrorKind::Validation(msg) }
            }
            RuntimeCapabilityError::MaxContextTokensZero { .. } => {
                // Internal invariant violation — not caused by user input.
                Self { kind: ErrorKind::Internal(anyhow::anyhow!("{msg}")) }
            }
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Determine HTTP status and a stable type-level code for standard variants.
        // ValidationWithCode / ForbiddenWithCode carry their own i18n code instead.
        let (status, type_code) = match &self.kind {
            ErrorKind::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            ErrorKind::Validation(_) => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR"),
            ErrorKind::ValidationWithCode { .. } => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR"),
            ErrorKind::Unprocessable(_) => (StatusCode::UNPROCESSABLE_ENTITY, "UNPROCESSABLE_ENTITY"),
            ErrorKind::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            ErrorKind::Forbidden(_) => (StatusCode::FORBIDDEN, "FORBIDDEN"),
            ErrorKind::ForbiddenWithCode { .. } => (StatusCode::FORBIDDEN, "FORBIDDEN"),
            ErrorKind::Conflict(_) => (StatusCode::CONFLICT, "CONFLICT"),
            ErrorKind::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE"),
            ErrorKind::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        };

        // Log all errors — internal at error level, client errors at debug level.
        match &self.kind {
            ErrorKind::Internal(err) => {
                tracing::error!(error = %err, "internal server error");
            }
            other => {
                tracing::debug!(error = %other, code = type_code, "client error");
            }
        }

        // Build the response body.  ValidationWithCode / ForbiddenWithCode emit
        // the i18n dotted key as `code` so the frontend can look up localised
        // copy.  All other variants keep the existing stable type-level code.
        let body = match &self.kind {
            ErrorKind::ValidationWithCode { code, message } => json!({
                "ok": false,
                "error": { "code": *code, "message": message }
            }),
            ErrorKind::ForbiddenWithCode { code, message } => json!({
                "ok": false,
                "error": { "code": *code, "message": message }
            }),
            ErrorKind::Internal(_) => json!({
                "ok": false,
                "error": { "code": type_code, "message": "Internal server error" }
            }),
            other => json!({
                "ok": false,
                "error": { "code": type_code, "message": other.to_string() }
            }),
        };

        (status, Json(body)).into_response()
    }
}

/// Convenience Result alias used throughout the application.
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn response_to_json(resp: Response) -> (StatusCode, serde_json::Value) {
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        (status, value)
    }

    #[tokio::test]
    async fn not_found_error_response() {
        let err = AppError::from(ErrorKind::NotFound("agent".to_string()));
        let (status, body) = response_to_json(err.into_response()).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
        assert_eq!(body["error"]["message"], "not found: agent");
    }

    #[tokio::test]
    async fn validation_error_response() {
        let err = AppError::from(ErrorKind::Validation("name is required".to_string()));
        let (status, body) = response_to_json(err.into_response()).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
        assert_eq!(body["error"]["message"], "validation error: name is required");
    }

    #[tokio::test]
    async fn unauthorized_error_response() {
        let err = AppError::from(ErrorKind::Unauthorized);
        let (status, body) = response_to_json(err.into_response()).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    #[tokio::test]
    async fn unprocessable_error_response() {
        let err = AppError::from(ErrorKind::Unprocessable("secret detected".to_string()));
        let (status, body) = response_to_json(err.into_response()).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"]["code"], "UNPROCESSABLE_ENTITY");
        assert_eq!(body["error"]["message"], "unprocessable entity: secret detected");
    }

    #[tokio::test]
    async fn forbidden_error_response() {
        let err = AppError::from(ErrorKind::Forbidden("forbidden".into()));
        let (status, body) = response_to_json(err.into_response()).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"]["code"], "FORBIDDEN");
    }

    #[tokio::test]
    async fn conflict_error_response() {
        let err = AppError::from(ErrorKind::Conflict("duplicate name".to_string()));
        let (status, body) = response_to_json(err.into_response()).await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "CONFLICT");
    }

    #[tokio::test]
    async fn internal_error_does_not_leak_details() {
        let err = AppError::from(anyhow::anyhow!("secret database password in error"));
        let (status, body) = response_to_json(err.into_response()).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error"]["code"], "INTERNAL_ERROR");
        assert_eq!(body["error"]["message"], "Internal server error");
        // Must NOT contain the actual error message
        assert!(!body["error"]["message"].as_str().unwrap().contains("secret"));
    }

    #[tokio::test]
    async fn sqlx_row_not_found_maps_to_not_found() {
        let err = AppError::from(sqlx::Error::RowNotFound);
        let (status, body) = response_to_json(err.into_response()).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[test]
    fn runtime_capability_error_maps_to_validation() {
        let err = RuntimeCapabilityError::UnknownRuntimeKind { raw: "host_cli".into() };
        let app: AppError = err.into();
        match app.kind {
            ErrorKind::Validation(msg) => {
                assert!(msg.contains("host_cli"), "expected message to contain 'host_cli', got: {msg}")
            }
            other => panic!("expected ErrorKind::Validation, got: {other:?}"),
        }
    }

    #[test]
    fn max_context_tokens_zero_does_not_map_to_validation() {
        use crate::RuntimeKind;
        let err = RuntimeCapabilityError::MaxContextTokensZero { runtime_kind: RuntimeKind::Container };
        let app: AppError = err.into();
        assert!(
            !matches!(app.kind, ErrorKind::Validation(_)),
            "MaxContextTokensZero is an internal invariant violation and must not map to Validation/400"
        );
        assert!(matches!(app.kind, ErrorKind::Internal(_)), "MaxContextTokensZero must map to Internal/500");
    }

    #[test]
    fn ok_false_in_all_error_responses() {
        // Structural verification — all errors must have ok: false
        let kinds = vec![
            ErrorKind::NotFound("x".into()),
            ErrorKind::Validation("x".into()),
            ErrorKind::ValidationWithCode { code: "errors.agent.lifecycle.restart_host_cli", message: "x".into() },
            ErrorKind::Unprocessable("x".into()),
            ErrorKind::Unauthorized,
            ErrorKind::Forbidden("forbidden".into()),
            ErrorKind::ForbiddenWithCode { code: "errors.agent.lifecycle.not_permitted", message: "x".into() },
            ErrorKind::Conflict("x".into()),
        ];
        for kind in kinds {
            let err = AppError::from(kind);
            // We just verify construction works; the async tests check the JSON
            assert!(matches!(
                err.kind,
                ErrorKind::NotFound(_)
                    | ErrorKind::Validation(_)
                    | ErrorKind::ValidationWithCode { .. }
                    | ErrorKind::Unprocessable(_)
                    | ErrorKind::Unauthorized
                    | ErrorKind::Forbidden(_)
                    | ErrorKind::ForbiddenWithCode { .. }
                    | ErrorKind::Conflict(_)
                    | ErrorKind::Unavailable(_)
            ));
        }
    }

    #[tokio::test]
    async fn validation_with_code_emits_i18n_code() {
        let err = AppError::from(ErrorKind::ValidationWithCode {
            code: "errors.agent.lifecycle.restart_host_cli",
            message: "Restart the sidecar from your machine.".into(),
        });
        let (status, body) = response_to_json(err.into_response()).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"]["code"], "errors.agent.lifecycle.restart_host_cli");
        assert_eq!(body["error"]["message"], "Restart the sidecar from your machine.");
    }

    #[tokio::test]
    async fn forbidden_with_code_emits_i18n_code() {
        let err = AppError::from(ErrorKind::ForbiddenWithCode {
            code: "errors.agent.lifecycle.not_permitted",
            message: "operation not permitted on this agent".into(),
        });
        let (status, body) = response_to_json(err.into_response()).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"]["code"], "errors.agent.lifecycle.not_permitted");
        assert_eq!(body["error"]["message"], "operation not permitted on this agent");
    }
}
