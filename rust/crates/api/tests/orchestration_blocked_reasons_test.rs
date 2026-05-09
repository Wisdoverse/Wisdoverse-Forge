//! Contract coverage for orchestration blocked-reason triggers.
//!
//! Issue #22 tracks the gap between UI hint rendering and real business paths
//! that put tasks into each blocked reason. These tests cover the non-agent
//! reasons so future changes cannot regress them into display-only states.

use agentforge_api::repositories::orchestration::{OrchestrationTaskRepository, ParticipantRepository};
use agentforge_api::services::orchestration::OrchestrationService;
use agentforge_api::test_support::tenant_scope_for_ids;
use agentforge_core::{AgentId, TenantScope, UserId};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

async fn seed_org_user(pool: &PgPool) -> (Uuid, Uuid) {
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

async fn seed_participant(pool: &PgPool, org_id: Uuid, user_id: Uuid, status: &str) -> Uuid {
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status) VALUES ($1, $2, $2, $3, 'blocked-reason-agent', 'idle')",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed agent");
    sqlx::query(
        r#"INSERT INTO participants (organization_id, agent_id, name, status, last_heartbeat_at)
           VALUES ($1, $2, 'blocked-reason-agent', $3, NOW())"#,
    )
    .bind(org_id)
    .bind(agent_id)
    .bind(status)
    .execute(pool)
    .await
    .expect("seed participant");
    agent_id
}

async fn seed_working_task(pool: &PgPool, org_id: Uuid, user_id: Uuid, agent_id: Option<Uuid>) -> Uuid {
    let task_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, created_by, assigned_agent_id, started_at, lease_expires_at)
           VALUES ($1, $2, 'working parent', 'working', $3, $4, NOW(), NOW() + INTERVAL '60 seconds')"#,
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
async fn create_task_blocks_on_missing_required_inputs(pool: PgPool) {
    let (org_id, user_id) = seed_org_user(&pool).await;
    let scope = scope_for(org_id, user_id);
    let params = json!({
        "task": "Deploy",
        "message": "Needs provider key",
        "requiredInputs": ["ANTHROPIC_API_KEY", "MODEL"],
        "env": { "MODEL": "claude-sonnet" }
    });

    let task = service(pool.clone())
        .create_task(&scope, "Deploy", None, Some(params), None, None, None, None, false)
        .await
        .expect("create task");

    assert_eq!(task.status, "blocked");
    assert_eq!(task.blocked_reason.as_deref(), Some("waiting_input"));
    assert_eq!(task.blocked_metadata.as_ref().unwrap()["missing"][0], "ANTHROPIC_API_KEY");
    let summary = OrchestrationService::to_summary_with_name(task, None);
    assert!(summary.blocked_hint.unwrap().contains("ANTHROPIC_API_KEY"));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn approve_task_unblocks_and_dispatches_when_agent_available(pool: PgPool) {
    let (org_id, user_id) = seed_org_user(&pool).await;
    let agent_id = seed_participant(&pool, org_id, user_id, "available").await;
    let scope = scope_for(org_id, user_id);

    let task = service(pool.clone())
        .create_task(&scope, "Ship guarded change", None, None, None, None, None, None, true)
        .await
        .expect("create approval task");
    assert_eq!(task.status, "blocked");
    assert_eq!(task.blocked_reason.as_deref(), Some("waiting_approval"));

    let approved = service(pool.clone()).approve_task(&scope, task.id).await.expect("approve task");

    assert_eq!(approved.status, "working");
    assert_eq!(approved.assigned_agent_id, Some(AgentId::from(agent_id)));
    assert!(!approved.requires_approval);
    assert_eq!(approved.approved_by, Some(UserId::from(user_id)));
    assert!(approved.approved_at.is_some());
    assert_eq!(participant_status(&pool, agent_id).await, "busy");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn approval_does_not_bypass_unfinished_parent_dependency(pool: PgPool) {
    let (org_id, user_id) = seed_org_user(&pool).await;
    let scope = scope_for(org_id, user_id);
    let parent_id = seed_working_task(&pool, org_id, user_id, None).await;

    let child = service(pool.clone())
        .create_task(&scope, "Child", None, None, None, None, None, Some(parent_id), true)
        .await
        .expect("create child");
    assert_eq!(child.blocked_reason.as_deref(), Some("waiting_approval"));

    let approved = service(pool.clone()).approve_task(&scope, child.id).await.expect("approve child");

    assert_eq!(approved.status, "blocked");
    assert_eq!(approved.blocked_reason.as_deref(), Some("waiting_dependency"));
    assert!(!approved.requires_approval);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn quota_failure_blocks_task_and_releases_participant(pool: PgPool) {
    let (org_id, user_id) = seed_org_user(&pool).await;
    let agent_id = seed_participant(&pool, org_id, user_id, "busy").await;
    let task_id = seed_working_task(&pool, org_id, user_id, Some(agent_id)).await;
    let scope = scope_for(org_id, user_id);

    let updated = service(pool.clone())
        .fail_task(
            &scope,
            task_id,
            json!({
                "code": "insufficient_quota",
                "provider": "openai",
                "used": 120,
                "limit": 100
            }),
        )
        .await
        .expect("quota block");

    assert_eq!(updated.status, "blocked");
    assert_eq!(updated.blocked_reason.as_deref(), Some("quota_exceeded"));
    assert_eq!(updated.assigned_agent_id, None);
    assert_eq!(updated.failure_code.as_deref(), Some("quota_exceeded"));
    assert!(updated.retryable);
    assert_eq!(updated.blocked_metadata.as_ref().unwrap()["used"], 120);
    assert_eq!(participant_status(&pool, agent_id).await, "available");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn non_agent_blocks_cannot_be_manually_dispatched(pool: PgPool) {
    let (org_id, user_id) = seed_org_user(&pool).await;
    let _agent_id = seed_participant(&pool, org_id, user_id, "available").await;
    let scope = scope_for(org_id, user_id);
    let params = json!({
        "task": "Needs input",
        "requiredInputs": ["ANTHROPIC_API_KEY"],
        "env": {}
    });

    let task = service(pool.clone())
        .create_task(&scope, "Needs input", None, Some(params), None, None, None, None, false)
        .await
        .expect("create input-blocked task");

    let manual_dispatch = service(pool.clone()).dispatch_task(&scope, task.id).await;
    assert!(manual_dispatch.is_err(), "manual dispatch must not bypass waiting_input");

    let drag_to_queued =
        service(pool.clone()).update_task(&scope, task.id, Some("queued".into()), None, None, None).await;
    assert!(drag_to_queued.is_err(), "kanban transition must not bypass waiting_input");
    assert_eq!(task_status(&pool, task.id).await, "blocked");
}
