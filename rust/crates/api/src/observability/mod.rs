//! Process-wide observability instrumentation that is not owned by a single
//! service or repository.
//!
//! Today this is the HTTP request-metrics middleware that emits the
//! `http_requests_total` counter and `http_request_duration_seconds`
//! histogram consumed by the agents-runtime SLO alerts
//! (`ops/prometheus/agents-runtime.yml`) and Grafana dashboard
//! (`ops/grafana/dashboards/agents-runtime.json`).

pub mod http_metrics;

pub use http_metrics::{register_http_metrics, track_http_metrics};
