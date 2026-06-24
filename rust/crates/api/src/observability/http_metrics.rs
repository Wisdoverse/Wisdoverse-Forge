//! HTTP request-metrics middleware.
//!
//! Emits, for every request that reaches the Axum router:
//!
//! - `http_requests_total{method, path, status}` — request counter.
//! - `http_request_duration_seconds{method, path}` — latency histogram
//!   (the Prometheus exporter derives `_bucket` / `_sum` / `_count` series).
//!
//! These are the SLIs behind the agents-runtime SLO alert rules in
//! `ops/prometheus/agents-runtime.yml` and the panels in
//! `ops/grafana/dashboards/agents-runtime.json`. Before this middleware the
//! rules and panels were dead: nothing in the backend ever emitted them.
//!
//! ## Path label cardinality
//!
//! The `path` label is the **matched Axum route template** (e.g.
//! `/api/v1/agents/{id}/restart`), taken from [`MatchedPath`] in the request
//! extensions — *not* the raw URI. Labelling by the raw URI would mint a new
//! Prometheus series for every UUID (an unbounded-cardinality explosion) and
//! would also break the SLO rules, which match by template-shaped path
//! (`path=~"/api/v1/agents/[^/]+/restart"`, `path="/api/v1/agents"`, …).
//!
//! When no route matched (404s, fallback service) `MatchedPath` is absent; we
//! label such requests `<unmatched>` so unknown URIs collapse into a single
//! bounded series instead of exploding cardinality.
//!
//! ## Layer ordering
//!
//! [`MatchedPath`] is inserted into the request extensions by the router once a
//! route has been matched. A `from_fn(track_http_metrics)` layer applied via
//! `Router::layer()` wraps each routed endpoint *after* that insertion, so it
//! reads `Some(MatchedPath)` — including the full `/api/v1/...` nest prefix for
//! nested routers (see `records_nest_prefixed_route_template`).
//!
//! This layer must also sit **outside** the catch-panic layer. Tower/Axum
//! layers run bottom-up, so when the metrics layer is applied last (outermost),
//! a panicking handler unwinds only as far as the inner catch-panic layer,
//! which converts the panic into a synthesized `500` `Response`. That `500`
//! then returns normally through the metrics layer's `next.run().await` and is
//! counted as `http_requests_total{status="500"}`. If the metrics layer were
//! *inside* catch-panic, the unwind would tear through its own `next.run().await`
//! and the panic-induced 500 would never be recorded. See
//! `crate::router::create_router` for the wiring and `counts_panic_induced_500`
//! below for the ordering guarantee.

use std::time::Instant;

use axum::body::Body;
use axum::extract::MatchedPath;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;

/// Label value used when no route matched (404 / fallback). Keeps unmatched
/// URIs from exploding `path` cardinality.
const UNMATCHED_PATH: &str = "<unmatched>";

/// SLO-aligned histogram bucket boundaries (seconds) for
/// `http_request_duration_seconds`.
///
/// Chosen to straddle the agents-runtime SLO thresholds so
/// `histogram_quantile(0.95, …)` resolves meaningfully near each budget:
/// 500ms (create), 800ms (enroll), and 2s (container restart).
pub const HTTP_DURATION_BUCKETS: [f64; 8] = [0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0];

/// Register descriptions (and the bucket-bearing histogram series) so the
/// `/metrics` scrape and dashboards have the series present before traffic.
///
/// Call once at startup, alongside the other `register_*_metrics()` hooks.
pub fn register_http_metrics() {
    metrics::describe_counter!(
        "http_requests_total",
        "Total HTTP requests handled, labelled by method, matched route path, and status code."
    );
    metrics::describe_histogram!(
        "http_request_duration_seconds",
        metrics::Unit::Seconds,
        "HTTP request latency in seconds, labelled by method and matched route path."
    );
}

/// Tower/Axum middleware that records request count and latency keyed by the
/// matched route template.
///
/// Apply via `axum::middleware::from_fn(track_http_metrics)` on the routed
/// router so [`MatchedPath`] is populated. See the module docs for ordering.
pub async fn track_http_metrics(req: Request<Body>, next: Next) -> Response {
    let method = req.method().as_str().to_owned();
    let path = matched_path_label(&req);

    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    metrics::counter!(
        "http_requests_total",
        "method" => method.clone(),
        "path" => path.clone(),
        "status" => status,
    )
    .increment(1);
    metrics::histogram!(
        "http_request_duration_seconds",
        "method" => method,
        "path" => path,
    )
    .record(elapsed);

    response
}

