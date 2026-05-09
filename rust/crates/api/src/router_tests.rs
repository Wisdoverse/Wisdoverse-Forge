//! Route integration tests using axum test utilities.
//!
//! These tests exercise HTTP routing, middleware layers, CORS, error handling,
//! and panic recovery WITHOUT requiring a running database or external services.
//! Instead, we construct minimal routers with just the layers/handlers under test.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::get;
use serde_json::Value;
use tower::ServiceExt; // for oneshot

use agentforge_core::{AppError, ErrorKind};

/// Helper: send a request through a router and return (status, body JSON).
async fn oneshot_json(app: Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("response not JSON: {e}"));
    (status, json)
}

/// Helper: send a request and return just the status code.
async fn oneshot_status(app: Router, req: Request<Body>) -> StatusCode {
    app.oneshot(req).await.unwrap().status()
}

// ── Health endpoint ──────────────────────────────────────────────────

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let app = Router::new().route("/health", get(crate::health::health));

    let req = Request::builder().uri("/health").body(Body::empty()).unwrap();
    let (status, body) = oneshot_json(app, req).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["status"], "healthy");
}

#[tokio::test]
async fn health_endpoint_with_middleware_stack() {
    let app = Router::new()
        .route("/health", get(crate::health::health))
        .layer(crate::middleware::cors_layer(false, None))
        .layer(crate::middleware::trace_layer())
        .layer(crate::middleware::catch_panic_layer());

    let req = Request::builder().uri("/health").body(Body::empty()).unwrap();
    let (status, body) = oneshot_json(app, req).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
}

// ── Unknown routes ───────────────────────────────────────────────────

#[tokio::test]
async fn unknown_route_returns_404() {
    let app = Router::new().route("/health", get(crate::health::health));

    let req = Request::builder().uri("/nonexistent").body(Body::empty()).unwrap();
    let status = oneshot_status(app, req).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn wrong_method_returns_405() {
    let app = Router::new().route("/health", get(crate::health::health));

    let req = Request::builder().method("POST").uri("/health").body(Body::empty()).unwrap();
    let status = oneshot_status(app, req).await;

    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

// ── Protected route without auth ─────────────────────────────────────

#[tokio::test]
async fn protected_route_without_auth_header_returns_401() {
    // The `me` handler uses AuthUser extractor which requires Authorization header
    // and JwtManager in extensions. Without the header, it should return 401.
    let app = Router::new().route("/api/v1/me", get(crate::routes::auth::me));

    let req = Request::builder().uri("/api/v1/me").body(Body::empty()).unwrap();
    let (status, body) = oneshot_json(app, req).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");
}

#[tokio::test]
async fn protected_route_with_invalid_bearer_returns_error() {
    // Has an Authorization header but no JwtManager in extensions.
    // The extractor should return Internal error (missing JwtManager).
    let app = Router::new().route("/api/v1/me", get(crate::routes::auth::me));

    let req = Request::builder()
        .uri("/api/v1/me")
        .header(header::AUTHORIZATION, "Bearer fake.token.here")
        .body(Body::empty())
        .unwrap();
    let (status, body) = oneshot_json(app, req).await;

    // Missing JwtManager → Internal error (500)
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "INTERNAL_ERROR");
    // Must not leak internal details
    assert_eq!(body["error"]["message"], "Internal server error");
}

#[tokio::test]
async fn protected_route_with_jwt_manager_but_bad_token_returns_401() {
    use agentforge_auth::JwtManager;
    use axum::Extension;
    use std::sync::Arc;

    let jwt = Arc::new(JwtManager::new("test-secret-at-least-32-chars-long!!", 3600));
    let app = Router::new().route("/api/v1/me", get(crate::routes::auth::me)).layer(Extension(jwt));

    let req = Request::builder()
        .uri("/api/v1/me")
        .header(header::AUTHORIZATION, "Bearer invalid.jwt.token")
        .body(Body::empty())
        .unwrap();
    let (status, body) = oneshot_json(app, req).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");
}

// ── CORS ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn cors_dev_mode_allows_any_origin() {
    let app =
        Router::new().route("/health", get(crate::health::health)).layer(crate::middleware::cors_layer(false, None));

    let req = Request::builder().uri("/health").header("Origin", "http://localhost:4002").body(Body::empty()).unwrap();
    let response = app.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("access-control-allow-origin"));
}

