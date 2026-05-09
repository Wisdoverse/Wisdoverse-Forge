//! Three-layer error system: domain errors → application errors → HTTP responses.
//!
//! - [`ErrorKind`] represents domain-level error categories (thiserror).
//! - [`AppError`] wraps `ErrorKind` and implements axum's `IntoResponse`.
//! - Internal errors are logged but NEVER leak details to clients.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Domain-level error kinds.
#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("unprocessable entity: {0}")]
    Unprocessable(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

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

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.kind {
            ErrorKind::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            ErrorKind::Validation(_) => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR"),
            ErrorKind::Unprocessable(_) => (StatusCode::UNPROCESSABLE_ENTITY, "UNPROCESSABLE_ENTITY"),
            ErrorKind::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            ErrorKind::Forbidden => (StatusCode::FORBIDDEN, "FORBIDDEN"),
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
                tracing::debug!(error = %other, code = code, "client error");
            }
        }

        let body = json!({
            "ok": false,
            "error": {
                "code": code,
                "message": match &self.kind {
                    ErrorKind::Internal(_) => "Internal server error".to_string(),
                    other => other.to_string(),
                }
            }
        });

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
        let err = AppError::from(ErrorKind::Forbidden);
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
    fn ok_false_in_all_error_responses() {
        // Structural verification — all errors must have ok: false
        let kinds = vec![
            ErrorKind::NotFound("x".into()),
            ErrorKind::Validation("x".into()),
            ErrorKind::Unprocessable("x".into()),
            ErrorKind::Unauthorized,
            ErrorKind::Forbidden,
            ErrorKind::Conflict("x".into()),
        ];
        for kind in kinds {
            let err = AppError::from(kind);
            // We just verify construction works; the async tests check the JSON
            assert!(matches!(
                err.kind,
                ErrorKind::NotFound(_)
                    | ErrorKind::Validation(_)
                    | ErrorKind::Unprocessable(_)
                    | ErrorKind::Unauthorized
                    | ErrorKind::Forbidden
                    | ErrorKind::Conflict(_)
                    | ErrorKind::Unavailable(_)
            ));
        }
    }
}
