use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

use agentforge_orchestrator::auth::Provisioner;
use agentforge_orchestrator::state::AppState;
use agentforge_orchestrator::task::MemoryStore as MemoryTaskStore;

const VALID_SIGNING_KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

async fn json_response(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(req).await.expect("request should succeed");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");
    (status, json)
}

#[tokio::test]
async fn health_endpoints_match_go_contract() {
    let app = AppState::test_internal_token("secret-token").router();

    let public_health = Request::builder().method("GET").uri("/health").body(Body::empty()).unwrap();
    let (status, body) = json_response(app.clone(), public_health).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, serde_json::json!({"status": "healthy"}));

    let unauthenticated = Request::builder().method("GET").uri("/api/v1/health").body(Body::empty()).unwrap();
    let (status, body) = json_response(app.clone(), unauthenticated).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, serde_json::json!({"error": "missing authorization header"}));

    let authenticated = Request::builder()
        .method("GET")
        .uri("/api/v1/health")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(app, authenticated).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, serde_json::json!({"ok": true, "status": "healthy", "service": "orchestrator"}));
}

#[tokio::test]
async fn auth_routes_stay_unmounted_without_signing_key() {
    let app = AppState::test_ready().router();

    let me_req = Request::builder().method("GET").uri("/api/v1/auth/me").body(Body::empty()).unwrap();
    let me_response = app.clone().oneshot(me_req).await.expect("request should succeed");
    assert_eq!(me_response.status(), StatusCode::NOT_FOUND);

    let refresh_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/refresh")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"refreshToken":"abc"}"#))
        .unwrap();
    let refresh_response = app.oneshot(refresh_req).await.expect("request should succeed");
    assert_eq!(refresh_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn me_returns_user_payload_for_valid_session_token() {
    let state = AppState::test_with_jwt_signing_key(VALID_SIGNING_KEY);
    let pair = state
        .sessions
        .as_ref()
        .expect("session manager")
        .issue_token_pair("user-1", "user@example.com", "User Example", "org-1")
        .await
        .expect("issue token pair");
    let app = state.router();

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(header::AUTHORIZATION, format!("Bearer {}", pair.access_token))
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(app, req).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["user"]["sub"], "user-1");
    assert_eq!(body["user"]["email"], "user@example.com");
    assert_eq!(body["user"]["displayName"], "User Example");
    assert_eq!(body["user"]["orgId"], "org-1");
    assert_eq!(body["user"]["type"], "human");
    assert!(body["user"]["id"].as_str().expect("participant id").starts_with("p-"));
    assert!(body["user"]["createdAt"].as_str().expect("createdAt").contains('T'));
}

#[tokio::test]
async fn me_rejects_internal_token_without_authentication_context() {
    let state = AppState::test_with_auth(VALID_SIGNING_KEY, "internal-secret");
    let app = state.router();

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(header::AUTHORIZATION, "Bearer internal-secret")
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(app, req).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, serde_json::json!({"ok": false, "error": "missing authentication context"}));
}

#[tokio::test]
async fn refresh_rejects_invalid_body() {
    let app = AppState::test_with_jwt_signing_key(VALID_SIGNING_KEY).router();

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/refresh")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{not-json"))
        .unwrap();
    let (status, body) = json_response(app, req).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, serde_json::json!({"ok": false, "error": "invalid request body"}));
}

#[tokio::test]
async fn refresh_requires_refresh_token() {
    let app = AppState::test_with_jwt_signing_key(VALID_SIGNING_KEY).router();

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/refresh")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{}"#))
        .unwrap();
    let (status, body) = json_response(app, req).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, serde_json::json!({"ok": false, "error": "refreshToken is required"}));
}

#[tokio::test]
async fn refresh_rotates_refresh_tokens() {
    let state = AppState::test_with_jwt_signing_key(VALID_SIGNING_KEY);
    let issued = state
        .sessions
        .as_ref()
        .expect("session manager")
        .issue_token_pair("user-1", "user@example.com", "User Example", "org-1")
        .await
        .expect("issue token pair");
    let app = state.router();

    let refresh_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/refresh")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"refreshToken":"{}"}}"#, issued.refresh_token)))
        .unwrap();
    let (status, body) = json_response(app.clone(), refresh_req).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    let new_access = body["accessToken"].as_str().expect("access token");
    let new_refresh = body["refreshToken"].as_str().expect("refresh token");
    assert!(!new_access.is_empty());
    assert!(!new_refresh.is_empty());
    assert_ne!(new_refresh, issued.refresh_token);
    assert!(body["expiresAt"].as_i64().expect("expiresAt") > 0);

    let replay_req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/refresh")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"refreshToken":"{}"}}"#, issued.refresh_token)))
        .unwrap();
    let (status, body) = json_response(app, replay_req).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, serde_json::json!({"ok": false, "error": "invalid or expired refresh token"}));
}

#[tokio::test]
async fn internal_token_requests_use_provisioned_participant_ids_when_available() {
    let mut state = AppState::test_internal_token("secret-token");
    state.task_store = Some(Arc::new(MemoryTaskStore::new()));
    state.provisioner = Some(Arc::new(Provisioner::new()));
    let app = state.router();

    let create_req = Request::builder()
        .method("POST")
        .uri("/api/v1/tasks")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Provisioned task"}"#))
        .unwrap();
    let (status, created) = json_response(app.clone(), create_req).await;

    assert_eq!(status, StatusCode::CREATED);
    let created_by = created["task"]["createdBy"].as_str().expect("createdBy");
    assert_ne!(created_by, "cli-user");
    assert!(created_by.starts_with("p-"));

    let second_req = Request::builder()
        .method("POST")
        .uri("/api/v1/tasks")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"title":"Provisioned task again"}"#))
        .unwrap();
    let (status, second) = json_response(app, second_req).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["task"]["createdBy"], created["task"]["createdBy"]);
}
