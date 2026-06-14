//! Test-only helpers that other crates and integration tests can reuse.
//!
//! This module is deliberately kept minimal: it only exposes what's needed
//! by integration tests today (metrics endpoint coverage). Expand carefully
//! — anything added here lives in the production dylib.
//!
//! The bearer-token shim is scoped to this module: we do NOT touch the real
//! `agentforge_auth` crate. `test-admin` / `test-member` strings are rewritten
//! into real signed JWTs by a per-test `axum::middleware::from_fn` layer so
//! the production `AuthUser` extractor still runs end-to-end.

use std::sync::{Arc, OnceLock};

use axum::Extension;
use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, header::AUTHORIZATION};
use axum::middleware::{Next, from_fn};
use axum::response::Response;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use uuid::Uuid;

use agentforge_auth::JwtManager;

use crate::routes::metrics as metrics_route;

/// Test-only re-export of the GitHub App client so integration tests can drive
/// it against an httpmock server. Gated behind `test-support`; never reachable
/// from production callers (the underlying module is `pub(crate)` without it).
#[cfg(any(test, feature = "test-support"))]
pub mod github_app {
    pub use crate::services::github_app::{
        GithubAppClient, GithubAppConfig, PrHead, PullRequest, build_app_jwt,
    };
}

/// 32+ byte secret used only in tests. Fixed so the JWT that the shim creates
/// can be verified by the same `JwtManager` instance on the route side.
const TEST_JWT_SECRET: &str = "metrics-endpoint-integration-test-secret-32bytes!";

/// Emit a single tick into a well-known counter so `/metrics` has something
/// to render. The counter name is a test fixture — NOT a production metric.
pub fn emit_test_counter() {
    ensure_recorder();
    metrics::counter!("af_test_bootstrap_total").increment(1);
}

/// Global, idempotent Prometheus recorder installation. `install_recorder`
/// registers a process-wide global; calling it twice errors, so guard it
/// behind a `OnceLock`. Subsequent callers reuse the same handle.
pub fn ensure_recorder() -> Arc<PrometheusHandle> {
    static HANDLE: OnceLock<Arc<PrometheusHandle>> = OnceLock::new();
    HANDLE
        .get_or_init(|| {
            let handle = PrometheusBuilder::new().install_recorder().expect("install prometheus recorder (test)");
            Arc::new(handle)
        })
        .clone()
}

/// Build a minimal `Router` that serves only `/metrics`, honoring the
/// `test-admin` / `test-member` bearer-token fixtures.
///
/// Deliberately independent of `AppState`: integration tests for the metrics
/// endpoint do not need the full application wiring. Anything that DOES need
/// AppState should build its own router via `create_router(state)` in
/// `crate::router`.
pub async fn test_router() -> Router {
    let handle = ensure_recorder();
    let jwt = Arc::new(JwtManager::new(TEST_JWT_SECRET, 3600));

    metrics_route::metrics_routes::<Arc<PrometheusHandle>>().with_state(handle).layer(Extension(jwt.clone())).layer(
        from_fn(move |req, next| {
            let jwt = jwt.clone();
            rewrite_test_bearer(jwt, req, next)
        }),
    )
}

/// Middleware that rewrites `Authorization: Bearer test-admin` and
/// `Authorization: Bearer test-member` into signed JWTs carrying `admin`
/// / `member` roles so the real `AuthUser` extractor accepts them.
/// Unknown bearer values pass through untouched — production auth failures
/// still surface as `401 UNAUTHORIZED` in tests.
async fn rewrite_test_bearer(jwt: Arc<JwtManager>, mut req: Request<Body>, next: Next) -> Response {
    let role = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|tok| match tok {
            "test-admin" => Some("admin"),
            "test-member" => Some("member"),
            _ => None,
        });

    if let Some(role) = role {
        let user_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();
        match jwt.create_token(user_id, org_id, role) {
            Ok(token) => {
                let replacement = format!("Bearer {token}");
                if let Ok(value) = HeaderValue::from_str(&replacement) {
                    req.headers_mut().insert(AUTHORIZATION, value);
                }
            }
            Err(err) => {
                tracing::error!(error = %err, "failed to mint test JWT");
            }
        }
    }

    next.run(req).await
}
