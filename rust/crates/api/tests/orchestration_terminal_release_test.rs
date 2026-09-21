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

async fn seed_durable_execution(
    pool: &PgPool,
    org_id: Uuid,
    user_id: Uuid,
    agent_id: Uuid,
    published: bool,
) -> (Uuid, Uuid) {
    let task_id = seed_working_task(pool, org_id, user_id, agent_id).await;
    let delivery_id = Uuid::now_v7();
    sqlx::query("UPDATE orchestration_tasks SET last_assignment_id = $2, attempt = 1 WHERE id = $1")
        .bind(task_id)
        .bind(delivery_id)
        .execute(pool)
        .await
        .expect("attach delivery");
    sqlx::query(
        r#"INSERT INTO orchestration_outbox
               (id, organization_id, aggregate_type, aggregate_id, event_type, payload, published_at)
           VALUES ($1, $2, 'task', $3, 'assignment', '{}',
                   CASE WHEN $4 THEN NOW() ELSE NULL END)"#,
    )
    .bind(delivery_id)
    .bind(org_id)
    .bind(task_id)
    .bind(published)
    .execute(pool)
    .await
    .expect("seed assignment outbox");
    sqlx::query(
        r#"INSERT INTO task_runs
               (organization_id, workspace_id, orchestration_task_id, agent_id,
                idempotency_key, status, started_at, capability_profile)
           VALUES ($1, $1, $2, $3, $4, 'working', NOW(), '{}')"#,
    )
    .bind(org_id)
    .bind(task_id)
    .bind(agent_id)
    .bind(delivery_id.to_string())
    .execute(pool)
    .await
    .expect("seed active run");
    (task_id, delivery_id)
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
async fn cancel_task_rejects_active_execution_without_releasing_participant(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_with_busy_participant(&pool).await;
    let task_id = seed_working_task(&pool, org_id, user_id, agent_id).await;
    let scope = scope_for(org_id, user_id);

    let result = service(pool.clone()).cancel_task(&scope, task_id).await;

    assert!(result.is_err(), "active execution has no safe remote cancellation protocol");
    assert_eq!(task_status(&pool, task_id).await, "working");
    assert_eq!(participant_status(&pool, agent_id).await, "busy");
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
async fn cancel_task_cancels_queued_work_without_touching_an_unrelated_participant(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_with_busy_participant(&pool).await;
    let task_id = seed_working_task(&pool, org_id, user_id, agent_id).await;
    sqlx::query("UPDATE orchestration_tasks SET status = 'queued', assigned_agent_id = NULL WHERE id = $1")
        .bind(task_id)
        .execute(&pool)
        .await
        .unwrap();
    let scope = scope_for(org_id, user_id);

    let updated = service(pool.clone()).cancel_task(&scope, task_id).await.expect("cancel task");

    assert_eq!(updated.status, "canceled");
    assert_eq!(task_status(&pool, task_id).await, "canceled");
    assert_eq!(participant_status(&pool, agent_id).await, "busy");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn http_terminal_actions_cannot_overtake_durable_agent_execution(pool: PgPool) {
    for published in [false, true] {
        let (org_id, user_id, agent_id) = seed_org_with_busy_participant(&pool).await;
        let (task_id, delivery_id) = seed_durable_execution(&pool, org_id, user_id, agent_id, published).await;
        let scope = scope_for(org_id, user_id);
        let svc = service(pool.clone());

        assert!(svc.complete_task(&scope, task_id, serde_json::json!({ "manual": true })).await.is_err());
        assert!(svc.fail_task(&scope, task_id, serde_json::json!({ "message": "manual" })).await.is_err());
        assert!(svc.cancel_task(&scope, task_id).await.is_err());

        let task: (String, Option<Uuid>, i64) =
            sqlx::query_as("SELECT status, last_assignment_id, row_version FROM orchestration_tasks WHERE id = $1")
                .bind(task_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(task.0, "working");
        assert_eq!(task.1, Some(delivery_id));
        assert_eq!(participant_status(&pool, agent_id).await, "busy");
        let run_status: String = sqlx::query_scalar("SELECT status FROM task_runs WHERE orchestration_task_id = $1")
            .bind(task_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(run_status, "working");
        let outbox_published: bool =
            sqlx::query_scalar("SELECT published_at IS NOT NULL FROM orchestration_outbox WHERE id = $1")
                .bind(delivery_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(outbox_published, published);
    }
}

#[sqlx::test(migrations = "../db/migrations")]
async fn release_if_idle_preserves_a_newer_working_owner(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_with_busy_participant(&pool).await;
    let old_task = seed_working_task(&pool, org_id, user_id, agent_id).await;
    let current_task = seed_working_task(&pool, org_id, user_id, agent_id).await;
    sqlx::query("UPDATE orchestration_tasks SET status = 'completed' WHERE id = $1")
        .bind(old_task)
        .execute(&pool)
        .await
        .unwrap();
    let scope = scope_for(org_id, user_id);
    let repo = ParticipantRepository::new(pool.clone());

    assert!(repo.release_if_idle(&scope, agent_id.into()).await.unwrap().is_none());
    assert_eq!(participant_status(&pool, agent_id).await, "busy");

    sqlx::query("UPDATE orchestration_tasks SET status = 'completed' WHERE id = $1")
        .bind(current_task)
        .execute(&pool)
        .await
        .unwrap();
    assert!(repo.release_if_idle(&scope, agent_id.into()).await.unwrap().is_some());
    assert_eq!(participant_status(&pool, agent_id).await, "available");

    sqlx::query("UPDATE participants SET status = 'offline' WHERE agent_id = $1")
        .bind(agent_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(repo.release_if_idle(&scope, agent_id.into()).await.unwrap().is_some());
    assert_eq!(participant_status(&pool, agent_id).await, "offline");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn register_and_heartbeat_derive_status_from_working_ownership(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_with_busy_participant(&pool).await;
    let task_id = seed_working_task(&pool, org_id, user_id, agent_id).await;
    let scope = scope_for(org_id, user_id);
    let repo = ParticipantRepository::new(pool.clone());

    sqlx::query("DELETE FROM participants WHERE agent_id = $1").bind(agent_id).execute(&pool).await.unwrap();
    assert_eq!(repo.register(&scope, agent_id.into(), "restarted", &["chat".into()]).await.unwrap().status, "busy");
    sqlx::query("UPDATE participants SET status = 'offline' WHERE agent_id = $1")
        .bind(agent_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(repo.heartbeat(&scope, agent_id.into()).await.unwrap().status, "busy");

    sqlx::query("UPDATE orchestration_tasks SET status = 'completed' WHERE id = $1")
        .bind(task_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        repo.register(&scope, agent_id.into(), "restarted", &["chat".into()]).await.unwrap().status,
        "available"
    );
}
