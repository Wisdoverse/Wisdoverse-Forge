//! Integration tests for the `/metrics` Prometheus scrape endpoint.
//!
//! Task 6a (P1): verify that
//! - Non-admin callers receive a 403 Forbidden.
//! - Admin callers receive a 200 OK with a Prometheus-formatted body that
//!   includes any metric emitted via the `metrics` facade.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn metrics_endpoint_rejects_non_admin() {
    let app = agentforge_api::testing::test_router().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .header("authorization", "Bearer test-member")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn metrics_endpoint_serves_prometheus_format_for_admin() {
    agentforge_api::testing::emit_test_counter();
    let app = agentforge_api::testing::test_router().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .header("authorization", "Bearer test-admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body = std::str::from_utf8(&body_bytes).unwrap();
    assert!(body.contains("af_test_bootstrap_total"));
    assert!(body.contains("# TYPE"));
}
