//! Process-level HTTP request metrics for the orchestrator (CN-5).
//!
//! Emits, for every request that reaches the Axum router:
//!
//! - `http_requests_total{method, path, status}` — request counter.
//! - `http_request_duration_seconds{method, path}` — latency histogram
//!   (the Prometheus exporter derives `_bucket` / `_sum` / `_count` series).
//!
//! These are the orchestrator's operational SLIs — request rate, error rate,
//! and latency for the coordinator API (participants, tasks, reviews,
//! workflows, the MCP bridge). They are scraped from the top-level `/metrics`
//! endpoint (see [`crate::router`]) and are entirely separate from the
//! *business* dashboard metrics under `/api/v1/metrics/*` (active tasks,
//! pending reviews, …), which are tenant-scoped JSON, not a Prometheus
//! exposition.
//!
//! This mirrors the main API server's `observability::http_metrics` so the
//! orchestrator participates in the same Prometheus/Grafana stack with the
//! same metric names and label shape.
//!
//! ## Path label cardinality
//!
//! The `path` label is the **matched Axum route template** (e.g.
//! `/api/v1/tasks/{id}`), taken from [`MatchedPath`] in the request extensions
//! — *not* the raw URI. Labelling by the raw URI would mint a new Prometheus
//! series for every id (an unbounded-cardinality explosion). When no route
//! matched (404s, fallback) `MatchedPath` is absent; such requests are
//! labelled `<unmatched>` so unknown URIs collapse into a single bounded
//! series.

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::MatchedPath;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};

/// Label value used when no route matched (404 / fallback). Keeps unmatched
/// URIs from exploding `path` cardinality.
const UNMATCHED_PATH: &str = "<unmatched>";

/// Histogram bucket boundaries (seconds) for `http_request_duration_seconds`.
///
/// Same bounds as the API server so a shared dashboard's
/// `histogram_quantile(0.95, …)` resolves identically across both services.
pub const HTTP_DURATION_BUCKETS: [f64; 8] = [0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0];

/// Install the global Prometheus recorder for the orchestrator process and
/// return the render handle for the `/metrics` scrape route.
///
/// Configures the SLO-aligned histogram buckets for
/// `http_request_duration_seconds` (without explicit buckets the exporter
/// renders a summary, so no `_bucket{le=…}` series exist and
/// `histogram_quantile()` queries return nothing).
///
/// A metrics-recorder hiccup must never take down the coordinator, so a build
/// or install failure (most commonly: a recorder is already installed in this
/// process) degrades to a non-installed local handle that renders an empty
/// exposition, and logs a warning, rather than erroring.
pub fn install_recorder() -> Arc<PrometheusHandle> {
    match PrometheusBuilder::new()
        .set_buckets_for_metric(Matcher::Full("http_request_duration_seconds".to_owned()), &HTTP_DURATION_BUCKETS)
        .and_then(|builder| builder.install_recorder())
    {
        Ok(handle) => Arc::new(handle),
        Err(err) => {
            tracing::warn!(
                error = %err,
                "prometheus recorder unavailable (already installed or misconfigured); /metrics will render empty"
            );
            placeholder_handle()
        }
    }
}

/// A non-installed render handle that produces an empty exposition.
///
/// Used by test / default `AppState`s that never install a global recorder, so
/// the `/metrics` route still has a handle to render. `build_recorder()` is
/// infallible and has no global side effect.
pub fn placeholder_handle() -> Arc<PrometheusHandle> {
    Arc::new(PrometheusBuilder::new().build_recorder().handle())
}

/// Register metric descriptions so the `/metrics` scrape and dashboards have
/// the series present before the first request. Call once at startup, after
/// the recorder is installed.
pub fn register_http_metrics() {
    metrics::describe_counter!(
        "http_requests_total",
        "Total HTTP requests handled by the orchestrator, labelled by method, matched route path, and status code."
    );
    metrics::describe_histogram!(
        "http_request_duration_seconds",
        metrics::Unit::Seconds,
        "Orchestrator HTTP request latency in seconds, labelled by method and matched route path."
    );
}

/// Tower/Axum middleware that records request count and latency keyed by the
/// matched route template. Apply via `axum::middleware::from_fn` on the routed
/// router so [`MatchedPath`] is populated.
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
    /// installed as the thread-local recorder, then return the snapshot.
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

    /// The middleware must label by the matched route TEMPLATE, not the raw
    /// URI: `/x/42` records `path="/x/{id}"`, never `path="/x/42"`.
    #[test]
    fn records_matched_route_template_not_raw_uri() {
        let (status, snapshot) = run_request_capturing_on(router(), "/x/42");
        assert_eq!(status, StatusCode::OK);
        let snapshot = snapshot.into_vec();

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

        let raw_uri_present =
            snapshot.iter().any(|(key, _, _, _)| key.key().labels().any(|l| l.key() == "path" && l.value() == "/x/42"));
        assert!(!raw_uri_present, "raw URI path label must never be recorded");
    }

    /// Unmatched URIs (no route -> 404) collapse into a single bounded series
    /// labelled `<unmatched>`, never the raw URI.
    #[test]
    fn unmatched_route_uses_placeholder_label() {
        let (status, snapshot) = run_request_capturing_on(router(), "/does/not/exist");
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

    /// The path label must carry the FULL nested route template including the
    /// `/api/v1` prefix — production wires the layer on the OUTER router that
    /// holds `.nest("/api/v1", …)`, so this pins that the nest prefix survives.
    #[test]
    fn records_nest_prefixed_route_template() {
        let app = Router::new()
            .nest("/api/v1", Router::new().route("/tasks/{id}", get(ok_handler)))
            .layer(axum::middleware::from_fn(track_http_metrics));

        let (status, snapshot) = run_request_capturing_on(app, "/api/v1/tasks/42");
        assert_eq!(status, StatusCode::OK);
        let snapshot = snapshot.into_vec();

        let prefixed = snapshot.iter().any(|(k, _, _, _)| {
            k.key().name() == "http_requests_total"
                && k.key().labels().any(|l| l.key() == "path" && l.value() == "/api/v1/tasks/{id}")
        });
        assert!(prefixed, "path label must include the /api/v1 nest prefix");

        let stripped = snapshot
            .iter()
            .any(|(k, _, _, _)| k.key().labels().any(|l| l.key() == "path" && l.value() == "/tasks/{id}"));
        assert!(!stripped, "path label must include the /api/v1 nest prefix, not the inner-only template");
    }

    /// End-to-end with the Prometheus exporter: with the buckets configured (as
    /// `install_recorder` does), the histogram renders as
    /// `http_request_duration_seconds_bucket{...,le="..."}` (NOT a quantile
    /// summary), which is the shape `histogram_quantile()` and SLO rate rules
    /// query.
    #[test]
    fn renders_bucket_series_for_dashboard_histogram_quantile() {
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

        assert!(
            rendered.contains("http_request_duration_seconds_bucket"),
            "exporter must emit _bucket series, got:\n{rendered}"
        );
        assert!(rendered.contains("le=\"0.5\""), "SLO bucket boundary 0.5 must be present:\n{rendered}");
        assert!(
            rendered.contains("http_requests_total") && rendered.contains("path=\"/x/{id}\""),
            "counter must carry the matched-route path label:\n{rendered}"
        );
        assert!(
            !rendered.contains("http_request_duration_seconds{quantile="),
            "metric must render as a histogram, not a summary:\n{rendered}"
        );
    }
}
