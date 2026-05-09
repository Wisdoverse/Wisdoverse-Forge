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
