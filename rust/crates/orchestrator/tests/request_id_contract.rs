//! Contract: the request-ID correlation middleware (MS-1) is wired into the
//! real orchestrator router, so EVERY response — on any route, here the
//! unauthenticated `/health` — carries an `x-request-id`, generated when absent
//! and echoed when a safe one is supplied.
//!
//! The middleware's own behaviour (sanitisation, generation) is unit-tested in
//! `observability::request_id`; this pins only that `create_router` applies it.

use axum::body::Body;
use axum::http::{Request, StatusCode, header::HeaderName};
use tower::ServiceExt;
use uuid::Uuid;

use agentforge_orchestrator::state::AppState;

const REQUEST_ID: &str = "x-request-id";

async fn health_request_id(inbound: Option<&str>) -> (StatusCode, Option<String>) {
    let mut builder = Request::builder().method("GET").uri("/health");
    if let Some(value) = inbound {
        builder = builder.header(REQUEST_ID, value);
    }
    let response = AppState::test().router().oneshot(builder.body(Body::empty()).unwrap()).await.expect("request");
    let status = response.status();
    let id = response
        .headers()
        .get(HeaderName::from_static(REQUEST_ID))
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
    (status, id)
}

#[tokio::test]
async fn health_response_carries_generated_request_id() {
    let (status, id) = health_request_id(None).await;
    assert_eq!(status, StatusCode::OK);
    let id = id.expect("router must attach x-request-id to every response");
    assert!(Uuid::parse_str(&id).is_ok(), "generated id must be a UUID, got {id:?}");
}

#[tokio::test]
async fn health_response_echoes_safe_inbound_request_id() {
    let (status, id) = health_request_id(Some("edge-req-42")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(id.as_deref(), Some("edge-req-42"), "a safe inbound id must be echoed end to end");
}
