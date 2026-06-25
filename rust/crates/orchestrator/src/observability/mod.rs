//! Process-wide observability instrumentation for the orchestrator that is not
//! owned by a single service or repository.
//!
//! - [`http_metrics`]: the HTTP request-metrics middleware + Prometheus recorder
//!   that back the top-level `/metrics` scrape endpoint (CN-5).
//! - [`request_id`]: the request-ID correlation middleware that ties every log
//!   line for a request together and echoes `x-request-id` (MS-1).

pub mod http_metrics;
pub mod request_id;

pub use http_metrics::{
    HTTP_DURATION_BUCKETS, install_recorder, placeholder_handle, register_http_metrics, track_http_metrics,
};
pub use request_id::{REQUEST_ID_HEADER, sanitize_request_id, track_request_id};
