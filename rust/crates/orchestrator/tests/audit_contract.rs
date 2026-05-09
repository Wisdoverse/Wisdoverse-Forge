use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use chrono::Utc;
use serde_json::Value;
use tower::ServiceExt;

use agentforge_orchestrator::audit::{AuditAction, AuditLog};
use agentforge_orchestrator::state::AppState;

async fn json_response(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(req).await.expect("request should succeed");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");
    (status, json)
}

async fn text_response(app: axum::Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, String) {
    let response = app.oneshot(req).await.expect("request should succeed");
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), 256 * 1024).await.expect("body");
    (status, headers, String::from_utf8(body.to_vec()).expect("utf8"))
}

#[tokio::test]
async fn audit_returns_service_unavailable_without_store() {
    let app = AppState::test_internal_token("secret-token").router();

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/audit")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(app, req).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, serde_json::json!({"ok": false, "error": "database not configured"}));
}

#[tokio::test]
async fn audit_supports_list_filter_and_csv_export() {
    let state = AppState::test_audit_internal_token("secret-token", "org-test");
    let store = state.audit_store.as_ref().expect("audit store").clone();

    store
        .create(&mut AuditLog {
            id: String::new(),
            action: AuditAction::TaskCreate,
            actor_id: "user-1".to_string(),
            actor_type: "human".to_string(),
            resource: "task".to_string(),
            resource_id: Some("task-1".to_string()),
            org_id: "org-test".to_string(),
            changes: None,
            ip_address: Some("127.0.0.1".to_string()),
            user_agent: Some("contract-test".to_string()),
            created_at: Utc::now(),
        })
        .await
        .expect("create log");
    store
        .create(&mut AuditLog {
            id: String::new(),
            action: AuditAction::ReviewApprove,
            actor_id: "user-2".to_string(),
            actor_type: "human".to_string(),
            resource: "review".to_string(),
            resource_id: Some("review-1".to_string()),
            org_id: "org-test".to_string(),
            changes: None,
            ip_address: None,
            user_agent: None,
            created_at: Utc::now(),
        })
        .await
        .expect("create log");

    let app = state.router();

    let list_req = Request::builder()
        .method("GET")
        .uri("/api/v1/audit?action=task.create&limit=10&offset=0")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(app.clone(), list_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["total"], 1);
    assert_eq!(body["limit"], 10);
    assert_eq!(body["offset"], 0);
    assert_eq!(body["logs"].as_array().expect("logs").len(), 1);
    assert_eq!(body["logs"][0]["action"], "task.create");

    let export_req = Request::builder()
        .method("GET")
        .uri("/api/v1/audit/export")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, headers, csv) = text_response(app, export_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()), Some("text/csv"));
    assert!(csv.contains("id,action,actor_id,actor_type,resource,resource_id,org_id,ip_address,user_agent,created_at"));
    assert!(csv.contains("task.create"));
    assert!(csv.contains("review.approve"));
}

#[tokio::test]
async fn audit_validates_rfc3339_filters() {
    let app = AppState::test_audit_internal_token("secret-token", "org-test").router();

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/audit?from=not-a-date")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(app, req).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, serde_json::json!({"ok": false, "error": "invalid 'from' date, expected RFC3339 format"}));
}
