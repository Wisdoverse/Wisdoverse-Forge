//! Request-ID correlation middleware (MS-1 slice 1, distributed tracing).
//!
//! Every request gets a stable correlation id, surfaced two ways:
//!
//! - bound to a `tracing` span (`request_id=…`) so every log line emitted while
//!   handling the request carries it — `grep` one id to follow a request end to
//!   end through the orchestrator's JSON logs;
//! - echoed back in the `x-request-id` response header so a caller (or the API
//!   gateway, in a later slice) can correlate its side too.
//!
//! The id is taken from an inbound `x-request-id` header when present and SAFE,
//! otherwise a fresh UUIDv4 is generated. Inbound values are a trust boundary:
//! the header is attacker-controllable and flows into both a response header and
//! structured logs, so an unsanitised value could inject a header (CR/LF), forge
//! log lines (newlines/control chars), or blow up label cardinality (unbounded
//! length). [`sanitize_request_id`] therefore accepts only a bounded, safe
//! charset and regenerates otherwise.
//!
//! This is the foundation for cross-service propagation: once the API forwards
//! its `x-request-id` (and, later, a W3C `traceparent`) on the call into the
//! orchestrator, the orchestrator's logs line up with the API's automatically.

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

tokio::task_local! {
    /// The current request's correlation id, set by [`track_request_id`] for the
    /// duration of request handling. Task-locals propagate across `.await` within
    /// the same task, so any outbound call a handler makes (e.g. the orchestrator
    /// calling back into the API's MCP bridge) can read it via
    /// [`current_request_id`] and forward `x-request-id` — lining the two
    /// services' logs up under one id (MS-1 cross-hop propagation).
    static REQUEST_ID: String;
}

/// The correlation id of the request currently being handled on this task, if
/// any. `None` outside request context (e.g. a background worker), so callers
/// degrade gracefully to no propagation rather than failing.
pub fn current_request_id() -> Option<String> {
    REQUEST_ID.try_with(|id| id.clone()).ok()
}

/// Run `future` with `request_id` re-established as the task-local correlation id.
///
/// Tokio task-locals are NOT inherited by `tokio::spawn`ed tasks, so a handler
/// that offloads work (e.g. the task-dispatch path spawns the outbound MCP
/// calls) must capture [`current_request_id`] BEFORE spawning and wrap the
/// spawned future with this so the id survives the task boundary. `None` runs
/// the future unscoped (no correlation id) — the graceful degrade.
pub async fn scope_request_id<F>(request_id: Option<String>, future: F) -> F::Output
where
    F: std::future::Future,
{
    match request_id {
        Some(id) => REQUEST_ID.scope(id, future).await,
        None => future.await,
    }
}

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
/// fields only if that span is enabled. The orchestrator's `EnvFilter` is
/// operator-set (`ORCHESTRATOR_LOG_LEVEL`); under `warn` or `error` an
/// INFO-level span would be disabled, so WARN/ERROR handler logs — exactly the
/// ones you most need to correlate — would lose `request_id`. ERROR is the
/// highest severity, so this span stays enabled under every level filter
/// (`error` ⊆ `warn` ⊆ `info` …) while never itself emitting a log line.
fn request_span(request_id: &str) -> tracing::Span {
    tracing::error_span!("http_request", request_id = %request_id)
}

