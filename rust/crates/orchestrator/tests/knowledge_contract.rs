use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tower::ServiceExt;

use agentforge_orchestrator::state::AppState;

async fn json_response(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(req).await.expect("request should succeed");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");
    (status, json)
}

#[tokio::test]
async fn knowledge_uses_request_identity_and_org_scope() {
    let app_state = AppState::test_internal_token("secret-token");
    let app = app_state.clone().router();

    let create_req = Request::builder()
        .method("POST")
        .uri("/api/v1/knowledge")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-alpha")
        .header("X-User-ID", "user-alpha")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Contract","content":"Initial content"}"#))
        .unwrap();
    let (status, created) = json_response(app.clone(), create_req).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["ok"], true);
    assert_eq!(created["entry"]["orgId"], "org-alpha");
    assert_eq!(created["entry"]["createdBy"], "user-alpha");
    let id = created["entry"]["id"].as_str().expect("id").to_string();

    let bulk_req = Request::builder()
        .method("POST")
        .uri("/api/v1/knowledge/bulk-index")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-alpha")
        .body(Body::empty())
        .unwrap();
    let (status, bulk) = json_response(app.clone(), bulk_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bulk["ok"], true);
    assert_eq!(bulk["submitted"], 1, "bulk-index should resubmit the pending entry");

    let wrong_org_get = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/knowledge/{id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-beta")
        .body(Body::empty())
        .unwrap();
    let (status, missing) = json_response(app.clone(), wrong_org_get).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing, serde_json::json!({"ok": false, "error": "entry not found"}));

    let update_req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/knowledge/{id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-alpha")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"content":"Updated content"}"#))
        .unwrap();
    let (status, updated) = json_response(app.clone(), update_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["entry"]["content"], "Updated content");
    assert_eq!(updated["entry"]["orgId"], "org-alpha");

    let get_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/knowledge/{id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-alpha")
        .body(Body::empty())
        .unwrap();
    let (status, got) = json_response(app.clone(), get_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["entry"]["content"], "Updated content");
    assert_eq!(got["entry"]["orgId"], "org-alpha");

    let delete_req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/knowledge/{id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-alpha")
        .body(Body::empty())
        .unwrap();
    let (status, deleted) = json_response(app, delete_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted["ok"], true);
}

#[tokio::test]
async fn knowledge_error_contract_matches_go() {
    let mut unavailable_state = AppState::test_internal_token("secret-token");
    unavailable_state.knowledge = None;
    let unavailable_app = unavailable_state.router();

    let unavailable_req = Request::builder()
        .method("POST")
        .uri("/api/v1/knowledge")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-alpha")
        .header("X-User-ID", "user-alpha")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Contract","content":"Initial content"}"#))
        .unwrap();
    let (status, unavailable) = json_response(unavailable_app, unavailable_req).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(unavailable, serde_json::json!({"ok": false, "error": "knowledge service not configured"}));

    let app = AppState::test_internal_token("secret-token").router();

    let missing_title_req = Request::builder()
        .method("POST")
        .uri("/api/v1/knowledge")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-alpha")
        .header("X-User-ID", "user-alpha")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"content":"Initial content"}"#))
        .unwrap();
    let (status, missing_title) = json_response(app.clone(), missing_title_req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(missing_title, serde_json::json!({"ok": false, "error": "title is required"}));

    let missing_content_req = Request::builder()
        .method("POST")
        .uri("/api/v1/knowledge")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-alpha")
        .header("X-User-ID", "user-alpha")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Contract"}"#))
        .unwrap();
    let (status, missing_content) = json_response(app, missing_content_req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(missing_content, serde_json::json!({"ok": false, "error": "content is required"}));
}
