//! Integration coverage for Workstream C lease-expiry recovery semantics.
//!
//! Expired `working` leases must fail closed with `failure_code = agent_lost`
//! and release the participant back to `available` / `offline` based on
//! heartbeat freshness. In-flight tasks with a valid lease must stay `working`.

use std::collections::HashSet;
use std::time::Duration;

use agentforge_jobs::expire_working_leases;
use sqlx::PgPool;
use uuid::Uuid;

async fn seed_org_and_user(pool: &PgPool) -> (Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("u-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");

    (org_id, user_id)
}

async fn seed_agent(pool: &PgPool, org_id: Uuid, user_id: Uuid, name: &str) -> Uuid {
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status) VALUES ($1, $2, $2, $3, $4, 'idle')",
    )
        .bind(agent_id)
        .bind(org_id)
        .bind(user_id)
        .bind(name)
        .execute(pool)
        .await
        .expect("seed agent");
    agent_id
}

async fn seed_participant(pool: &PgPool, org_id: Uuid, agent_id: Uuid, status: &str, heartbeat_sql: &str) {
    let sql = format!(
        r#"INSERT INTO participants
               (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
           VALUES ($1, $2, 'test-agent', ARRAY['codex'], $3, {heartbeat_sql})"#
    );
    sqlx::query(&sql).bind(org_id).bind(agent_id).bind(status).execute(pool).await.expect("seed participant");
}

async fn seed_task(pool: &PgPool, org_id: Uuid, user_id: Uuid, agent_id: Uuid, title: &str, lease_sql: &str) -> Uuid {
    let task_id = Uuid::now_v7();
    let sql = format!(
        r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, created_by, assigned_agent_id, priority,
                attempt, lease_expires_at, last_assignment_id, started_at)
           VALUES ($1, $2, $3, 'working', $4, $5, 'normal', 1, {lease_sql}, $6, NOW() - INTERVAL '10 minutes')"#
    );
    sqlx::query(&sql)
        .bind(task_id)
        .bind(org_id)
        .bind(title)
        .bind(user_id)
        .bind(agent_id)
        .bind(Uuid::now_v7())
        .execute(pool)
        .await
        .expect("seed working task");
    task_id
}

async fn seed_task_run(pool: &PgPool, org_id: Uuid, task_id: Uuid, agent_id: Uuid) -> Uuid {
    let run_id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO task_runs
               (id, organization_id, workspace_id, orchestration_task_id, agent_id,
                idempotency_key, status, started_at, capability_profile)
           VALUES ($1, $2, $2, $3, $4, $5, 'working', NOW() - INTERVAL '10 minutes', '{}')"#,
    )
    .bind(run_id)
    .bind(org_id)
    .bind(task_id)
    .bind(agent_id)
    .bind(Uuid::now_v7().to_string())
    .execute(pool)
    .await
    .expect("seed task run");
    run_id
}

