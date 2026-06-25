//! Contract for the orchestrator's top-level Prometheus scrape endpoint
//! (`GET /metrics`, CN-5).
//!
//! This is the process-level operational exposition (request rate / latency /
//! status), distinct from the business dashboard JSON under
//! `/api/v1/metrics/*`. It is gated to the internal operator token — the
//! credential a Prometheus scraper carries — and is NOT a per-user surface:
//!
//! - internal token (or dev-mode, auth disabled) -> 200 `text/plain; version=0.0.4`
//! - no / wrong token while auth is enabled -> 401
//! - a valid *user session* JWT -> 403 (metrics are infra-only, not tenant data)
//!
//! The render-correctness of the exposition (bucket series, matched-route
//! labels) is pinned by the unit tests in `observability::http_metrics`; here
//! we pin only the route wiring + auth tier.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;

use agentforge_orchestrator::state::AppState;

const VALID_SIGNING_KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

async fn scrape(app: axum::Router, auth: Option<&str>) -> (StatusCode, Option<String>, String) {
    let mut builder = Request::builder().method("GET").uri("/metrics");
    if let Some(token) = auth {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app.oneshot(builder.body(Body::empty()).unwrap()).await.expect("request should succeed");
    let status = response.status();
    let content_type =
        response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(ToString::to_string);
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("body");
    (status, content_type, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn scrape_with_internal_token_returns_prometheus_text() {
    let app = AppState::test_internal_token("secret-token").router();
    let (status, content_type, _body) = scrape(app, Some("secret-token")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("text/plain; version=0.0.4"));
}

#[tokio::test]
async fn scrape_without_token_is_unauthorized_when_auth_enabled() {
    let app = AppState::test_internal_token("secret-token").router();
    let (status, _ct, _body) = scrape(app, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn scrape_with_wrong_token_is_unauthorized() {
    let app = AppState::test_internal_token("secret-token").router();
    let (status, _ct, _body) = scrape(app, Some("not-the-token")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn scrape_with_user_session_token_is_forbidden() {
    // A valid *user* session JWT must NOT be able to scrape process metrics:
    // the exposition is an operator/infra surface, gated to the internal token,
    // not a tenant-facing endpoint. This is the deliberate strict tier.
    let state = AppState::test_with_jwt_signing_key(VALID_SIGNING_KEY);
    let pair = state
        .sessions
        .as_ref()
        .expect("sessions configured")
        .issue_token_pair("user-1", "user@example.com", "User Example", "org-1")
        .await
        .expect("issue session token");
    let app = state.router();
    let (status, _ct, _body) = scrape(app, Some(&pair.access_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn scrape_is_open_when_auth_disabled() {
    // Dev mode: no internal token, no signing key -> auth disabled. The scrape
    // endpoint is open, matching the orchestrator's unauthenticated `/health`.
    let app = AppState::test().router();
    let (status, content_type, _body) = scrape(app, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("text/plain; version=0.0.4"));
}