/// Tower/Axum middleware that establishes the request-id correlation span and
/// echoes the id in the response.
///
/// Apply it as the OUTERMOST layer so the span is active while every inner layer
/// (metrics) and the handler run, and so the echo wraps the whole response.
pub async fn track_request_id(req: Request<Body>, next: Next) -> Response {
    let inbound = req.headers().get(REQUEST_ID_HEADER).and_then(|value| value.to_str().ok());
    let request_id = sanitize_request_id(inbound);

    // Run the inner service inside the correlation span (so handler logs inherit
    // `request_id`) AND inside the REQUEST_ID task-local scope (so outbound calls
    // the handler makes can forward the id). The id is a safe charset, so the
    // HeaderValue conversion below never fails — but fall back gracefully.
    let inner = next.run(req).instrument(request_span(&request_id));
    let mut response = REQUEST_ID.scope(request_id.clone(), inner).await;

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

    /// Drive one request through the layered router and return the echoed
    /// `x-request-id` (if any) plus the status.
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
    async fn current_request_id_is_none_outside_request_scope() {
        // No middleware, no scope → outbound callers must see None and skip
        // propagation rather than panic.
        assert_eq!(current_request_id(), None);
    }

    #[tokio::test]
    async fn track_request_id_populates_task_local_for_handlers() {
        // A handler reads the task-local and reflects it; if the middleware sets
        // the REQUEST_ID scope, the handler sees the SAME id it was given.
        async fn reflect() -> axum::http::HeaderMap {
            let mut headers = axum::http::HeaderMap::new();
            let seen = current_request_id().unwrap_or_default();
            headers.insert("x-seen-request-id", seen.parse().expect("valid header value"));
            headers
        }
        let app = Router::new().route("/x", get(reflect)).layer(axum::middleware::from_fn(track_request_id));
        let response = app
            .oneshot(Request::builder().uri("/x").header(REQUEST_ID_HEADER, "corr-tl-1").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let seen = response.headers().get("x-seen-request-id").and_then(|v| v.to_str().ok());
        assert_eq!(seen, Some("corr-tl-1"), "handler must observe the request id via the task-local");
    }

    #[tokio::test]
    async fn scope_request_id_survives_tokio_spawn() {
        // The dispatch path captures the id in a request scope, then offloads the
        // MCP work to `tokio::spawn`. Task-locals are NOT inherited by spawned
        // tasks, so a BARE spawn loses the id — but `scope_request_id` with the
        // captured id re-establishes it. This pins the task/handler.rs fix.
        let (bare, scoped) = REQUEST_ID
            .scope("corr-spawn-1".to_string(), async {
                let captured = current_request_id();
                let bare = tokio::spawn(async { current_request_id() }).await.expect("bare join");
                let scoped = tokio::spawn(scope_request_id(captured, async { current_request_id() }))
                    .await
                    .expect("scoped join");
                (bare, scoped)
            })
            .await;
        assert_eq!(bare, None, "a bare spawn does not inherit the task-local");
        assert_eq!(scoped.as_deref(), Some("corr-spawn-1"), "scope_request_id must re-establish it across the spawn");
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
        // The value must be discarded and a fresh UUID generated, never echoed.
        let unsafe_id = "forged; user=admin path=/x";
        let (status, id) = request_id_of(Some(unsafe_id)).await;
        assert_eq!(status, StatusCode::OK);
        let id = id.expect("response must still carry an id");
        assert_ne!(id, unsafe_id, "unsafe inbound id must not be echoed");
        assert!(Uuid::parse_str(&id).is_ok(), "replacement must be a UUID, got {id:?}");
    }

    #[test]
    fn sanitize_rules() {
        // Safe values pass through.
        assert_eq!(sanitize_request_id(Some("abc-123_x.y")), "abc-123_x.y");
        // Absent / empty / unsafe / over-long all regenerate to a UUID.
        for raw in [None, Some(""), Some("has space"), Some("nl\n"), Some("semi;colon")] {
            assert!(Uuid::parse_str(&sanitize_request_id(raw)).is_ok(), "must regenerate for {raw:?}");
        }
        let over_long = "a".repeat(MAX_REQUEST_ID_LEN + 1);
        assert!(Uuid::parse_str(&sanitize_request_id(Some(&over_long))).is_ok(), "over-long id must regenerate");
        // Exactly at the bound is still accepted.
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
    /// `EnvFilter` (a common production setting), a WARN log emitted by a handler
    /// inside the request span must STILL carry `request_id`. This fails if the
    /// span is INFO-level (the filter would disable it); it passes because
    /// `request_span` is ERROR-level. Regression guard for that level choice.
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
