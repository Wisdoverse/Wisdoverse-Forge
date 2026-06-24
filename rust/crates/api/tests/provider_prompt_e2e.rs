//! End-to-end HTTP tests for the provider+prompt agent chat loop (#21 T11-T12).
//! Stands up the full Axum router with a mock LLM provider and exercises:
//!   - POST /api/v1/agents/:id/prompt on a provider+prompt agent → SSE frames
//!   - POST /api/v1/agents/:id/prompt on a CLI-tool agent → JSON "sent"
//!   - GET /api/v1/agents/:id/messages → tenant-scoped chronological list
//!   - DELETE /api/v1/agents/:id/messages → wipes and returns `deleted` count

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use agentforge_api::test_support::{seed_cli_agent, seed_provider_agent, test_app_with_mock_provider};

mod common;

fn auth_header(token: &str) -> String {
    format!("Bearer {token}")
}

#[sqlx::test(migrations = "../db/migrations")]
async fn post_prompt_provider_branch_emits_sse_frames(pool: sqlx::PgPool) {
    let seed = seed_provider_agent(&pool, "mock", "claude-sonnet-4-6").await;
    let app = test_app_with_mock_provider(pool, "mock", "hi from mock").await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/agents/{}/prompt", seed.agent_id.as_uuid()))
                .header("content-type", "application/json")
                .header("authorization", auth_header(&seed.jwt))
                .body(Body::from(r#"{"content":"hello"}"#))
                .unwrap(),
        )
        .await
        .expect("request");

    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.starts_with("text/event-stream"), "expected text/event-stream, got {ct}");

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("event: message_start"), "missing message_start frame: {body_str}");
    assert!(body_str.contains("event: delta"), "missing delta frame");
    assert!(body_str.contains("hi from mock"), "missing mock reply text in delta payload");
    assert!(body_str.contains("event: message_stop"), "missing message_stop frame");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn post_prompt_cli_agent_returns_json_sent(pool: sqlx::PgPool) {
    let seed = seed_provider_agent(&pool, "mock", "claude-sonnet-4-6").await;
    // Second agent on the same tenant, this one with cli_tool set.
    let cli_agent = seed_cli_agent(&pool, seed.org_id.as_uuid(), seed.user_id.as_uuid(), "claude").await;
    let app = test_app_with_mock_provider(pool, "mock", "ignored").await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/agents/{}/prompt", cli_agent.as_uuid()))
                .header("content-type", "application/json")
                .header("authorization", auth_header(&seed.jwt))
                .body(Body::from(r#"{"content":"hello"}"#))
                .unwrap(),
        )
        .await
        .expect("request");

    // NATS is degraded (no URL), so the publish is a no-op but should still respond 200.
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.starts_with("application/json"), "expected application/json, got {ct}");

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["status"], "sent");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn get_messages_returns_tenant_scoped_list(pool: sqlx::PgPool) {
    let seed = seed_provider_agent(&pool, "mock", "claude-sonnet-4-6").await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "pong").await;

    // First: send a prompt so the DB has user + assistant rows.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/agents/{}/prompt", seed.agent_id.as_uuid()))
                .header("content-type", "application/json")
                .header("authorization", auth_header(&seed.jwt))
                .body(Body::from(r#"{"content":"ping"}"#))
                .unwrap(),
        )
        .await
        .expect("prompt");
    // Drain the SSE body so the finalize block persists the assistant row.
    let _ = axum::body::to_bytes(resp.into_body(), usize::MAX).await;

    // Then: fetch the list.
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/agents/{}/messages", seed.agent_id.as_uuid()))
                .header("authorization", auth_header(&seed.jwt))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("list");

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["ok"], true);
    let messages = v["messages"].as_array().expect("messages array");
    assert!(messages.len() >= 2, "expected user + assistant rows, got {}", messages.len());
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"], "ping");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"], "pong");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn delete_messages_clears_history(pool: sqlx::PgPool) {
    let seed = seed_provider_agent(&pool, "mock", "claude-sonnet-4-6").await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "x").await;

    // Send a prompt to create rows.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/agents/{}/prompt", seed.agent_id.as_uuid()))
                .header("content-type", "application/json")
                .header("authorization", auth_header(&seed.jwt))
                .body(Body::from(r#"{"content":"bye"}"#))
                .unwrap(),
        )
        .await
        .expect("prompt");
    let _ = axum::body::to_bytes(resp.into_body(), usize::MAX).await;

    // DELETE /messages.
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/agents/{}/messages", seed.agent_id.as_uuid()))
                .header("authorization", auth_header(&seed.jwt))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("delete");

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["ok"], true);
    let deleted = v["deleted"].as_u64().unwrap();
    assert!(deleted >= 2, "expected ≥2 rows deleted, got {deleted}");
}
