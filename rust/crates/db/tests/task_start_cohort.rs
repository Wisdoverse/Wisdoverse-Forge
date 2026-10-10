//! First-start evidence survives retries and explicit source deletion.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

async fn owner(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let org = Uuid::new_v4();
    let user = Uuid::new_v4();
    let agent = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id,name,slug) VALUES ($1,'Test',$2)")
        .bind(org)
        .bind(org.to_string())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id,email) VALUES ($1,'dev@example.com')").bind(user).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO workspaces (id,organization_id,name) VALUES ($1,$1,'Test')")
        .bind(org)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO agents (id,organization_id,workspace_id,user_id,status) VALUES ($1,$2,$2,$3,'idle')")
        .bind(agent)
        .bind(org)
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
    (org, user, agent)
}

#[sqlx::test(migrations = "./migrations")]
async fn first_start_survives_retry_run_retention_and_source_deletion(pool: PgPool) {
    let (org, user, agent) = owner(&pool).await;
    let task: Uuid = sqlx::query_scalar("INSERT INTO orchestration_tasks (organization_id,title,created_by,status) VALUES ($1,'Test',$2,'queued') RETURNING id")
        .bind(org).bind(user).fetch_one(&pool).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_starts WHERE task_id=$1")
        .bind(task)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "queued work never entered the started cohort");
    sqlx::query("UPDATE orchestration_tasks SET status='working',started_at=NOW() - INTERVAL '2 hours' WHERE id=$1")
        .bind(task)
        .execute(&pool)
        .await
        .unwrap();
    let first: DateTime<Utc> = sqlx::query_scalar("SELECT first_started_at FROM task_starts WHERE task_id=$1")
        .bind(task)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO task_runs (organization_id,workspace_id,orchestration_task_id,agent_id,idempotency_key,status,started_at) VALUES ($1,$1,$2,$3,'attempt-one','failed',$4)")
        .bind(org).bind(task).bind(agent).bind(first).execute(&pool).await.unwrap();
    sqlx::query("UPDATE orchestration_tasks SET status='queued',started_at=NULL,attempt=1 WHERE id=$1")
        .bind(task)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE orchestration_tasks SET status='working',started_at=NOW(),attempt=2 WHERE id=$1")
        .bind(task)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM task_runs WHERE orchestration_task_id=$1").bind(task).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM orchestration_tasks WHERE id=$1").bind(task).execute(&pool).await.unwrap();
    let starts: Vec<DateTime<Utc>> = sqlx::query_scalar("SELECT first_started_at FROM task_starts WHERE task_id=$1")
        .bind(task)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(starts, vec![first], "retries and source deletion preserve one first start");
    sqlx::query("DELETE FROM agents WHERE id=$1").bind(agent).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM workspaces WHERE id=$1").bind(org).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM organizations WHERE id=$1").bind(org).execute(&pool).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_starts WHERE organization_id=$1")
        .bind(org)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "organization deletion removes its start metadata");
}

#[sqlx::test(migrations = "./migrations")]
async fn rolled_back_start_does_not_change_the_cohort(pool: PgPool) {
    let (org, user, _) = owner(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    let task: Uuid = sqlx::query_scalar("INSERT INTO orchestration_tasks (organization_id,title,created_by,status) VALUES ($1,'Test',$2,'working') RETURNING id")
        .bind(org).bind(user).fetch_one(&mut *tx).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_starts WHERE task_id=$1")
        .bind(task)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(count, 1, "start evidence shares the task transaction");
    tx.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_starts WHERE task_id=$1")
        .bind(task)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "rollback removes the uncommitted start");
}

