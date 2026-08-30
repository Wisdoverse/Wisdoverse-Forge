//! Contract coverage for orchestration blocked-reason triggers.
//!
//! Issue #22 tracks the gap between UI hint rendering and real business paths
//! that put tasks into each blocked reason. These tests cover the non-agent
//! reasons so future changes cannot regress them into display-only states.

use agentforge_api::repositories::orchestration::{OrchestrationTaskRepository, ParticipantRepository, UpdateTaskRow};
use agentforge_api::services::orchestration::{OrchestrationService, task_summary};
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
        r#"INSERT INTO participants (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
           VALUES ($1, $2, 'blocked-reason-agent', ARRAY['task'], $3, NOW())"#,
    )
    .bind(org_id)
    .bind(agent_id)
    .bind(status)
    .execute(pool)
    .await
    .expect("seed participant");
    agent_id
}

async fn seed_group(pool: &PgPool, org_id: Uuid, user_id: Uuid) -> Uuid {
    let team_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let group_id = Uuid::new_v4();
    sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Platform', $3)")
        .bind(team_id)
        .bind(org_id)
        .bind(format!("platform-{team_id}"))
        .execute(pool)
        .await
        .expect("seed team");
    sqlx::query(
        "INSERT INTO projects (id, organization_id, workspace_id, team_id, name, slug)
         VALUES ($1, $2, $2, $3, 'Blocked reasons', $4)",
    )
    .bind(project_id)
    .bind(org_id)
    .bind(team_id)
    .bind(format!("blocked-reasons-{project_id}"))
    .execute(pool)
    .await
    .expect("seed project");
    sqlx::query(
        "INSERT INTO groups (id, organization_id, project_id, name, created_by)
         VALUES ($1, $2, $3, 'Blocked reason tasks', $4)",
    )
    .bind(group_id)
    .bind(org_id)
    .bind(project_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed group");
    group_id
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
    let summary = task_summary(task, None);
    assert!(summary.blocked_hint.unwrap().contains("ANTHROPIC_API_KEY"));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn assigned_tasks_start_only_after_all_prerequisites_are_ready(pool: PgPool) {
    let (org_id, user_id) = seed_org_user(&pool).await;
    let scope = scope_for(org_id, user_id);
    let group_id = seed_group(&pool, org_id, user_id).await;
    let agent_id = AgentId::from(seed_participant(&pool, org_id, user_id, "available").await);
    let prerequisite_id = seed_working_task(&pool, org_id, user_id, None).await;
    let service = service(pool.clone());

    let missing_input = service
        .create_task(
            &scope,
            "Assigned missing input",
            None,
            Some(json!({ "requiredInputs": ["OPENAI_API_KEY"], "env": {} })),
            None,
            Some(group_id),
            Some(agent_id),
            None,
            false,
        )
        .await
        .expect_err("assigned task with missing input must fail before insert");
    assert!(missing_input.kind.to_string().contains("missing inputs OPENAI_API_KEY"));

    let unfinished_parent = service
        .create_task(
            &scope,
            "Assigned unfinished parent",
            None,
            None,
            None,
            Some(group_id),
            Some(agent_id),
            Some(prerequisite_id),
            false,
        )
        .await
        .expect_err("assigned child with unfinished parent must fail before insert");
    assert!(unfinished_parent.kind.to_string().contains("parent task to finish"));

    let params = json!({ "dependency_ids": [prerequisite_id] });
    let unfinished_dependency = service
        .create_task(
            &scope,
            "Assigned unfinished dependency",
            None,
            Some(params.clone()),
            None,
            Some(group_id),
            Some(agent_id),
            None,
            false,
        )
        .await
        .expect_err("assigned task with unfinished prerequisite must fail before insert");
    assert!(unfinished_dependency.kind.to_string().contains("prerequisite tasks to finish"));

    let queued_dependency = service
        .create_task(
            &scope,
            "Queued unfinished dependency",
            None,
            Some(params.clone()),
            None,
            Some(group_id),
            None,
            None,
            false,
        )
        .await
        .expect("create dependency-blocked task");
    sqlx::query(
        "UPDATE orchestration_tasks SET status = 'queued', blocked_reason = NULL, blocked_metadata = NULL WHERE id = $1",
    )
    .bind(queued_dependency.id)
    .execute(&pool)
    .await
    .expect("simulate stale queued dependency state");
    let reassignment = service
        .update_task(&scope, queued_dependency.id, None, None, None, Some(Some(agent_id)))
        .await
        .expect_err("explicit reassignment must re-check prerequisites");
    assert!(reassignment.kind.to_string().contains("prerequisite tasks to finish"));
    assert_eq!(task_status(&pool, queued_dependency.id).await, "queued");

    let approval_dependency = service
        .create_task(
            &scope,
            "Approval with dependency",
            None,
            Some(params.clone()),
            None,
            Some(group_id),
            None,
            None,
            true,
        )
        .await
        .expect("create approval task with dependency");
    assert_eq!(approval_dependency.blocked_reason.as_deref(), Some("waiting_approval"));
    let approved = service.approve_task(&scope, approval_dependency.id).await.expect("approve task with dependency");
    assert_eq!(approved.status, "blocked");
    assert_eq!(approved.blocked_reason.as_deref(), Some("waiting_dependency"));

    let rejected_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM orchestration_tasks WHERE organization_id = $1 AND title LIKE 'Assigned %'",
    )
    .bind(org_id)
    .fetch_one(&pool)
    .await
    .expect("count rejected tasks");
    assert_eq!(rejected_count, 0);
    assert_eq!(participant_status(&pool, agent_id.as_uuid()).await, "available");

    sqlx::query("UPDATE orchestration_tasks SET status = 'completed' WHERE id = $1 AND organization_id = $2")
        .bind(prerequisite_id)
        .bind(org_id)
        .execute(&pool)
        .await
        .expect("complete prerequisite");

    let started = service
        .create_task(
            &scope,
            "Assigned ready dependency",
            None,
            Some(params),
            None,
            Some(group_id),
            Some(agent_id),
            Some(prerequisite_id),
            false,
        )
        .await
        .expect("completed prerequisites allow assigned task");
    assert_eq!(started.status, "working");
    assert_eq!(started.assigned_agent_id, Some(agent_id));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn approve_task_unblocks_and_dispatches_when_agent_available(pool: PgPool) {
    let (org_id, user_id) = seed_org_user(&pool).await;
    let agent_id = seed_participant(&pool, org_id, user_id, "available").await;
    let group_id = seed_group(&pool, org_id, user_id).await;
    let scope = scope_for(org_id, user_id);

    let task = service(pool.clone())
        .create_task(&scope, "Ship guarded change", None, None, None, Some(group_id), None, None, true)
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

#[sqlx::test(migrations = "../db/migrations")]
async fn retry_recomputes_missing_inputs_and_dependencies(pool: PgPool) {
    let (org_id, user_id) = seed_org_user(&pool).await;
    let scope = scope_for(org_id, user_id);
    let prerequisite_id = seed_working_task(&pool, org_id, user_id, None).await;
    let missing_input_id = Uuid::new_v4();
    let dependency_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO orchestration_tasks (id, organization_id, title, status, created_by, params)
           VALUES ($1, $2, 'failed missing input', 'failed', $3, $4),
                  ($5, $2, 'canceled dependency', 'canceled', $3, $6)"#,
    )
    .bind(missing_input_id)
    .bind(org_id)
    .bind(user_id)
    .bind(json!({ "requiredInputs": ["MODEL_KEY"], "env": {} }))
    .bind(dependency_id)
    .bind(json!({ "dependency_ids": [prerequisite_id] }))
    .execute(&pool)
    .await
    .expect("seed retryable tasks");
    let service = service(pool.clone());

    let missing = service.retry_task(&scope, missing_input_id).await.expect("retry missing-input task");
    assert_eq!(missing.status, "blocked");
    assert_eq!(missing.blocked_reason.as_deref(), Some("waiting_input"));

    let dependent = service.retry_task(&scope, dependency_id).await.expect("retry dependency task");
    assert_eq!(dependent.status, "blocked");
    assert_eq!(dependent.blocked_reason.as_deref(), Some("waiting_dependency"));

    let second_retry = service.retry_task(&scope, dependency_id).await;
    assert!(second_retry.is_err(), "a dependency block is not itself retryable");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn stale_patch_block_and_retry_writes_preserve_newer_lifecycle_state(pool: PgPool) {
    let (org_id, user_id) = seed_org_user(&pool).await;
    let scope = scope_for(org_id, user_id);
    let repo = OrchestrationTaskRepository::new(pool.clone());

    let patch_id = seed_working_task(&pool, org_id, user_id, None).await;
    let stale_patch = repo.find_by_id(&scope, patch_id).await.unwrap();
    sqlx::query(
        "UPDATE orchestration_tasks SET status = 'completed', completed_at = NOW(), updated_at = NOW() + INTERVAL '1 second' WHERE id = $1",
    )
    .bind(patch_id)
    .execute(&pool)
    .await
    .unwrap();
    let patch = repo
        .patch(
            &scope,
            patch_id,
            UpdateTaskRow {
                status: Some("backlog".into()),
                expected_status: Some(stale_patch.status),
                expected_row_version: Some(stale_patch.row_version),
                ..Default::default()
            },
        )
        .await;
    assert!(patch.is_err(), "stale lane patch must conflict");
    assert_eq!(task_status(&pool, patch_id).await, "completed");

    let block_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO orchestration_tasks (id, organization_id, title, status, created_by) VALUES ($1, $2, 'block race', 'queued', $3)",
    )
    .bind(block_id)
    .bind(org_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    let stale_block = repo.find_by_id(&scope, block_id).await.unwrap();
    sqlx::query(
        "UPDATE orchestration_tasks SET status = 'canceled', canceled_at = NOW(), updated_at = NOW() + INTERVAL '1 second' WHERE id = $1",
    )
    .bind(block_id)
    .execute(&pool)
    .await
    .unwrap();
    let blocked = repo.mark_blocked_if_unchanged(&scope, &stale_block, "waiting_agent", json!({})).await.unwrap();
    assert_eq!(blocked.status, "canceled");

    let retry_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO orchestration_tasks (id, organization_id, title, status, created_by, canceled_at) VALUES ($1, $2, 'retry race', 'canceled', $3, NOW())",
    )
    .bind(retry_id)
    .bind(org_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    let stale_retry = repo.find_by_id(&scope, retry_id).await.unwrap();
    sqlx::query(
        "UPDATE orchestration_tasks SET status = 'canceled', canceled_at = NOW() + INTERVAL '1 second', updated_at = NOW() + INTERVAL '1 second' WHERE id = $1",
    )
    .bind(retry_id)
    .execute(&pool)
    .await
    .unwrap();
    let retry = repo.retry(&scope, retry_id, "canceled", stale_retry.row_version, "backlog", None, None).await;
    assert!(retry.is_err(), "stale retry must not match a later canceled generation");
    assert_eq!(task_status(&pool, retry_id).await, "canceled");
}
