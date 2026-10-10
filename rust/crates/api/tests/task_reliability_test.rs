//! Authenticated cohort measurement with real PostgreSQL and synthetic history.

use agentforge_api::repositories::retention::RetentionRepository;
use agentforge_api::test_support::{mint_test_jwt, test_app_with_mock_provider};
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn organization(pool: &PgPool, user: Uuid) -> (Uuid, String) {
    let org = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id,name,slug) VALUES ($1,'Test',$2)")
        .bind(org)
        .bind(org.to_string())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces (id,organization_id,name) VALUES ($1,$1,'Test')")
        .bind(org)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO organization_members (organization_id,user_id,role) VALUES ($1,$2,'owner')")
        .bind(org)
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
    (org, mint_test_jwt(org, user, "owner"))
}

async fn user(pool: &PgPool) -> Uuid {
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id,email) VALUES ($1,'dev@example.com')").bind(user).execute(pool).await.unwrap();
    user
}

async fn task(
    pool: &PgPool,
    org: Uuid,
    user: Uuid,
    status: &str,
    result: Option<Value>,
    error: Option<Value>,
    hours: i32,
) -> Uuid {
    sqlx::query_scalar("INSERT INTO orchestration_tasks (organization_id,title,created_by,status,result,error,started_at,updated_at) VALUES ($1,'PRIVATE_TASK_TITLE',$2,$3,$4,$5,NOW() - ($6::int * INTERVAL '1 hour'),NOW() - ($6::int * INTERVAL '1 hour')) RETURNING id")
        .bind(org).bind(user).bind(status).bind(result).bind(error).bind(hours).fetch_one(pool).await.unwrap()
}

