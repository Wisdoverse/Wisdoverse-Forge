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

// model-capability follow-up: the image gate is model-aware. A text-only model on
// an otherwise vision-capable provider must reject an image prompt naming the
// MODEL (and must reject before touching the bogus attachment, proving order).
#[sqlx::test(migrations = "../db/migrations")]
async fn post_prompt_rejects_images_for_text_only_model(pool: sqlx::PgPool) {
    let seed = seed_provider_agent(&pool, "openai", "gpt-3.5-turbo").await;
    let app = test_app_with_mock_provider(pool, "openai", "unused").await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/agents/{}/prompt", seed.agent_id.as_uuid()))
                .header("content-type", "application/json")
                .header("authorization", auth_header(&seed.jwt))
                .body(Body::from(r#"{"content":"look at this","images":["00000000-0000-0000-0000-000000000001"]}"#))
                .unwrap(),
        )
        .await
        .expect("request");

    assert!(resp.status().is_client_error(), "text-only model + images must be a client error, got {}", resp.status());
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("gpt-3.5-turbo"), "error must name the model: {body_str}");
    assert!(body_str.contains("does not support image input"), "error must explain the gate: {body_str}");
}

// model-capability follow-up: a vision-capable model on the same provider passes
// the model gate (it then fails on the non-existent attachment, NOT on the
// model) — proving the gate no longer blocks legitimate vision models.
#[sqlx::test(migrations = "../db/migrations")]
async fn post_prompt_passes_model_gate_for_vision_model(pool: sqlx::PgPool) {
    let seed = seed_provider_agent(&pool, "openai", "gpt-4o").await;
    let app = test_app_with_mock_provider(pool, "openai", "unused").await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/agents/{}/prompt", seed.agent_id.as_uuid()))
                .header("content-type", "application/json")
                .header("authorization", auth_header(&seed.jwt))
                .body(Body::from(r#"{"content":"look at this","images":["00000000-0000-0000-0000-000000000001"]}"#))
                .unwrap(),
        )
        .await
        .expect("request");

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = std::str::from_utf8(&body).unwrap();
    // The model gate must NOT be what rejects this; the bogus attachment is.
    assert!(!body_str.contains("does not support image input"), "vision model wrongly blocked by gate: {body_str}");
}
