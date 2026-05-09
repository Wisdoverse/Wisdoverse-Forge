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
async fn reviews_return_service_unavailable_without_store() {
    let app = AppState::test_internal_token("secret-token").router();

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/reviews")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(app, req).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, serde_json::json!({"ok": false, "error": "database not configured"}));
}

#[tokio::test]
async fn reviews_support_create_comment_and_approve_round_trip() {
    let app = AppState::test_review_internal_token("secret-token", "org-test", "cli-user").router();

    let create_task_req = Request::builder()
        .method("POST")
        .uri("/api/v1/tasks")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Review task"}"#))
        .unwrap();
    let (status, task_created) = json_response(app.clone(), create_task_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = task_created["task"]["id"].as_str().expect("task id").to_string();

    let create_review_req = Request::builder()
        .method("POST")
        .uri("/api/v1/reviews")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"taskId":"{}","sessionId":"session-123"}}"#, task_id)))
        .unwrap();
    let (status, review_created) = json_response(app.clone(), create_review_req).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(review_created["review"]["state"], "pending");
    assert_eq!(review_created["review"]["diffRef"], "manual");
    let review_id = review_created["review"]["id"].as_str().expect("review id").to_string();

    let list_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/reviews?taskId={task_id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, listed) = json_response(app.clone(), list_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["reviews"].as_array().expect("reviews").len(), 1);

    let comment_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/reviews/{review_id}/comments"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"body":"Looks good","filePath":"src/main.rs","line":12}"#))
        .unwrap();
    let (status, comment_created) = json_response(app.clone(), comment_req).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(comment_created["comment"]["body"], "Looks good");

    let get_review_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/reviews/{review_id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, fetched_review) = json_response(app.clone(), get_review_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched_review["review"]["id"], review_id);
    assert_eq!(fetched_review["review"]["comments"].as_array().expect("comments").len(), 1);

    let approve_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/reviews/{review_id}/approve"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, approved) = json_response(app.clone(), approve_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["state"], "approved");

    let get_task_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/tasks/{task_id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, fetched_task) = json_response(app, get_task_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched_task["task"]["state"], "completed");
}

#[tokio::test]
async fn reviews_reject_moves_task_to_changes_requested() {
    let app = AppState::test_review_internal_token("secret-token", "org-test", "cli-user").router();

    let create_task_req = Request::builder()
        .method("POST")
        .uri("/api/v1/tasks")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Reject review task"}"#))
        .unwrap();
    let (status, task_created) = json_response(app.clone(), create_task_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = task_created["task"]["id"].as_str().expect("task id").to_string();

    let create_review_req = Request::builder()
        .method("POST")
        .uri("/api/v1/reviews")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"taskId":"{}","sessionId":"session-123"}}"#, task_id)))
        .unwrap();
    let (status, review_created) = json_response(app.clone(), create_review_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let review_id = review_created["review"]["id"].as_str().expect("review id").to_string();

    let reject_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/reviews/{review_id}/reject"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"feedback":"needs work"}"#))
        .unwrap();
    let (status, rejected) = json_response(app.clone(), reject_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rejected["state"], "changes_requested");

    let get_task_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/tasks/{task_id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, fetched_task) = json_response(app, get_task_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched_task["task"]["state"], "changes_requested");
}