#[tokio::test]
async fn cors_preflight_returns_200() {
    let app =
        Router::new().route("/health", get(crate::health::health)).layer(crate::middleware::cors_layer(false, None));

    let req = Request::builder()
        .method("OPTIONS")
        .uri("/health")
        .header("Origin", "http://localhost:4002")
        .header("Access-Control-Request-Method", "GET")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("access-control-allow-origin"));
    assert!(response.headers().contains_key("access-control-allow-methods"));
}

#[tokio::test]
async fn cors_production_with_configured_origin() {
    let app = Router::new()
        .route("/health", get(crate::health::health))
        .layer(crate::middleware::cors_layer(true, Some("https://app.example.com")));

    let req =
        Request::builder().uri("/health").header("Origin", "https://app.example.com").body(Body::empty()).unwrap();
    let response = app.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let allow_origin = response.headers().get("access-control-allow-origin").and_then(|v| v.to_str().ok());
    assert_eq!(allow_origin, Some("https://app.example.com"));
}

// ── Catch-panic layer ────────────────────────────────────────────────

#[tokio::test]
async fn catch_panic_returns_json_500() {
    async fn panicking_handler() -> &'static str {
        panic!("deliberate test panic");
    }

    let app = Router::new().route("/panic", get(panicking_handler)).layer(crate::middleware::catch_panic_layer());

    let req = Request::builder().uri("/panic").body(Body::empty()).unwrap();
    let (status, body) = oneshot_json(app, req).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "INTERNAL_ERROR");
    assert_eq!(body["error"]["message"], "Internal server error");
}

#[tokio::test]
async fn catch_panic_does_not_leak_panic_message() {
    async fn panicking_handler() -> &'static str {
        panic!("secret internal details");
    }

    let app = Router::new().route("/panic", get(panicking_handler)).layer(crate::middleware::catch_panic_layer());

    let req = Request::builder().uri("/panic").body(Body::empty()).unwrap();
    let (_, body) = oneshot_json(app, req).await;

    let msg = body["error"]["message"].as_str().unwrap();
    assert!(!msg.contains("secret"), "panic message must not leak to client");
}

// ── Error response format ────────────────────────────────────────────

#[tokio::test]
async fn handler_returning_app_error_produces_correct_envelope() {
    async fn not_found_handler() -> Result<String, AppError> {
        Err(ErrorKind::NotFound("test item".into()).into())
    }

    let app = Router::new().route("/err", get(not_found_handler));

    let req = Request::builder().uri("/err").body(Body::empty()).unwrap();
    let (status, body) = oneshot_json(app, req).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
    assert!(body["error"]["message"].as_str().unwrap().contains("test item"));
}

#[tokio::test]
async fn validation_error_returns_400() {
    async fn validation_handler() -> Result<String, AppError> {
        Err(ErrorKind::Validation("name is required".into()).into())
    }

    let app = Router::new().route("/err", get(validation_handler));
    let req = Request::builder().uri("/err").body(Body::empty()).unwrap();
    let (status, body) = oneshot_json(app, req).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn conflict_error_returns_409() {
    async fn conflict_handler() -> Result<String, AppError> {
        Err(ErrorKind::Conflict("duplicate name".into()).into())
    }

    let app = Router::new().route("/err", get(conflict_handler));
    let req = Request::builder().uri("/err").body(Body::empty()).unwrap();
    let (status, body) = oneshot_json(app, req).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "CONFLICT");
}

#[tokio::test]
async fn forbidden_error_returns_403() {
    async fn forbidden_handler() -> Result<String, AppError> {
        Err(ErrorKind::Forbidden.into())
    }

    let app = Router::new().route("/err", get(forbidden_handler));
    let req = Request::builder().uri("/err").body(Body::empty()).unwrap();
    let (status, body) = oneshot_json(app, req).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "FORBIDDEN");
}

#[tokio::test]
async fn internal_error_returns_500_without_leaking_details() {
    async fn internal_handler() -> Result<String, AppError> {
        Err(anyhow::anyhow!("database password leaked").into())
    }

    let app = Router::new().route("/err", get(internal_handler));
    let req = Request::builder().uri("/err").body(Body::empty()).unwrap();
    let (status, body) = oneshot_json(app, req).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"]["code"], "INTERNAL_ERROR");
    assert_eq!(body["error"]["message"], "Internal server error");
    assert!(!body.to_string().contains("database password"));
}

