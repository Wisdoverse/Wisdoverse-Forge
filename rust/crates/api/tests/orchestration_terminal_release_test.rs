//! Regression coverage for terminal task paths that release participants.
//!
//! `complete_task` already commits task terminal state before releasing the
//! participant. `fail_task` and `cancel_task` must preserve the same ordering
//! so a failed terminal write cannot make the agent available for another
//! claim while the task is still `working`.

use agentforge_api::repositories::orchestration::{OrchestrationTaskRepository, ParticipantRepository};
use agentforge_api::services::orchestration::OrchestrationService;
use agentforge_api::test_support::tenant_scope_for_ids;
use agentforge_core::TenantScope;
use sqlx::PgPool;
use uuid::Uuid;

async fn seed_org_with_busy_participant(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

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
    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status) VALUES ($1, $2, $2, $3, 'terminal-agent', 'idle')",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed agent");
    sqlx::query(
        r#"INSERT INTO participants (organization_id, agent_id, name, status, last_heartbeat_at)
           VALUES ($1, $2, 'terminal-agent', 'busy', NOW())"#,
    )
    .bind(org_id)
    .bind(agent_id)
    .execute(pool)
    .await
    .expect("seed participant");

    (org_id, user_id, agent_id)
}

async fn seed_working_task(pool: &PgPool, org_id: Uuid, user_id: Uuid, agent_id: Uuid) -> Uuid {
    let task_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, created_by, assigned_agent_id, started_at, lease_expires_at)
           VALUES ($1, $2, 'terminal task', 'working', $3, $4, NOW(), NOW() + INTERVAL '60 seconds')"#,
    )
    .bind(task_id)
    .bind(org_id)
    .bind(user_id)
    .bind(agent_id)
    .execute(pool)
    .await
    .expect("seed working task");
    task_id
}

fn service(pool: PgPool) -> OrchestrationService {
    OrchestrationService::new(OrchestrationTaskRepository::new(pool.clone()), ParticipantRepository::new(pool))
}

fn scope_for(org_id: Uuid, user_id: Uuid) -> TenantScope {
    tenant_scope_for_ids(org_id, user_id)
}

async fn participant_status(pool: &PgPool, agent_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM participants WHERE agent_id = $1")
        .bind(agent_id)
        .fetch_one(pool)
        .await
        .expect("query participant status")
}

async fn task_status(pool: &PgPool, task_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM orchestration_tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(pool)
        .await
        .expect("query task status")
}

