//! `GET /metrics` — Prometheus scrape endpoint (admin-gated).
//!
//! Registered at the top-level router (NOT under `/api/v1`) per the CLAUDE.md
//! Routing section. Only admins can scrape — the handler delegates role
//! enforcement to [`AdminService::require_admin`] so the rule stays defined
//! exactly once.
//!
//! Task 6a of the legacy-nav metrics plan: infrastructure only. Individual
//! counters / gauges for compat routes are wired in Task 6b.

use std::sync::Arc;

use axum::Router;
use axum::extract::{FromRef, State};
use axum::response::IntoResponse;
use axum::routing::get;
use metrics_exporter_prometheus::PrometheusHandle;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::services::admin::AdminService;

/// Render the Prometheus text-format exposition for the shared recorder.
/// Returns `403 Forbidden` for non-admin callers via [`AdminService::require_admin`].
async fn scrape(State(handle): State<Arc<PrometheusHandle>>, auth: AuthUser) -> AppResult<impl IntoResponse> {
    AdminService::require_admin(&auth.role)?;
    let body = handle.render();
    Ok(([("content-type", "text/plain; version=0.0.4")], body))
}

/// Top-level `/metrics` router.
///
/// Generic over the outer state `S` so integration tests can plug in a bare
/// `Arc<PrometheusHandle>` without constructing a full `AppState`. Production
/// call sites use `metrics_routes::<AppState>()`, which extracts the handle
/// via the `FromRef<AppState> for Arc<PrometheusHandle>` impl in `health.rs`.
pub fn metrics_routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<PrometheusHandle>: FromRef<S>,
{
    Router::new().route("/metrics", get(scrape))
}
