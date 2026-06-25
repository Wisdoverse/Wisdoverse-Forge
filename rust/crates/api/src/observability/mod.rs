//! Process-wide observability instrumentation that is not owned by a single
//! service or repository.
//!
//! - [`http_metrics`]: the HTTP request-metrics middleware that emits the
//!   `http_requests_total` counter and `http_request_duration_seconds`
//!   histogram consumed by the agents-runtime SLO alerts
//!   (`ops/prometheus/agents-runtime.yml`) and Grafana dashboard
//!   (`ops/grafana/dashboards/agents-runtime.json`).
//! - [`request_id`]: the request-ID correlation middleware that ties every log
//!   line for a request together and echoes `x-request-id` (MS-1).

pub mod http_metrics;
pub mod request_id;

pub use http_metrics::{register_http_metrics, track_http_metrics};
pub use request_id::{REQUEST_ID_HEADER, sanitize_request_id, track_request_id};