// ── Middleware ordering ──────────────────────────────────────────────

#[tokio::test]
async fn full_middleware_stack_catches_panic() {
    async fn panicking_handler() -> &'static str {
        panic!("should be caught by outermost layer");
    }

    // Mirrors the middleware order from create_router:
    // CORS (innermost) → Trace → CatchPanic (outermost)
    let app = Router::new()
        .route("/test", get(panicking_handler))
        .layer(crate::middleware::cors_layer(false, None))
        .layer(crate::middleware::trace_layer())
        .layer(crate::middleware::catch_panic_layer());

    let req = Request::builder().uri("/test").body(Body::empty()).unwrap();
    let (status, body) = oneshot_json(app, req).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["ok"], false);
}

#[tokio::test]
async fn error_handler_works_through_full_middleware_stack() {
    async fn error_handler() -> Result<String, AppError> {
        Err(ErrorKind::NotFound("widget".into()).into())
    }

    let app = Router::new()
        .route("/widget", get(error_handler))
        .layer(crate::middleware::cors_layer(false, None))
        .layer(crate::middleware::trace_layer())
        .layer(crate::middleware::catch_panic_layer());

    let req = Request::builder().uri("/widget").header("Origin", "http://localhost:4002").body(Body::empty()).unwrap();
    let response = app.oneshot(req).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    // CORS headers should still be present even on error responses
    assert!(response.headers().contains_key("access-control-allow-origin"));

    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
}

// ── Request body parsing ─────────────────────────────────────────────
// These tests verify Axum's JSON extractor behavior using a standalone
// handler (no AppState needed) that accepts a typed JSON body.

#[tokio::test]
async fn post_with_invalid_json_returns_422() {
    use axum::Json;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct TestBody {
        _name: String,
    }

    async fn handler(Json(_body): Json<TestBody>) -> &'static str {
        "ok"
    }

    let app = Router::new().route("/test", axum::routing::post(handler));

    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("not valid json"))
        .unwrap();
    let status = oneshot_status(app, req).await;

    // Axum's Json extractor returns 422 for malformed JSON
    assert!(
        status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::BAD_REQUEST,
        "expected 422 or 400 for invalid JSON, got {status}"
    );
}

#[tokio::test]
async fn post_with_missing_required_fields_returns_422() {
    use axum::Json;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct TestLoginBody {
        _email: String,
        _password: String,
    }

    async fn handler(Json(_body): Json<TestLoginBody>) -> &'static str {
        "ok"
    }

    let app = Router::new().route("/test", axum::routing::post(handler));

    // Valid JSON but missing required "password" field
    let req = Request::builder()
        .method("POST")
        .uri("/test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"_email":"dev@example.com"}"#))
        .unwrap();
    let status = oneshot_status(app, req).await;

    assert!(
        status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::BAD_REQUEST,
        "expected 422 or 400 for missing fields, got {status}"
    );
}

// ── All ErrorKind→StatusCode mappings via IntoResponse ───────────────

#[tokio::test]
async fn all_error_kinds_produce_correct_status_codes() {
    use axum::response::IntoResponse;

    let cases: Vec<(ErrorKind, StatusCode, &str)> = vec![
        (ErrorKind::NotFound("x".into()), StatusCode::NOT_FOUND, "NOT_FOUND"),
        (ErrorKind::Validation("x".into()), StatusCode::BAD_REQUEST, "VALIDATION_ERROR"),
        (ErrorKind::Unauthorized, StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
        (ErrorKind::Forbidden, StatusCode::FORBIDDEN, "FORBIDDEN"),
        (ErrorKind::Conflict("x".into()), StatusCode::CONFLICT, "CONFLICT"),
        (ErrorKind::Unavailable("x".into()), StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE"),
    ];

    for (kind, expected_status, expected_code) in cases {
        let label = format!("{kind:?}");
        let error: AppError = kind.into();
        let response = error.into_response();

        assert_eq!(response.status(), expected_status, "status mismatch for {label}");

        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["ok"], false, "ok must be false for {label}");
        assert_eq!(body["error"]["code"], expected_code, "code mismatch for {label}");
    }
}
