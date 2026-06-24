//! Chat turn endpoint regression tests for issue #32.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

use agentforge_api::test_support::{seed_provider_agent, test_app_with_mock_provider};

fn auth_header(token: &str) -> String {
    format!("Bearer {token}")
}

async fn insert_event(
    pool: &sqlx::PgPool,
    org_id: Uuid,
    agent_id: Uuid,
    id: Uuid,
    event_type: &str,
    payload: Value,
    ms: i64,
) {
    let created_at = chrono::DateTime::from_timestamp_millis(ms).expect("valid timestamp");
    sqlx::query(
        r#"INSERT INTO events
           (id, organization_id, agent_id, event_type, payload, session_id, created_at)
           VALUES ($1, $2, $3, $4, $5, 'cli-session', $6)"#,
    )
    .bind(id)
    .bind(org_id)
    .bind(agent_id)
    .bind(event_type)
    .bind(payload)
    .bind(created_at)
    .execute(pool)
    .await
    .expect("insert event");
}

async fn get_turns(app: axum::Router, agent_id: Uuid, jwt: &str, query: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/agents/{agent_id}/turns{query}"))
                .header("authorization", auth_header(jwt))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request");

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value = serde_json::from_slice(&body).unwrap();
    (status, value)
}

#[sqlx::test(migrations = "../db/migrations")]
async fn get_turns_returns_latest_page_and_older_cursor(pool: sqlx::PgPool) {
    let seed = seed_provider_agent(&pool, "mock", "claude-sonnet-4-6").await;
    let agent_id = seed.agent_id.as_uuid();
    let org_id = seed.org_id.as_uuid();

    for (idx, prompt) in ["one", "two", "three"].into_iter().enumerate() {
        let base = ((idx as i64) + 1) * 10_000;
        let prompt_id = Uuid::from_u128((idx as u128) + 1);
        let stop_id = Uuid::from_u128((idx as u128) + 101);
        insert_event(
            &pool,
            org_id,
            agent_id,
            prompt_id,
            "user_prompt_submit",
            json!({"prompt": prompt, "cliTool": "claude"}),
            base,
        )
        .await;
        insert_event(
            &pool,
            org_id,
            agent_id,
            stop_id,
            "stop",
            json!({"response": format!("{prompt} done")}),
            base + 1_000,
        )
        .await;
    }

    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let (status, body) = get_turns(app.clone(), agent_id, &seed.jwt, "?limit=2").await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["ok"], true);
    assert_eq!(body["totalTurnCount"], 6);
    assert_eq!(body["hasMore"], true);
    assert!(body["cursor"].as_str().is_some_and(|cursor| !cursor.is_empty()));
    assert_eq!(body["lastEvent"]["id"], Uuid::from_u128(103).to_string());

    let turns = body["turns"].as_array().expect("turns array");
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0]["type"], "user");
    assert_eq!(turns[0]["prompt"], "three");
    assert_eq!(turns[1]["type"], "assistant");
    assert_eq!(turns[1]["response"], "three done");

    let cursor = body["cursor"].as_str().unwrap();
    let (older_status, older_body) = get_turns(app, agent_id, &seed.jwt, &format!("?limit=2&cursor={cursor}")).await;
    assert_eq!(older_status, StatusCode::OK, "body: {older_body}");
    let older_turns = older_body["turns"].as_array().expect("older turns array");
    assert_eq!(older_turns.len(), 2);
    assert_eq!(older_turns[0]["prompt"], "two");
    assert_eq!(older_turns[1]["response"], "two done");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn get_turns_enforces_tenant_scope(pool: sqlx::PgPool) {
    let seed = seed_provider_agent(&pool, "mock", "claude-sonnet-4-6").await;
    let other = seed_provider_agent(&pool, "mock", "claude-sonnet-4-6").await;
    insert_event(
        &pool,
        other.org_id.as_uuid(),
        other.agent_id.as_uuid(),
        Uuid::new_v4(),
        "user_prompt_submit",
        json!({"prompt": "private"}),
        10_000,
    )
    .await;

    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, body) = get_turns(app, other.agent_id.as_uuid(), &seed.jwt, "?limit=10").await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["ok"], true);
    assert_eq!(body["totalTurnCount"], 0);
    assert!(body["turns"].as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn get_turns_rejects_invalid_cursor(pool: sqlx::PgPool) {
    let seed = seed_provider_agent(&pool, "mock", "claude-sonnet-4-6").await;
    let app = test_app_with_mock_provider(pool, "mock", "unused").await;
    let (status, body) = get_turns(app, seed.agent_id.as_uuid(), &seed.jwt, "?cursor=not-a-cursor").await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body}");
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
}