async fn report(app: &Router, token: Option<&str>, query: &str) -> (StatusCode, Value) {
    let mut request = Request::builder().uri(format!("/api/v1/analytics/task-reliability{query}"));
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = app.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 2_000_000).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn report_includes_unfinished_deleted_and_missing_results_without_leaking_tenants(pool: PgPool) {
    let user = user(&pool).await;
    let (org, token) = organization(&pool, user).await;
    let (other, other_token) = organization(&pool, user).await;
    // Only fixtures move the coverage marker to simulate an observed window.
    sqlx::query("UPDATE task_start_measurement SET coverage_since=NOW() - INTERVAL '90 days'")
        .execute(&pool)
        .await
        .unwrap();
    for (status, result, error) in [
        ("completed", Some(json!({"stdout":"PRIVATE_RESULT_CONTENT"})), None),
        ("completed", None, Some(json!({"oldError":"must not qualify"}))),
        ("completed", Some(Value::Null), None),
        ("failed", None, Some(json!({"code":"exit_1"}))),
        ("failed", Some(json!({"oldResult":"must not qualify"})), Some(Value::Null)),
        ("canceled", None, None),
        ("working", None, None),
        ("blocked", None, None),
    ] {
        task(&pool, org, user, status, result, error, 2).await;
    }
    let canceled = task(&pool, org, user, "working", None, None, 2).await;
    sqlx::query("UPDATE orchestration_tasks SET status='canceled',canceled_at=NOW() WHERE id=$1")
        .bind(canceled)
        .execute(&pool)
        .await
        .unwrap();
    let retry = task(&pool, org, user, "working", None, None, 2).await;
    sqlx::query("UPDATE orchestration_tasks SET status='queued',started_at=NULL,attempt=2 WHERE id=$1")
        .bind(retry)
        .execute(&pool)
        .await
        .unwrap();
    let deleted = task(&pool, org, user, "completed", Some(json!({"stdout":"deleted"})), None, 2).await;
    sqlx::query("DELETE FROM orchestration_tasks WHERE id=$1").bind(deleted).execute(&pool).await.unwrap();
    task(&pool, org, user, "working", None, None, 1_000).await;
    task(&pool, org, user, "working", None, None, -1).await;
    task(&pool, other, user, "completed", Some(json!({"stdout":"OTHER_PRIVATE_CONTENT"})), None, 2).await;
    sqlx::query("INSERT INTO orchestration_tasks (organization_id,title,created_by,status) VALUES ($1,'Never started',$2,'queued')")
        .bind(org).bind(user).execute(&pool).await.unwrap();

    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    assert_eq!(report(&app, None, "").await.0, StatusCode::UNAUTHORIZED);
    let (status, body) = report(&app, Some(&token), "?hours=720").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ok"], true);
    let data = &body["data"];
    assert_eq!(data["startedTasks"], 11);
    assert_eq!(data["terminalWithPersistedResults"], 3);
    assert_eq!(data["terminalWithoutPersistedResults"], 4);
    assert_eq!(data["unfinishedTasks"], 3);
    assert_eq!(data["deletedTasks"], 1);
    assert_eq!(data["coverageComplete"], true);
    assert_eq!(data["terminalPersistenceRate"], 3.0 / 11.0);
    let serialized = body.to_string();
    for private in ["PRIVATE_TASK_TITLE", "PRIVATE_RESULT_CONTENT", "OTHER_PRIVATE_CONTENT", "dev@example.com"] {
        assert!(!serialized.contains(private), "report must contain counts only");
    }
    assert!(!serialized.contains(&other.to_string()));
    let (_, other_body) = report(&app, Some(&other_token), "").await;
    assert_eq!(other_body["data"]["startedTasks"], 1);
    assert_eq!(other_body["data"]["terminalWithPersistedResults"], 1);
    let (_, short) = report(&app, Some(&token), "?hours=-100").await;
    assert_eq!(short["data"]["windowHours"], 1);
    assert_eq!(short["data"]["coverageComplete"], true);
    assert_eq!(short["data"]["startedTasks"], 0);
    assert!(short["data"]["terminalPersistenceRate"].is_null());
    let (_, wide) = report(&app, Some(&token), "?hours=99999").await;
    assert_eq!(wide["data"]["windowHours"], 8_760);
    assert_eq!(wide["data"]["startedTasks"], 12);
    assert_eq!(wide["data"]["coverageComplete"], false);
    assert!(wide["data"]["terminalPersistenceRate"].is_null());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn empty_and_incompletely_covered_windows_have_no_rate(pool: PgPool) {
    let user = user(&pool).await;
    let (org, token) = organization(&pool, user).await;
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    // This NULL represents an unplaced start from the historical backfill.
    sqlx::query("INSERT INTO task_starts (organization_id,task_id,first_started_at) VALUES ($1,$2,NULL)")
        .bind(org)
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .unwrap();
    let (_, empty) = report(&app, Some(&token), "").await;
    assert_eq!(empty["data"]["coverageComplete"], false);
    assert_eq!(empty["data"]["startedTasks"], 0);
    assert_eq!(empty["data"]["unplacedHistoricalTasks"], 1);
    assert!(empty["data"]["terminalPersistenceRate"].is_null());
    task(&pool, org, user, "completed", Some(json!({"noArtifacts":true})), None, 0).await;
    let (_, incomplete) = report(&app, Some(&token), "?hours=1").await;
    assert_eq!(incomplete["data"]["terminalWithPersistedResults"], 1);
    assert_eq!(incomplete["data"]["coverageComplete"], false);
    assert!(incomplete["data"]["terminalPersistenceRate"].is_null());
    sqlx::query("DELETE FROM orchestration_tasks WHERE organization_id=$1").bind(org).execute(&pool).await.unwrap();
    sqlx::query("UPDATE task_start_measurement SET coverage_since=NOW() - INTERVAL '90 days'")
        .execute(&pool)
        .await
        .unwrap();
    let (_, deleted) = report(&app, Some(&token), "").await;
    assert_eq!(deleted["data"]["startedTasks"], 1);
    assert_eq!(deleted["data"]["deletedTasks"], 1);
    assert_eq!(deleted["data"]["terminalPersistenceRate"], 0.0);
    let (_, narrow) = report(&app, Some(&token), "?hours=1").await;
    assert_eq!(narrow["data"]["coverageComplete"], true);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn run_retention_and_agent_deletion_preserve_result_eligibility(pool: PgPool) {
    let user = user(&pool).await;
    let (org, token) = organization(&pool, user).await;
    sqlx::query("UPDATE task_start_measurement SET coverage_since=NOW() - INTERVAL '90 days'")
        .execute(&pool)
        .await
        .unwrap();
    let agent = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id,organization_id,workspace_id,user_id,status) VALUES ($1,$2,$2,$3,'idle')")
        .bind(agent)
        .bind(org)
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let completed: Uuid = sqlx::query_scalar("INSERT INTO orchestration_tasks (organization_id,title,created_by,status,assigned_agent_id,result,started_at,updated_at) VALUES ($1,'Test',$2,'completed',$3,'{\"stdout\":\"retained\"}',NOW() - INTERVAL '2 days',NOW() - INTERVAL '2 days') RETURNING id")
        .bind(org).bind(user).bind(agent).fetch_one(&pool).await.unwrap();
    sqlx::query("INSERT INTO task_runs (organization_id,workspace_id,orchestration_task_id,agent_id,idempotency_key,status) VALUES ($1,$1,$2,$3,'completed-run','completed')")
        .bind(org).bind(completed).bind(agent).execute(&pool).await.unwrap();
    let removed = RetentionRepository::new(pool.clone()).purge_finished_runs(1).await.unwrap();
    assert_eq!(removed, 1);
    sqlx::query("DELETE FROM agents WHERE id=$1").bind(agent).execute(&pool).await.unwrap();
    let app = test_app_with_mock_provider(pool.clone(), "mock", "unused").await;
    let (status, body) = report(&app, Some(&token), "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["startedTasks"], 1);
    assert_eq!(body["data"]["terminalWithPersistedResults"], 1);
    assert_eq!(body["data"]["terminalPersistenceRate"], 1.0);
}
