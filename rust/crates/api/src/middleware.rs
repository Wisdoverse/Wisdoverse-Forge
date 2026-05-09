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
use serde_json::json;
use tower_http::catch_panic::{CatchPanicLayer, ResponseForPanic};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

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

        let body = json!({
            "ok": false,
            "error": {
                "code": "INTERNAL_ERROR",
                "message": "Internal server error"
            }
        });
        (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
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