#[sqlx::test(migrations = "./migrations")]
async fn upgrade_uses_surviving_history_without_fabricating_unknown_starts(pool: PgPool) {
    // Reconstruct the installed 105 schema and ledger before a real upgrade.
    sqlx::raw_sql("DROP TRIGGER orchestration_task_first_start ON orchestration_tasks; DROP FUNCTION record_task_first_start(); DROP TABLE task_starts; DROP TABLE task_start_measurement; DELETE FROM _sqlx_migrations WHERE version=106;")
        .execute(&pool).await.unwrap();
    let previous_checksums: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    let (org, user, agent) = owner(&pool).await;
    let task: Uuid = sqlx::query_scalar("INSERT INTO orchestration_tasks (organization_id,title,created_by,status,started_at,result,attempt) VALUES ($1,'Test',$2,'completed',NOW() - INTERVAL '1 hour','{\"stdout\":\"preserve\"}',2) RETURNING id")
        .bind(org).bind(user).fetch_one(&pool).await.unwrap();
    let before: (String, DateTime<Utc>, serde_json::Value) =
        sqlx::query_as("SELECT status,started_at,result FROM orchestration_tasks WHERE id=$1")
            .bind(task)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO task_runs (organization_id,workspace_id,orchestration_task_id,agent_id,idempotency_key,status,started_at) VALUES ($1,$1,$2,$3,'older-attempt','failed',NOW() - INTERVAL '10 hours')")
        .bind(org).bind(task).bind(agent).execute(&pool).await.unwrap();
    let earliest: DateTime<Utc> = sqlx::query_scalar("INSERT INTO orchestration_outbox (id,organization_id,aggregate_type,aggregate_id,event_type,payload,created_at) VALUES ($1,$2,'orchestration_task',$3,'assignment','{}',NOW() - INTERVAL '20 hours') RETURNING created_at")
        .bind(Uuid::new_v4()).bind(org).bind(task).fetch_one(&pool).await.unwrap();
    let unknown: Uuid = sqlx::query_scalar("INSERT INTO orchestration_tasks (organization_id,title,created_by,status,attempt) VALUES ($1,'Unknown',$2,'queued',1) RETURNING id")
        .bind(org).bind(user).fetch_one(&pool).await.unwrap();
    let deleted = Uuid::new_v4();
    sqlx::query("INSERT INTO orchestration_outbox (id,organization_id,aggregate_type,aggregate_id,event_type,payload) VALUES ($1,$2,'orchestration_task',$3,'assignment','{}')")
        .bind(Uuid::new_v4()).bind(org).bind(deleted).execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let before_install: DateTime<Utc> =
        sqlx::query_scalar("SELECT clock_timestamp()").fetch_one(&mut *tx).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/106_task_start_cohorts.sql")).execute(&mut *tx).await.unwrap();
    let coverage: DateTime<Utc> =
        sqlx::query_scalar("SELECT coverage_since FROM task_start_measurement").fetch_one(&mut *tx).await.unwrap();
    assert!(coverage >= before_install, "coverage cannot use the earlier transaction start timestamp");
    tx.rollback().await.unwrap();
    agentforge_db::run_migrations(&pool).await.unwrap();
    let unchanged_checksums: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations WHERE version<106 ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(unchanged_checksums, previous_checksums, "upgrade preserves installed migration checksums");
    let coverage: DateTime<Utc> =
        sqlx::query_scalar("SELECT coverage_since FROM task_start_measurement").fetch_one(&pool).await.unwrap();
    agentforge_db::run_migrations(&pool).await.unwrap();
    let unchanged_coverage: DateTime<Utc> =
        sqlx::query_scalar("SELECT coverage_since FROM task_start_measurement").fetch_one(&pool).await.unwrap();
    assert_eq!(unchanged_coverage, coverage, "startup does not reset the recording boundary");
    let start: DateTime<Utc> = sqlx::query_scalar("SELECT first_started_at FROM task_starts WHERE task_id=$1")
        .bind(task)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(start, earliest, "earliest surviving start wins across attempts");
    let after: (String, DateTime<Utc>, serde_json::Value) =
        sqlx::query_as("SELECT status,started_at,result FROM orchestration_tasks WHERE id=$1")
            .bind(task)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after, before, "backfill preserves source state and result contents");
    let unknown_start: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT first_started_at FROM task_starts WHERE task_id=$1")
            .bind(unknown)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unknown_start, None, "missing historical timestamps stay unknown");
    sqlx::query("UPDATE orchestration_tasks SET status='working',started_at=NOW() WHERE id=$1")
        .bind(unknown)
        .execute(&pool)
        .await
        .unwrap();
    let unknown_start: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT first_started_at FROM task_starts WHERE task_id=$1")
            .bind(unknown)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unknown_start, None, "a retry cannot invent an unknown historical first start");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_starts WHERE task_id=$1")
        .bind(deleted)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "surviving assignment evidence retains a deleted task");
}
