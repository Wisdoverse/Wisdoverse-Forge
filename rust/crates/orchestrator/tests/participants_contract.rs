use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tower::ServiceExt;

use agentforge_orchestrator::auth::Provisioner;
use agentforge_orchestrator::state::AppState;

async fn json_response(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(req).await.expect("request should succeed");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");
    (status, json)
}

#[tokio::test]
async fn participants_create_and_list_provisioned_humans() {
    let mut state = AppState::test_internal_token("secret-token");
    state.provisioner = Some(Arc::new(Provisioner::new()));
    let app = state.router();

    let create_req = Request::builder()
        .method("POST")
        .uri("/api/v1/participants")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"userId":"cli-user","displayName":"CLI User"}"#))
        .unwrap();
    let (status, created) = json_response(app.clone(), create_req).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["ok"], true);
    assert_eq!(created["participant"]["type"], "human");
    assert_eq!(created["participant"]["userId"], "cli-user");
    assert_eq!(created["participant"]["displayName"], "CLI User");
    assert_eq!(created["participant"]["orgId"], "org-test");
    let participant_id = created["participant"]["id"].as_str().expect("participant id").to_string();
    assert!(participant_id.starts_with("p-"));

    let recreate_req = Request::builder()
        .method("POST")
        .uri("/api/v1/participants")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"userId":"cli-user","displayName":"CLI User Renamed"}"#))
        .unwrap();
    let (status, recreated) = json_response(app.clone(), recreate_req).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(recreated["participant"]["id"], created["participant"]["id"]);
    assert_eq!(recreated["participant"]["displayName"], "CLI User Renamed");

    let list_req = Request::builder()
        .method("GET")
        .uri("/api/v1/participants")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, listed) = json_response(app, list_req).await;

    assert_eq!(status, StatusCode::OK);
    let participants = listed["participants"].as_array().expect("participants array");
    assert_eq!(participants.len(), 1);
    assert_eq!(participants[0]["id"], participant_id);
    assert_eq!(participants[0]["type"], "human");
    assert_eq!(participants[0]["userId"], "cli-user");
    assert_eq!(participants[0]["displayName"], "CLI User Renamed");
    assert_eq!(participants[0]["orgId"], "org-test");
    assert!(participants[0]["createdAt"].as_str().expect("createdAt").contains('T'));
}
