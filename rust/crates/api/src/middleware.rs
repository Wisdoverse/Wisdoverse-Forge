//! Tower middleware helpers.
//!
//! Provides pre-configured middleware layers for the Axum router.
//! Layers are applied bottom-up in the router, so `catch_panic_layer`
//! (applied last) becomes the outermost wrapper.

use std::any::Any;

use axum::Json;
use axum::http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::response::IntoResponse;
use tower_http::catch_panic::{CatchPanicLayer, ResponseForPanic};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::domain::system::internal_error_response;

/// CORS layer configured for the current environment.
///
/// - **Development:** fully permissive (any origin, any method, any header).
/// - **Production:** restricted to the configured `cors_origin`. If no origin
///   is configured, defaults to same-origin only (no external origins allowed)
///   and logs a warning.
pub fn cors_layer(is_production: bool, cors_origin: Option<&str>) -> CorsLayer {
    if is_production {
        let allow_origin = match cors_origin {
            Some(origin) => AllowOrigin::exact(HeaderValue::from_str(origin).unwrap_or_else(|_| {
                tracing::warn!(origin, "invalid CORS_ORIGIN value, falling back to restrictive default");
                HeaderValue::from_static("https://invalid.example.com")
            })),
            None => {
                tracing::warn!(
                    "CORS_ORIGIN not configured in production — defaulting to same-origin only (no external origins)"
                );
                // Empty list = no external origins allowed (same-origin only)
                AllowOrigin::list(std::iter::empty::<HeaderValue>())
            }
        };
        CorsLayer::new()
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::PATCH])
            .allow_headers([AUTHORIZATION, CONTENT_TYPE, ACCEPT])
            .allow_origin(allow_origin)
    } else {
        CorsLayer::permissive()
    }
}

/// HTTP request/response tracing layer.
///
/// Emits `tracing` spans for each request with method, URI, status code,
/// and latency. Integrates with the `tracing-subscriber` configured in `main`.
pub fn trace_layer() -> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>>
{
    TraceLayer::new_for_http()
}

/// Custom panic response handler that returns JSON error bodies.
#[derive(Clone)]
pub struct JsonPanicResponse;

impl ResponseForPanic for JsonPanicResponse {
    type ResponseBody = axum::body::Body;

    fn response_for_panic(&mut self, err: Box<dyn Any + Send + 'static>) -> axum::http::Response<Self::ResponseBody> {
        let detail = if let Some(s) = err.downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = err.downcast_ref::<&str>() {
            (*s).to_string()
        } else {
            "unknown panic".to_string()
        };
        tracing::error!(panic = %detail, "handler panicked");

        (StatusCode::INTERNAL_SERVER_ERROR, Json(internal_error_response())).into_response()
    }
}

/// Catch-panic layer — prevents a panicking handler from killing the server.
///
/// Returns a JSON 500 Internal Server Error instead of crashing the process.
/// Should be the outermost layer (applied last in the router chain).
pub fn catch_panic_layer() -> CatchPanicLayer<JsonPanicResponse> {
    CatchPanicLayer::custom(JsonPanicResponse)
}

// Issue #15 P4 removed `record_compat_hit`, `COMPAT_HITS_COUNTER`,
// `COMPAT_LATENCY_HISTOGRAM`, `DEPRECATION_SUNSET`, `DEPRECATION_LINK`,
// and `apply_deprecation_headers`. The legacy nav surface is now just
// "the nav surface" — canonical reads, no sunset date. See the P4
// merge commit for history.

// ---------------------------------------------------------------------------
// IdempotencyKey extractor
// ---------------------------------------------------------------------------

use agentforge_core::AppError;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::domain::agent::IdempotencyKeyPolicy;

/// Axum extractor for the `Idempotency-Key` request header.
///
/// Required on mutating endpoints that support idempotent replay.  Returns a
/// `400 VALIDATION_ERROR` when the header is absent, empty, or longer than 256
/// bytes.
///
/// # Usage
///
/// ```ignore
/// async fn enroll(idempotency: IdempotencyKey, ...) -> AppResult<Json<EnrollResponse>> {
///     let key: &str = &idempotency.0;
///     // pass key to the idempotency store before executing the action
/// }
/// ```
#[derive(Debug)]
pub struct IdempotencyKey(pub String);

impl<S> FromRequestParts<S> for IdempotencyKey
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .headers
            .get("idempotency-key")
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .map(|s| IdempotencyKey(s.to_string()))
            .ok_or_else(IdempotencyKeyPolicy::missing_header_error)
    }
}

#[cfg(test)]
mod idempotency_tests {
    use super::*;
    use agentforge_core::ErrorKind;
    use axum::http::Request;

    #[tokio::test]
    async fn idempotency_key_extractor_reads_header() {
        let req = Request::builder().header("Idempotency-Key", "abc-123").body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let key = IdempotencyKey::from_request_parts(&mut parts, &()).await.unwrap();
        assert_eq!(key.0, "abc-123");
    }

    #[tokio::test]
    async fn idempotency_key_extractor_rejects_missing() {
        let req = Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let res = IdempotencyKey::from_request_parts(&mut parts, &()).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn idempotency_key_extractor_rejects_empty() {
        let req = Request::builder().header("Idempotency-Key", "").body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let res = IdempotencyKey::from_request_parts(&mut parts, &()).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn idempotency_key_extractor_rejects_oversized() {
        let big = "x".repeat(257);
        let req = Request::builder().header("Idempotency-Key", big.as_str()).body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let res = IdempotencyKey::from_request_parts(&mut parts, &()).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn idempotency_key_extractor_accepts_exactly_256_chars() {
        let exact = "a".repeat(256);
        let req = Request::builder().header("Idempotency-Key", exact.as_str()).body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let key = IdempotencyKey::from_request_parts(&mut parts, &()).await.unwrap();
        assert_eq!(key.0.len(), 256);
    }

    #[tokio::test]
    async fn idempotency_key_rejection_is_validation_error() {
        let req = Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let err = IdempotencyKey::from_request_parts(&mut parts, &()).await.unwrap_err();
        assert!(matches!(err.kind, ErrorKind::ValidationWithCode { .. }));
    }

    #[tokio::test]
    async fn idempotency_key_rejection_carries_i18n_code() {
        let req = Request::builder().body(()).unwrap();
        let (mut parts, _) = req.into_parts();
        let err = IdempotencyKey::from_request_parts(&mut parts, &()).await.unwrap_err();
        match err.kind {
            ErrorKind::ValidationWithCode { code, .. } => {
                assert_eq!(code, "errors.agent.enroll.missing_idempotency_key");
            }
            other => panic!("expected ValidationWithCode, got {other:?}"),
        }
    }
}