/// Resolve the `path` label: the matched route template, or `<unmatched>`.
fn matched_path_label(req: &Request<Body>) -> String {
    req.extensions().get::<MatchedPath>().map(|m| m.as_str().to_owned()).unwrap_or_else(|| UNMATCHED_PATH.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use metrics::{Key, Label};
    use metrics_util::CompositeKey;
    use metrics_util::MetricKind;
    use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshot};
    use tower::ServiceExt;

    async fn ok_handler() -> &'static str {
        "ok"
    }

    fn router() -> Router {
        Router::new().route("/x/{id}", get(ok_handler)).layer(axum::middleware::from_fn(track_http_metrics))
    }

    /// Drive a single request through the layered router with `recorder`
    /// installed as the thread-local recorder for the *entire* request,
    /// then return the captured snapshot.
    ///
    /// `with_local_recorder` is synchronous and thread-local: it only applies
    /// for the duration of its closure on the calling thread. We therefore run
    /// the request to completion on a current-thread runtime *inside* the
    /// closure, so every `metrics::counter!` / `histogram!` the middleware
    /// emits is captured by `recorder` rather than the global no-op.
    fn run_request_capturing(uri: &str) -> (StatusCode, Snapshot) {
        run_request_capturing_on(router(), uri)
    }

    /// Like [`run_request_capturing`] but drives the supplied router, so tests
    /// can exercise alternate wirings (nesting, catch-panic) against the same
    /// thread-local-recorder capture machinery.
    fn run_request_capturing_on(app: Router, uri: &str) -> (StatusCode, Snapshot) {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("current-thread runtime");

        let status = metrics::with_local_recorder(&recorder, || {
            rt.block_on(async {
                let res = app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap()).await.unwrap();
                res.status()
            })
        });

        (status, snapshotter.snapshot())
    }

    /// True if `http_requests_total` was recorded once with the given
    /// `method` / `path` / `status` label triple.
    fn counter_present(
        snapshot: &[(CompositeKey, Option<metrics::Unit>, Option<metrics::SharedString>, DebugValue)],
        method: &str,
        path: &str,
        status: &str,
    ) -> bool {
        let key = CompositeKey::new(
            MetricKind::Counter,
            Key::from_parts(
                "http_requests_total",
                vec![
                    Label::new("method", method.to_owned()),
                    Label::new("path", path.to_owned()),
                    Label::new("status", status.to_owned()),
                ],
            ),
        );
        snapshot
            .iter()
            .find(|(k, _, _, _)| k == &key)
            .map(|(_, _, _, v)| matches!(v, DebugValue::Counter(n) if *n >= 1))
            .unwrap_or(false)
    }

    /// The middleware must label by the matched route TEMPLATE, not the raw
    /// URI: a request to `/x/42` records `path="/x/{id}"`, never `path="/x/42"`.
    /// This is the cardinality + SLO-regex correctness guarantee.
    #[test]
    fn records_matched_route_template_not_raw_uri() {
        let (status, snapshot) = run_request_capturing("/x/42");
        assert_eq!(status, StatusCode::OK);
        let snapshot = snapshot.into_vec();

        // Counter recorded with the route template, not the concrete id.
        let counter_key = CompositeKey::new(
            MetricKind::Counter,
            Key::from_parts(
                "http_requests_total",
                vec![Label::new("method", "GET"), Label::new("path", "/x/{id}"), Label::new("status", "200")],
            ),
        );
        let counter = snapshot
            .iter()
            .find(|(key, _, _, _)| key == &counter_key)
            .map(|(_, _, _, value)| value)
            .expect("http_requests_total series with path=\"/x/{id}\" must exist");
        assert_eq!(*counter, DebugValue::Counter(1));

        // The raw-URI series must NOT exist (no cardinality explosion).
        let raw_uri_present =
            snapshot.iter().any(|(key, _, _, _)| key.key().labels().any(|l| l.key() == "path" && l.value() == "/x/42"));
        assert!(!raw_uri_present, "raw URI path label must never be recorded");

        // Histogram recorded under the template path as well.
        let hist_present = snapshot.iter().any(|(key, _, _, value)| {
            key.kind() == MetricKind::Histogram
                && key.key().name() == "http_request_duration_seconds"
                && key.key().labels().any(|l| l.key() == "path" && l.value() == "/x/{id}")
                && matches!(value, DebugValue::Histogram(samples) if samples.len() == 1)
        });
        assert!(hist_present, "http_request_duration_seconds must be recorded with path=\"/x/{{id}}\"");
    }

    /// Unmatched URIs (no route -> 404) collapse into a single bounded series
    /// labelled `<unmatched>`, never the raw URI.
    #[test]
    fn unmatched_route_uses_placeholder_label() {
        let (status, snapshot) = run_request_capturing("/does/not/exist");
        assert_eq!(status, StatusCode::NOT_FOUND);
        let snapshot = snapshot.into_vec();
        let unmatched_present = snapshot.iter().any(|(key, _, _, _)| {
            key.key().name() == "http_requests_total"
                && key.key().labels().any(|l| l.key() == "path" && l.value() == UNMATCHED_PATH)
        });
        assert!(unmatched_present, "404s must be labelled <unmatched>");

        let raw_uri_present =
            snapshot.iter().any(|(key, _, _, _)| key.key().labels().any(|l| l.value() == "/does/not/exist"));
        assert!(!raw_uri_present, "unmatched raw URI must not become a label");
    }

    /// End-to-end contract with the Grafana dashboard: with the SLO buckets
    /// configured (as `main.rs` does), the Prometheus exporter must render the
    /// histogram as `http_request_duration_seconds_bucket{...,le="..."}` (NOT a
    /// quantile summary). The dashboard's `histogram_quantile(0.95,
    /// rate(http_request_duration_seconds_bucket{...}[5m]))` queries depend on
    /// these `_bucket` series existing.
    #[test]
    fn renders_bucket_series_for_dashboard_histogram_quantile() {
        use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};

        let recorder = PrometheusBuilder::new()
            .set_buckets_for_metric(Matcher::Full("http_request_duration_seconds".to_owned()), &HTTP_DURATION_BUCKETS)
            .expect("configure buckets")
            .build_recorder();
        let handle = recorder.handle();

        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
        metrics::with_local_recorder(&recorder, || {
            rt.block_on(async {
                let app = router();
                let res = app.oneshot(Request::builder().uri("/x/42").body(Body::empty()).unwrap()).await.unwrap();
                assert_eq!(res.status(), StatusCode::OK);
            });
        });

        let rendered = handle.render();

        // Histogram emits `_bucket` series with `le` bounds — the shape the
        // dashboard's histogram_quantile() and the SLO rules query.
        assert!(
            rendered.contains("http_request_duration_seconds_bucket"),
            "exporter must emit _bucket series, got:\n{rendered}"
        );
        assert!(rendered.contains("le=\"0.5\""), "SLO bucket boundary 0.5 must be present:\n{rendered}");
        // Counter emitted with the matched-route template path.
        assert!(
            rendered.contains("http_requests_total") && rendered.contains("path=\"/x/{id}\""),
            "counter must carry the matched-route path label:\n{rendered}"
        );
        // Must NOT degrade to a quantile summary (no buckets) for this metric.
        assert!(
            !rendered.contains("http_request_duration_seconds{quantile="),
            "metric must render as a histogram, not a summary:\n{rendered}"
        );
    }

    /// GAP A: the `path` label must carry the FULL nested route template,
    /// including the `/api/v1` nest prefix — that prefix is load-bearing for
    /// every agents-runtime SLO alert regex (`path=~"/api/v1/agents.*"`,
    /// `path="/api/v1/agents"`, …) and Grafana panel in
    /// `ops/prometheus/agents-runtime.yml`. The earlier flat-router tests only
    /// proved templating, not that the nest prefix survives.
    ///
    /// This mirrors production: the metrics layer is applied via
    /// `Router::layer()` on the OUTER router that holds the `.nest("/api/v1", …)`,
    /// exactly as `crate::router::create_router` wires it.
    #[test]
    fn records_nest_prefixed_route_template() {
        let app = Router::new()
            .nest("/api/v1", Router::new().route("/agents/{id}/restart", get(ok_handler)))
            .layer(axum::middleware::from_fn(track_http_metrics));

        let (status, snapshot) = run_request_capturing_on(app, "/api/v1/agents/42/restart");
        assert_eq!(status, StatusCode::OK);
        let snapshot = snapshot.into_vec();

        // The recorded path label must be the full nested template WITH prefix.
        assert!(
            counter_present(&snapshot, "GET", "/api/v1/agents/{id}/restart", "200"),
            "counter must carry path=\"/api/v1/agents/{{id}}/restart\" (with nest prefix); got: {:?}",
            snapshot
                .iter()
                .filter_map(|(k, _, _, _)| k.key().labels().find(|l| l.key() == "path").map(|l| l.value().to_owned()))
                .collect::<Vec<_>>()
        );

        // NOT the prefix-stripped template the inner router alone would yield.
        let stripped = snapshot.iter().any(|(k, _, _, _)| {
            k.key().name() == "http_requests_total"
                && k.key().labels().any(|l| l.key() == "path" && l.value() == "/agents/{id}/restart")
        });
        assert!(!stripped, "path label must include the /api/v1 nest prefix, not the inner-only template");

        // NOT the raw, id-bearing URI (cardinality explosion + breaks SLO regex).
        let raw = snapshot.iter().any(|(k, _, _, _)| {
            k.key().labels().any(|l| l.key() == "path" && l.value() == "/api/v1/agents/42/restart")
        });
        assert!(!raw, "path label must be the route template, not the raw URI");
    }

    /// Panic accounting: a handler that `panic!()`s, wrapped (in production
    /// order) with `catch_panic_layer` INSIDE the metrics layer, must surface a
    /// 500 to the metrics layer's `next.run().await` so it is counted as
    /// `http_requests_total{status="500"}`.
    ///
    /// If the metrics layer were inside CatchPanic the unwind would tear through
    /// its own `next.run().await` and the 500 would never be recorded — the bug
    /// `crate::router::create_router`'s layer ordering fixes. This test pins the
    /// ordering: metrics is applied LAST (outermost) so it wraps CatchPanic.
    #[test]
    fn counts_panic_induced_500() {
        async fn panic_handler() -> &'static str {
            panic!("boom from handler");
        }

        // Same nesting order as production: inner middleware (CatchPanic) is
        // applied first, the metrics layer is applied last so it is OUTERMOST.
        let app = Router::new()
            .route("/panic", get(panic_handler))
            .layer(crate::middleware::catch_panic_layer())
            .layer(axum::middleware::from_fn(track_http_metrics));

        let (status, snapshot) = run_request_capturing_on(app, "/panic");
        // CatchPanic converted the panic into a synthesized 500 Response.
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        let snapshot = snapshot.into_vec();

        // The synthesized 500 must have flowed back through the metrics layer
        // and been counted — proving the layer is OUTSIDE CatchPanic.
        assert!(
            counter_present(&snapshot, "GET", "/panic", "500"),
            "panic-induced 500 must be counted as http_requests_total{{status=\"500\"}}; got: {:?}",
            snapshot
                .iter()
                .filter(|(k, _, _, _)| k.key().name() == "http_requests_total")
                .map(|(k, _, _, _)| k.key().labels().map(|l| format!("{}={}", l.key(), l.value())).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        );
    }

    /// Companion to [`counts_panic_induced_500`]: an explicit
    /// `StatusCode::INTERNAL_SERVER_ERROR` response (no panic) is also recorded
    /// as `status="500"`. Together they cover both the synthesized and the
    /// returned 500 paths.
    #[test]
    fn counts_explicit_500_response() {
        async fn err_handler() -> StatusCode {
            StatusCode::INTERNAL_SERVER_ERROR
        }

        let app = Router::new().route("/err", get(err_handler)).layer(axum::middleware::from_fn(track_http_metrics));

        let (status, snapshot) = run_request_capturing_on(app, "/err");
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        let snapshot = snapshot.into_vec();
        assert!(counter_present(&snapshot, "GET", "/err", "500"), "explicit 500 must be counted with status=\"500\"");
    }
}
