//! Request-ID correlation middleware (MS-1, distributed tracing).
//!
//! Mirror of `agentforge_orchestrator::observability::request_id` (same way the
//! HTTP-metrics middleware is mirrored across the two services), so the API and
//! orchestrator correlate requests identically. Every request gets a stable id:
//!
//! - bound to a `tracing` span (`request_id=…`) so every log line emitted while
//!   handling the request carries it — `grep` one id to follow a request through
//!   the API's JSON logs;
//! - echoed back in the `x-request-id` response header so a caller (browser,
//!   gateway, or — in a later slice — the cross-service hop) can correlate too.
//!
//! The id is an inbound `x-request-id` when present and SAFE, otherwise a fresh
//! UUIDv4. Inbound values are a trust boundary: the header is attacker-
//! controllable and flows into both a response header and structured logs, so an
//! unsanitised value could inject a header (CR/LF), forge log lines, or blow up
//! cardinality (unbounded length). [`sanitize_request_id`] accepts only a
//! bounded, safe charset and regenerates otherwise.

use axum::body::Body;
use axum::http::{HeaderValue, Request};
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument;
use uuid::Uuid;

/// Correlation header name. Lowercase: HTTP/2 requires lowercase header names and
/// `http::HeaderName` compares case-insensitively, so this matches `X-Request-Id`
/// from an HTTP/1.1 client too.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Upper bound on an accepted inbound id. Long enough for a UUID, a
/// `traceparent` trace-id, or a short opaque token; short enough to bound log /
/// header size. An over-long inbound id is treated as untrusted and regenerated.
const MAX_REQUEST_ID_LEN: usize = 128;

/// True if `value` is a safe correlation id: non-empty, within the length bound,
/// and restricted to `[A-Za-z0-9._-]`. That charset excludes CR/LF and other
/// control characters (no header injection / log forging) and whitespace.
fn is_valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REQUEST_ID_LEN
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Resolve the correlation id: the inbound value if present and
/// [`is_valid_request_id`], otherwise a fresh UUIDv4. The returned string is
/// always safe to write into a header and a log field.
pub fn sanitize_request_id(inbound: Option<&str>) -> String {
    match inbound {
        Some(value) if is_valid_request_id(value) => value.to_owned(),
        _ => Uuid::new_v4().to_string(),
    }
}

/// Build the request-correlation span carrying `request_id`.
///
/// The span is at **ERROR** level on purpose. A span is only recorded while its
/// level passes the subscriber's filter, and an event inherits a parent span's
/// fields only if that span is enabled. The server's `EnvFilter` is
/// operator-set (`config.log_level`); under `warn` or `error` an INFO-level span
/// would be disabled, so WARN/ERROR handler logs — exactly the ones you most
/// need to correlate — would lose `request_id`. ERROR is the highest severity,
/// so this span stays enabled under every level filter while never itself
/// emitting a log line.
fn request_span(request_id: &str) -> tracing::Span {
    tracing::error_span!("http_request", request_id = %request_id)
}

