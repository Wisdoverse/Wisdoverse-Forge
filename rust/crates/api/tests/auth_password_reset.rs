use std::sync::{Arc, Mutex};

use agentforge_api::create_router;
use agentforge_api::services::email::{EmailMessage, EmailSender};
use agentforge_api::test_support::app_state_with_mock_provider_and_email_sender;
use agentforge_core::AppResult;
use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

#[derive(Debug, Default)]
struct RecordingEmailSender {
    messages: Arc<Mutex<Vec<EmailMessage>>>,
}

impl RecordingEmailSender {
    fn messages(&self) -> Vec<EmailMessage> {
        self.messages.lock().expect("email recorder lock poisoned").clone()
    }
}

#[async_trait]
impl EmailSender for RecordingEmailSender {
    fn is_configured(&self) -> bool {
        true
    }

    async fn send(&self, message: EmailMessage) -> AppResult<()> {
        self.messages.lock().expect("email recorder lock poisoned").push(message);
        Ok(())
    }
}

async fn post_json(app: Router, path: &str, payload: Value) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (status, body)
}

fn reset_token_from_email(body: &str) -> String {
    let token = body
        .split("reset_token=")
        .nth(1)
        .expect("reset email should include reset_token")
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect::<String>();
    assert!(token.len() >= 32, "token should be high entropy");
    token
}

#[sqlx::test(migrations = "../db/migrations")]
async fn forgot_password_sends_reset_email_and_reset_token_updates_password(pool: sqlx::PgPool) {
    let sender = Arc::new(RecordingEmailSender::default());
    let state = app_state_with_mock_provider_and_email_sender(
        pool,
        "mock",
        "unused",
        sender.clone(),
        Some("https://forge.example.com".to_string()),
    )
    .await;
    let app = create_router(state);

    let email = "reset-e2e@example.com";
    let old_password = ["Old", "Password", "123!"].concat();
    let new_password = ["New", "Password", "123!"].concat();

    let (status, body) = post_json(
        app.clone(),
        "/api/v1/auth/register",
        json!({ "email": email, "password": old_password.clone(), "username": "Reset E2E" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "register body: {body}");

    let (status, body) = post_json(app.clone(), "/api/v1/auth/forgot-password", json!({ "email": email })).await;
    assert_eq!(status, StatusCode::OK, "forgot body: {body}");

    let messages = sender.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].to, email);
    assert!(messages[0].body.contains("https://forge.example.com/login?reset_token="));
    let token = reset_token_from_email(&messages[0].body);

    let (status, body) = post_json(
        app.clone(),
        "/api/v1/auth/reset-password",
        json!({ "token": token, "newPassword": new_password.clone() }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "reset body: {body}");

    let (status, _) =
        post_json(app.clone(), "/api/v1/auth/login", json!({ "email": email, "password": old_password })).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) =
        post_json(app, "/api/v1/auth/login", json!({ "email": email, "password": new_password })).await;
    assert_eq!(status, StatusCode::OK, "new password login body: {body}");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn forgot_password_does_not_send_email_for_unknown_account(pool: sqlx::PgPool) {
    let sender = Arc::new(RecordingEmailSender::default());
    let state = app_state_with_mock_provider_and_email_sender(
        pool,
        "mock",
        "unused",
        sender.clone(),
        Some("https://forge.example.com".to_string()),
    )
    .await;
    let app = create_router(state);

    let (status, body) =
        post_json(app, "/api/v1/auth/forgot-password", json!({ "email": "missing-e2e@example.com" })).await;

    assert_eq!(status, StatusCode::OK, "forgot body: {body}");
    assert!(sender.messages().is_empty());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn reset_password_rejects_invalid_token(pool: sqlx::PgPool) {
    let sender = Arc::new(RecordingEmailSender::default());
    let state = app_state_with_mock_provider_and_email_sender(
        pool,
        "mock",
        "unused",
        sender,
        Some("https://forge.example.com".to_string()),
    )
    .await;
    let app = create_router(state);

    let (status, body) = post_json(
        app,
        "/api/v1/auth/reset-password",
        json!({ "token": "invalid-reset-token-1234567890abcdef", "newPassword": "NewPassword123!" }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "reset body: {body}");
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "VALIDATION_ERROR");
    assert_eq!(body["message"], "invalid or expired reset token");
}
