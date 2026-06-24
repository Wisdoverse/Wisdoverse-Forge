//! `GET /metrics` — Prometheus scrape endpoint (platform-admin-gated).
//!
//! Registered at the top-level router (NOT under `/api/v1`) per the CLAUDE.md
//! Routing section. The exposition aggregates cross-tenant counts, queue
//! depths, and per-route latencies, so only platform admins may scrape it.
//!
//! #889/F005: the gate keys off the LIVE `users.is_admin` column via
//! [`AdminService::require_platform_admin`] rather than the JWT `role` claim —
//! a forged or stale elevated claim must not disclose platform-wide metrics,
//! and a demoted admin loses access immediately rather than for the token's
//! remaining lifetime.

use axum::Router;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;

use agentforge_auth::AuthUser;
use agentforge_core::AppResult;

use crate::health::AppState;

/// Render the Prometheus text-format exposition for the shared recorder.
/// Returns `403 Forbidden` for non-platform-admin callers via the live
/// DB-backed [`AdminService::require_platform_admin`] gate.
async fn scrape(State(state): State<AppState>, auth: AuthUser) -> AppResult<impl IntoResponse> {
    state.admin_service().require_platform_admin(auth.scope.user_id().as_uuid()).await?;
    let body = state.prometheus_handle.render();
    Ok(([("content-type", "text/plain; version=0.0.4")], body))
}

/// Top-level `/metrics` router, bound to `AppState` so the scrape handler can
/// reach the DB-backed platform-admin gate and the shared Prometheus recorder.
pub fn metrics_routes() -> Router<AppState> {
    Router::new().route("/metrics", get(scrape))
}
