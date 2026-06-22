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
    // Task-state atomicity (both code_reviews and tasks updated in one transaction) is
    // verified by the PgReviewStore sqlx test `review_verdict_tx`.  The in-memory
    // MemoryStore double is single-aggregate and intentionally does not mirror tasks.
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
    // Task-state atomicity (both code_reviews and tasks updated in one transaction) is
    // verified by the PgReviewStore sqlx test `review_verdict_tx`.  The in-memory
    // MemoryStore double is single-aggregate and intentionally does not mirror tasks.
}

#[tokio::test]
async fn reject_without_feedback_returns_400() {
    let app = AppState::test_review_internal_token("secret-token", "org-test", "cli-user").router();

    let create_task_req = Request::builder()
        .method("POST")
        .uri("/api/v1/tasks")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Reject task"}"#))
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
        .body(Body::from(format!(r#"{{"taskId":"{}","sessionId":"session-abc"}}"#, task_id)))
        .unwrap();
    let (status, review_created) = json_response(app.clone(), create_review_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let review_id = review_created["review"]["id"].as_str().expect("review id").to_string();

    // Reject with missing feedback -> 400
    let reject_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/reviews/{review_id}/reject"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{}"#))
        .unwrap();
    let (status, body) = json_response(app.clone(), reject_req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["ok"], false);

    // Reject with empty feedback -> 400
    let reject_empty_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/reviews/{review_id}/reject"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"feedback":"   "}"#))
        .unwrap();
    let (status, body) = json_response(app.clone(), reject_empty_req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["ok"], false);
}

#[tokio::test]
async fn reject_with_feedback_persists_comment_and_audit() {
    let app = AppState::test_review_internal_token("secret-token", "org-test", "cli-user").router();

    let create_task_req = Request::builder()
        .method("POST")
        .uri("/api/v1/tasks")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Reject with feedback"}"#))
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
        .body(Body::from(format!(r#"{{"taskId":"{}","sessionId":"session-456"}}"#, task_id)))
        .unwrap();
    let (status, review_created) = json_response(app.clone(), create_review_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let review_id = review_created["review"]["id"].as_str().expect("review id").to_string();

    let reject_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/reviews/{review_id}/reject"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "reviewer-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"feedback":"needs major refactoring"}"#))
        .unwrap();
    let (status, rejected) = json_response(app.clone(), reject_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rejected["state"], "changes_requested");

    // Comment should be persisted on the review
    let get_review_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/reviews/{review_id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, fetched_review) = json_response(app.clone(), get_review_req).await;
    assert_eq!(status, StatusCode::OK);
    let comments = fetched_review["review"]["comments"].as_array().expect("comments");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["body"], "needs major refactoring");

    // Audit store should have a ReviewReject entry
    let audit_req = Request::builder()
        .method("GET")
        .uri("/api/v1/audit?resource=review")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, audit_body) = json_response(app.clone(), audit_req).await;
    assert_eq!(status, StatusCode::OK);
    let logs = audit_body["logs"].as_array().expect("logs");
    let has_reject = logs.iter().any(|log| log["action"] == "review.reject");
    assert!(has_reject, "expected a review.reject audit log entry");
}

#[tokio::test]
async fn approve_terminal_review_returns_409() {
    let app = AppState::test_review_internal_token("secret-token", "org-test", "cli-user").router();

    let create_task_req = Request::builder()
        .method("POST")
        .uri("/api/v1/tasks")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Terminal approve test"}"#))
        .unwrap();
    let (status, task_created) = json_response(app.clone(), create_task_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = task_created["task"]["id"].as_str().expect("task id").to_string();

    let create_review_req = Request::builder()
        .method("POST")
        .uri("/api/v1/reviews")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "creator-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"taskId":"{}","sessionId":"session-789"}}"#, task_id)))
        .unwrap();
    let (status, review_created) = json_response(app.clone(), create_review_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let review_id = review_created["review"]["id"].as_str().expect("review id").to_string();

    // First approve (by a different user) -> 200
    let approve_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/reviews/{review_id}/approve"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "different-reviewer")
        .body(Body::empty())
        .unwrap();
    let (status, _) = json_response(app.clone(), approve_req).await;
    assert_eq!(status, StatusCode::OK);

    // Second approve -> 409 Conflict (already terminal)
    let approve_again_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/reviews/{review_id}/approve"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "another-reviewer")
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(app.clone(), approve_again_req).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["ok"], false);
}

#[tokio::test]
async fn creator_cannot_approve_own_review() {
    let app = AppState::test_review_internal_token("secret-token", "org-test", "cli-user").router();

    let create_task_req = Request::builder()
        .method("POST")
        .uri("/api/v1/tasks")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Self-approve test"}"#))
        .unwrap();
    let (status, task_created) = json_response(app.clone(), create_task_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = task_created["task"]["id"].as_str().expect("task id").to_string();

    // Create review as "the-creator"
    let create_review_req = Request::builder()
        .method("POST")
        .uri("/api/v1/reviews")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "the-creator")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"taskId":"{}","sessionId":"session-self"}}"#, task_id)))
        .unwrap();
    let (status, review_created) = json_response(app.clone(), create_review_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let review_id = review_created["review"]["id"].as_str().expect("review id").to_string();

    // Attempt to approve as the same user -> 403
    let approve_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/reviews/{review_id}/approve"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "the-creator")
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(app.clone(), approve_req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["ok"], false);
}

#[tokio::test]
async fn valid_approve_by_different_user_succeeds_with_audit() {
    let app = AppState::test_review_internal_token("secret-token", "org-test", "cli-user").router();

    let create_task_req = Request::builder()
        .method("POST")
        .uri("/api/v1/tasks")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Approve audit test"}"#))
        .unwrap();
    let (status, task_created) = json_response(app.clone(), create_task_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = task_created["task"]["id"].as_str().expect("task id").to_string();

    // Create review as "owner-user"
    let create_review_req = Request::builder()
        .method("POST")
        .uri("/api/v1/reviews")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "owner-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"taskId":"{}","sessionId":"session-diff"}}"#, task_id)))
        .unwrap();
    let (status, review_created) = json_response(app.clone(), create_review_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let review_id = review_created["review"]["id"].as_str().expect("review id").to_string();

    // Approve as "reviewer-user" (different from "owner-user") -> 200
    let approve_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/reviews/{review_id}/approve"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "reviewer-user")
        .body(Body::empty())
        .unwrap();
    let (status, approved) = json_response(app.clone(), approve_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["state"], "approved");

    // Audit store should have a ReviewApprove entry
    let audit_req = Request::builder()
        .method("GET")
        .uri("/api/v1/audit?resource=review")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, audit_body) = json_response(app.clone(), audit_req).await;
    assert_eq!(status, StatusCode::OK);
    let logs = audit_body["logs"].as_array().expect("logs");
    let has_approve = logs.iter().any(|log| log["action"] == "review.approve");
    assert!(has_approve, "expected a review.approve audit log entry");
}