/// Install a `BEFORE UPDATE` trigger that aborts any attempt to move a task
/// out of `working`. This forces the in-transaction terminal write
/// (`set_result_in_tx` / `cancel_in_tx` / `mark_blocked_retryable_in_tx`) to
/// fail, which rolls the whole transaction back before `tx.commit()`. The
/// participant release runs strictly after that commit, so a failed terminal
/// write must leave the task `working` and the participant `busy`.
async fn install_terminal_write_failure_trigger(pool: &PgPool) {
    sqlx::raw_sql(
        r#"
        CREATE OR REPLACE FUNCTION test_block_terminal_task_write() RETURNS trigger AS $$
        BEGIN
            IF OLD.status = 'working' AND NEW.status IS DISTINCT FROM 'working' THEN
                RAISE EXCEPTION 'injected terminal-write failure for task %', NEW.id;
            END IF;
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;

        CREATE TRIGGER test_block_terminal_task_write_trg
            BEFORE UPDATE ON orchestration_tasks
            FOR EACH ROW EXECUTE FUNCTION test_block_terminal_task_write();
        "#,
    )
    .execute(pool)
    .await
    .expect("install terminal-write failure trigger");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn fail_task_keeps_participant_busy_when_terminal_write_fails(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_with_busy_participant(&pool).await;
    let task_id = seed_working_task(&pool, org_id, user_id, agent_id).await;
    let scope = scope_for(org_id, user_id);
    install_terminal_write_failure_trigger(&pool).await;

    let result = service(pool.clone()).fail_task(&scope, task_id, serde_json::json!({ "message": "boom" })).await;

    assert!(result.is_err(), "fail_task must surface the terminal-write failure, not swallow it");
    assert_eq!(task_status(&pool, task_id).await, "working", "terminal write must roll back");
    assert_eq!(participant_status(&pool, agent_id).await, "busy", "release must not run when the terminal write fails");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn cancel_task_keeps_participant_busy_when_terminal_write_fails(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_with_busy_participant(&pool).await;
    let task_id = seed_working_task(&pool, org_id, user_id, agent_id).await;
    let scope = scope_for(org_id, user_id);
    install_terminal_write_failure_trigger(&pool).await;

    let result = service(pool.clone()).cancel_task(&scope, task_id).await;

    assert!(result.is_err(), "cancel_task must surface the terminal-write failure, not swallow it");
    assert_eq!(task_status(&pool, task_id).await, "working", "terminal write must roll back");
    assert_eq!(participant_status(&pool, agent_id).await, "busy", "release must not run when the terminal write fails");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn complete_task_keeps_participant_busy_when_terminal_write_fails(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_with_busy_participant(&pool).await;
    let task_id = seed_working_task(&pool, org_id, user_id, agent_id).await;
    let scope = scope_for(org_id, user_id);
    install_terminal_write_failure_trigger(&pool).await;

    let result = service(pool.clone()).complete_task(&scope, task_id, serde_json::json!({ "ok": true })).await;

    assert!(result.is_err(), "complete_task must surface the terminal-write failure, not swallow it");
    assert_eq!(task_status(&pool, task_id).await, "working", "terminal write must roll back");
    assert_eq!(participant_status(&pool, agent_id).await, "busy", "release must not run when the terminal write fails");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn quota_block_keeps_participant_busy_when_terminal_write_fails(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_with_busy_participant(&pool).await;
    let task_id = seed_working_task(&pool, org_id, user_id, agent_id).await;
    let scope = scope_for(org_id, user_id);
    install_terminal_write_failure_trigger(&pool).await;

    // `code: quota_exceeded` routes fail_task through the quota-block branch,
    // which marks the task `blocked` in a transaction and releases the
    // participant only after commit.
    let result = service(pool.clone())
        .fail_task(&scope, task_id, serde_json::json!({ "code": "quota_exceeded", "used": 100, "limit": 100 }))
        .await;

    assert!(result.is_err(), "quota-block path must surface the terminal-write failure, not swallow it");
    assert_eq!(task_status(&pool, task_id).await, "working", "blocked write must roll back");
    assert_eq!(participant_status(&pool, agent_id).await, "busy", "release must not run when the terminal write fails");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn fail_task_commits_terminal_state_before_releasing_participant(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_with_busy_participant(&pool).await;
    let task_id = seed_working_task(&pool, org_id, user_id, agent_id).await;
    let scope = scope_for(org_id, user_id);

    let updated = service(pool.clone())
        .fail_task(&scope, task_id, serde_json::json!({ "message": "boom" }))
        .await
        .expect("fail task");

    assert_eq!(updated.status, "failed");
    assert_eq!(task_status(&pool, task_id).await, "failed");
    assert_eq!(participant_status(&pool, agent_id).await, "available");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn cancel_task_commits_terminal_state_before_releasing_participant(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_with_busy_participant(&pool).await;
    let task_id = seed_working_task(&pool, org_id, user_id, agent_id).await;
    let scope = scope_for(org_id, user_id);

    let updated = service(pool.clone()).cancel_task(&scope, task_id).await.expect("cancel task");

    assert_eq!(updated.status, "canceled");
    assert_eq!(task_status(&pool, task_id).await, "canceled");
    assert_eq!(participant_status(&pool, agent_id).await, "available");
}
