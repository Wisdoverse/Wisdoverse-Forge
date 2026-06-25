//! Process-wide observability instrumentation for the orchestrator that is not
//! owned by a single service or repository.
//!
//! Today this is the HTTP request-metrics middleware + Prometheus recorder that
//! back the top-level `/metrics` scrape endpoint (CN-5).

pub mod http_metrics;

pub use http_metrics::{
    HTTP_DURATION_BUCKETS, install_recorder, placeholder_handle, register_http_metrics, track_http_metrics,
};
