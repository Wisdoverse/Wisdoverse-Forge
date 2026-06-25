//! Contract: the request-ID correlation middleware (MS-1) is wired into the
//! real API router, so EVERY response — here the unauthenticated liveness
//! `/health` — carries an `x-request-id`, generated when absent and echoed when
//! a safe one is supplied.
//!
//! The middleware's own behaviour (sanitisation, generation, span level) is
//! unit-tested in `observability::request_id`; this pins only that
//! `create_router` applies it (as the outermost layer).

use agentforge_api::create_router;
use agentforge_api::test_support::app_state_with_mock_provider;
use axum::Router;
use axum::body::Body;
use http::{Request, StatusCode, header::HeaderName};
use tower::ServiceExt;
use uuid::Uuid;

const REQUEST_ID: &str = "x-request-id";

async fn health_request_id(app: Router, inbound: Option<&str>) -> (StatusCode, Option<String>) {
    let mut builder = Request::builder().method("GET").uri("/health");
    if let Some(value) = inbound {
        builder = builder.header(REQUEST_ID, value);
    }
    let response = app.oneshot(builder.body(Body::empty()).unwrap()).await.expect("request");
    let status = response.status();
    let id = response
        .headers()
        .get(HeaderName::from_static(REQUEST_ID))
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
    (status, id)
}

#[sqlx::test(migrations = "../db/migrations")]
async fn health_response_carries_generated_request_id(pool: sqlx::PgPool) {
    let app = create_router(app_state_with_mock_provider(pool, "mock", "unused").await);
    let (status, id) = health_request_id(app, None).await;
    assert_eq!(status, StatusCode::OK);
    let id = id.expect("router must attach x-request-id to every response");
    assert!(Uuid::parse_str(&id).is_ok(), "generated id must be a UUID, got {id:?}");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn health_response_echoes_safe_inbound_request_id(pool: sqlx::PgPool) {
    let app = create_router(app_state_with_mock_provider(pool, "mock", "unused").await);
    let (status, id) = health_request_id(app, Some("edge-req-42")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(id.as_deref(), Some("edge-req-42"), "a safe inbound id must be echoed end to end");
}