/// Tower/Axum middleware that establishes the request-id correlation span and
/// echoes the id in the response.
///
/// Apply it as the OUTERMOST layer so the span is active while every inner layer
/// (metrics, catch-panic) and the handler run, and so the echo wraps the whole
/// response — including a panic-synthesised 500.
pub async fn track_request_id(req: Request<Body>, next: Next) -> Response {
    let inbound = req.headers().get(REQUEST_ID_HEADER).and_then(|value| value.to_str().ok());
    let request_id = sanitize_request_id(inbound);

    // Run the inner service inside the correlation span so handler logs inherit
    // `request_id`. The id is a safe charset, so the HeaderValue conversion below
    // never fails — but fall back gracefully rather than panic.
    let mut response = next.run(req).instrument(request_span(&request_id)).await;

    if let Ok(header_value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, header_value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use tower::ServiceExt;

    async fn ok_handler() -> &'static str {
        "ok"
    }

    fn router() -> Router {
        Router::new().route("/x", get(ok_handler)).layer(axum::middleware::from_fn(track_request_id))
    }

    async fn request_id_of(inbound: Option<&str>) -> (StatusCode, Option<String>) {
        let mut builder = Request::builder().uri("/x");
        if let Some(value) = inbound {
            builder = builder.header(REQUEST_ID_HEADER, value);
        }
        let response = router().oneshot(builder.body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let id = response.headers().get(REQUEST_ID_HEADER).and_then(|v| v.to_str().ok()).map(ToString::to_string);
        (status, id)
    }

    #[tokio::test]
    async fn generates_uuid_when_no_inbound_header() {
        let (status, id) = request_id_of(None).await;
        assert_eq!(status, StatusCode::OK);
        let id = id.expect("response must carry an x-request-id");
        assert!(Uuid::parse_str(&id).is_ok(), "generated id must be a UUID, got {id:?}");
    }

    #[tokio::test]
    async fn preserves_safe_inbound_request_id() {
        let (status, id) = request_id_of(Some("trace-abc_123.9")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(id.as_deref(), Some("trace-abc_123.9"), "a safe inbound id must be echoed unchanged");
    }

    #[tokio::test]
    async fn replaces_unsafe_inbound_request_id_with_generated_uuid() {
        // Spaces + `;`/`=` are valid HTTP header bytes (so the HTTP layer lets
        // them in) but unsafe in a correlation id — log forging, cardinality.
        // (CR/LF can't reach here: `http` rejects them at the header boundary.)
        let unsafe_id = "forged; user=admin path=/x";
        let (status, id) = request_id_of(Some(unsafe_id)).await;
        assert_eq!(status, StatusCode::OK);
        let id = id.expect("response must still carry an id");
        assert_ne!(id, unsafe_id, "unsafe inbound id must not be echoed");
        assert!(Uuid::parse_str(&id).is_ok(), "replacement must be a UUID, got {id:?}");
    }

    #[test]
    fn sanitize_rules() {
        assert_eq!(sanitize_request_id(Some("abc-123_x.y")), "abc-123_x.y");
        for raw in [None, Some(""), Some("has space"), Some("nl\n"), Some("semi;colon")] {
            assert!(Uuid::parse_str(&sanitize_request_id(raw)).is_ok(), "must regenerate for {raw:?}");
        }
        let over_long = "a".repeat(MAX_REQUEST_ID_LEN + 1);
        assert!(Uuid::parse_str(&sanitize_request_id(Some(&over_long))).is_ok(), "over-long id must regenerate");
        let at_bound = "a".repeat(MAX_REQUEST_ID_LEN);
        assert_eq!(sanitize_request_id(Some(&at_bound)), at_bound);
    }

    /// Shared-buffer writer so a test can capture what a `tracing` subscriber
    /// renders, then assert on it.
    #[derive(Clone)]
    struct BufWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for BufWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("writer lock").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl tracing_subscriber::fmt::MakeWriter<'_> for BufWriter {
        type Writer = BufWriter;
        fn make_writer(&self) -> Self::Writer {
            self.clone()
        }
    }

    /// The correlation contract that matters operationally: under a `warn`
    /// `EnvFilter`, a WARN log emitted by a handler inside the request span must
    /// STILL carry `request_id`. Fails if the span is INFO-level (the filter
    /// would disable it); passes because `request_span` is ERROR-level.
    #[test]
    fn warn_log_under_warn_filter_keeps_request_id() {
        use tracing_subscriber::EnvFilter;
        use tracing_subscriber::fmt;

        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let subscriber =
            fmt().with_env_filter(EnvFilter::new("warn")).with_writer(BufWriter(buf.clone())).with_ansi(false).finish();

        tracing::subscriber::with_default(subscriber, || {
            let span = request_span("corr-9f3");
            let _enter = span.enter();
            tracing::warn!("handler hit a problem");
        });

        let out = String::from_utf8(buf.lock().expect("buf lock").clone()).expect("utf8");
        assert!(out.contains("handler hit a problem"), "the warn event must be emitted under a warn filter:\n{out}");
        assert!(
            out.contains("corr-9f3"),
            "a warn log under a warn filter must still carry request_id (span must outrank the filter):\n{out}"
        );
    }
}