#[sqlx::test(migrations = "../db/migrations")]
async fn expired_working_leases_fail_closed_and_release_participants(pool: PgPool) {
    let (org_id, user_id) = seed_org_and_user(&pool).await;

    let fresh_agent = seed_agent(&pool, org_id, user_id, "fresh-agent").await;
    let stale_agent = seed_agent(&pool, org_id, user_id, "stale-agent").await;
    let healthy_agent = seed_agent(&pool, org_id, user_id, "healthy-agent").await;
    let legacy_orphan_agent = seed_agent(&pool, org_id, user_id, "legacy-orphan-agent").await;
    let legacy_busy_agent = seed_agent(&pool, org_id, user_id, "legacy-busy-agent").await;

    seed_participant(&pool, org_id, fresh_agent, "busy", "NOW()").await;
    seed_participant(&pool, org_id, stale_agent, "busy", "NOW() - INTERVAL '10 minutes'").await;
    seed_participant(&pool, org_id, healthy_agent, "busy", "NOW()").await;
    seed_participant(&pool, org_id, legacy_orphan_agent, "available", "NOW()").await;
    seed_participant(&pool, org_id, legacy_busy_agent, "busy", "NOW()").await;

    let expired_fresh =
        seed_task(&pool, org_id, user_id, fresh_agent, "expired-fresh", "NOW() - INTERVAL '5 minutes'").await;
    let expired_stale =
        seed_task(&pool, org_id, user_id, stale_agent, "expired-stale", "NOW() - INTERVAL '5 minutes'").await;
    let still_working =
        seed_task(&pool, org_id, user_id, healthy_agent, "still-working", "NOW() + INTERVAL '5 minutes'").await;
    let legacy_orphan = seed_task(&pool, org_id, user_id, legacy_orphan_agent, "legacy-orphan", "NULL").await;
    let legacy_busy = seed_task(&pool, org_id, user_id, legacy_busy_agent, "legacy-busy", "NULL").await;
    let expired_run = seed_task_run(&pool, org_id, expired_fresh, fresh_agent).await;
    let healthy_run = seed_task_run(&pool, org_id, still_working, healthy_agent).await;

    let outcomes = expire_working_leases(&pool, Duration::from_secs(90)).await.expect("expire working leases");
    assert_eq!(outcomes.len(), 3, "expired and orphaned legacy tasks should be reconciled");

    let expired_ids: HashSet<Uuid> = outcomes.iter().map(|outcome| outcome.task.id).collect();
    assert!(expired_ids.contains(&expired_fresh));
    assert!(expired_ids.contains(&expired_stale));
    assert!(expired_ids.contains(&legacy_orphan));
    assert!(!expired_ids.contains(&still_working));
    assert!(!expired_ids.contains(&legacy_busy));

    for outcome in &outcomes {
        assert_eq!(outcome.task.status, "failed");
        assert_eq!(outcome.task.failure_code.as_deref(), Some("agent_lost"));
        assert!(!outcome.task.retryable);
        assert!(outcome.task.lease_expires_at.is_none());
        assert!(outcome.task.completed_at.is_some());
        let error = outcome.task.error.as_ref().expect("expired task must carry error payload");
        assert_eq!(error.get("code").and_then(|v| v.as_str()), Some("agent_lost"));
    }

    let fresh_status: String =
        sqlx::query_scalar("SELECT status FROM participants WHERE organization_id = $1 AND agent_id = $2")
            .bind(org_id)
            .bind(fresh_agent)
            .fetch_one(&pool)
            .await
            .expect("fresh participant status");
    assert_eq!(fresh_status, "available");

    let stale_status: String =
        sqlx::query_scalar("SELECT status FROM participants WHERE organization_id = $1 AND agent_id = $2")
            .bind(org_id)
            .bind(stale_agent)
            .fetch_one(&pool)
            .await
            .expect("stale participant status");
    assert_eq!(stale_status, "offline");

    let healthy_status: String =
        sqlx::query_scalar("SELECT status FROM participants WHERE organization_id = $1 AND agent_id = $2")
            .bind(org_id)
            .bind(healthy_agent)
            .fetch_one(&pool)
            .await
            .expect("healthy participant status");
    assert_eq!(healthy_status, "busy");

    let legacy_orphan_status: String =
        sqlx::query_scalar("SELECT status FROM participants WHERE organization_id = $1 AND agent_id = $2")
            .bind(org_id)
            .bind(legacy_orphan_agent)
            .fetch_one(&pool)
            .await
            .expect("legacy orphan participant status");
    assert_eq!(legacy_orphan_status, "available");

    let working_ids: HashSet<Uuid> = sqlx::query_scalar("SELECT id FROM orchestration_tasks WHERE status = 'working'")
        .fetch_all(&pool)
        .await
        .expect("working task ids")
        .into_iter()
        .collect();
    assert!(working_ids.contains(&still_working));
    assert!(working_ids.contains(&legacy_busy));

    let (expired_run_status, expired_finished_at): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, finished_at FROM task_runs WHERE id = $1")
            .bind(expired_run)
            .fetch_one(&pool)
            .await
            .expect("expired run status");
    assert_eq!(expired_run_status, "failed");
    assert!(expired_finished_at.is_some());

    let (healthy_run_status, healthy_finished_at): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, finished_at FROM task_runs WHERE id = $1")
            .bind(healthy_run)
            .fetch_one(&pool)
            .await
            .expect("healthy run status");
    assert_eq!(healthy_run_status, "working");
    assert!(healthy_finished_at.is_none());
}
