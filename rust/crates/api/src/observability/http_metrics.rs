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
//! [`MatchedPath`] is inserted into the request extensions by the router only
//! once a route has been matched. A `from_fn(track_http_metrics)` layer
//! therefore reads `Some(MatchedPath)` so long as it is applied to the router
//! *after* the routes are defined (i.e. it wraps the routed service). See
//! `crate::router::create_router` for the wiring and the unit test below for
//! the ordering guarantee.

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
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("current-thread runtime");

        let status = metrics::with_local_recorder(&recorder, || {
            rt.block_on(async {
                let app = router();
                let res = app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap()).await.unwrap();
                res.status()
            })
        });

        (status, snapshotter.snapshot())
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
}
