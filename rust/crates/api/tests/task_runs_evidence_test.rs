//! Unit 1.3 regression coverage for task runs and run-scoped evidence.

use agentforge_api::repositories::orchestration::task_run::TaskRunRepository;
use agentforge_api::repositories::orchestration::{OrchestrationTaskRepository, ParticipantRepository};
use agentforge_api::services::evidence_projection::EvidenceProjectionService;
use agentforge_api::services::orchestration::OrchestrationService;
use agentforge_api::test_support::tenant_scope_for_ids;
use agentforge_core::orchestration_protocol::{TaskOutcome, TaskResult};
use agentforge_core::{AgentId, TenantScope};
use agentforge_jobs::{SqlxTaskWriter, TaskWriter};
use sqlx::PgPool;
use uuid::Uuid;

fn signed_image_identity(hex: char, version: &str) -> serde_json::Value {
    let digest = format!("sha256:{}", hex.to_string().repeat(64));
    serde_json::json!({
        "source": format!("ghcr.io/example/agent-codex@{digest}"),
        "imageId": digest.clone(),
        "manifestDigest": digest,
        "version": version,
        "versionSource": "docker-label",
        "trust": "verified-signature"
    })
}

async fn seed_org_user_agent(pool: &PgPool, participant_status: &str) -> (Uuid, Uuid, Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let group_id = Uuid::new_v4();
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
    sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Platform', $3)")
        .bind(team_id)
        .bind(org_id)
        .bind(format!("platform-{team_id}"))
        .execute(pool)
        .await
        .expect("seed team");
    sqlx::query(
        "INSERT INTO projects (id, organization_id, workspace_id, team_id, name, slug)
         VALUES ($1, $2, $2, $3, 'Task runs', $4)",
    )
    .bind(project_id)
    .bind(org_id)
    .bind(team_id)
    .bind(format!("task-runs-{project_id}"))
    .execute(pool)
    .await
    .expect("seed project");
    sqlx::query(
        "INSERT INTO groups (id, organization_id, project_id, name, created_by)
         VALUES ($1, $2, $3, 'Task run evidence', $4)",
    )
    .bind(group_id)
    .bind(org_id)
    .bind(project_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed group");
    let image_identity = serde_json::json!({
        "source": "agentforge-agent:claude",
        "imageId": format!("sha256:{}", "c".repeat(64)),
        "versionSource": "not-reported",
        "trust": "host-local"
    });
    sqlx::query(
        "INSERT INTO agents
            (id, organization_id, workspace_id, user_id, name, status, cli_tool, runtime_kind,
             container_id, container_image_identity, hmac_secret, nats_connect_password)
         VALUES ($1, $2, $2, $3, 'run-agent', 'idle', 'claude', 'container', $4, $5,
                 'test-hmac-secret', 'test-nats-password')",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(user_id)
    .bind(format!("run-container-{agent_id}"))
    .bind(image_identity)
    .execute(pool)
    .await
    .expect("seed agent");
    sqlx::query(
        r#"INSERT INTO participants (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
           VALUES ($1, $2, 'run-agent', ARRAY['codex'], $3, NOW())"#,
    )
    .bind(org_id)
    .bind(agent_id)
    .bind(participant_status)
    .execute(pool)
    .await
    .expect("seed participant");

    (org_id, user_id, agent_id, group_id)
}

fn scope_for(org_id: Uuid, user_id: Uuid) -> TenantScope {
    tenant_scope_for_ids(org_id, user_id)
}

fn orchestration_service(pool: PgPool) -> OrchestrationService {
    OrchestrationService::new(OrchestrationTaskRepository::new(pool.clone()), ParticipantRepository::new(pool))
}

async fn create_and_dispatch(pool: &PgPool, scope: &TenantScope, group_id: Uuid) -> uuid::Uuid {
    let service = orchestration_service(pool.clone());
    let task = service
        .create_task(scope, "run task", Some("collect evidence"), None, None, Some(group_id), None, None, false)
        .await
        .expect("create task");
    let working =
        service.update_task(scope, task.id, Some("queued".into()), None, None, None).await.expect("dispatch task");
    assert_eq!(working.status, "working");
    working.id
}

async fn apply_agent_result(pool: &PgPool, scope: &TenantScope, task_id: Uuid, outcome: TaskOutcome) {
    let task =
        OrchestrationTaskRepository::new(pool.clone()).find_by_id(scope, task_id).await.expect("load dispatched task");
    SqlxTaskWriter::new(pool.clone())
        .apply(
            scope.org_id().as_uuid(),
            TaskResult {
                delivery_id: task.last_assignment_id,
                attempt: Some(task.attempt),
                task_id,
                agent_id: task.assigned_agent_id.expect("dispatched task agent").as_uuid(),
                outcome,
            },
        )
        .await
        .expect("apply Agent result");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn assignment_creates_run_and_agent_result_closes_it(pool: PgPool) {
    let (org_id, user_id, _agent_id, group_id) = seed_org_user_agent(&pool, "available").await;
    let scope = scope_for(org_id, user_id);
    let task_id = create_and_dispatch(&pool, &scope, group_id).await;

    let run_repo = TaskRunRepository::new(pool.clone());
    let runs = run_repo.list_by_task(&scope, task_id).await.expect("list task runs");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "working");
    assert!(runs[0].finished_at.is_none());

    apply_agent_result(&pool, &scope, task_id, TaskOutcome::Completed { stdout: "done".into() }).await;

    let runs = run_repo.list_by_task(&scope, task_id).await.expect("list task runs after complete");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "completed");
    assert!(runs[0].finished_at.is_some());

    let evidence = EvidenceProjectionService::new(TaskRunRepository::new(pool.clone()))
        .for_run(&scope, runs[0].id)
        .await
        .expect("run evidence");
    assert!(
        evidence.iter().any(|row| row.source_type == "task_result"
            && row.payload.get("result").and_then(|v| v.get("stdout")).and_then(|v| v.as_str()) == Some("done")),
        "completed run should project task_result evidence"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn container_agent_without_image_identity_cannot_start_a_run(pool: PgPool) {
    let (org_id, user_id, agent_id, group_id) = seed_org_user_agent(&pool, "available").await;
    let scope = scope_for(org_id, user_id);
    sqlx::query("UPDATE agents SET container_image_identity = NULL WHERE id = $1")
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("clear image identity");
    let service = orchestration_service(pool.clone());
    let task = service
        .create_task(&scope, "unverified run", None, None, None, Some(group_id), None, None, false)
        .await
        .expect("create task");

    let blocked = service
        .update_task(&scope, task.id, Some("queued".into()), None, None, None)
        .await
        .expect("container dispatch without immutable image evidence must remain blocked");
    assert_eq!(blocked.status, "blocked");
    assert_eq!(blocked.blocked_reason.as_deref(), Some("waiting_agent"));

    let (status, run_count, participant_status): (String, i64, String) = sqlx::query_as(
        r#"SELECT task.status,
                  (SELECT count(*) FROM task_runs WHERE orchestration_task_id = task.id),
                  (SELECT status FROM participants WHERE agent_id = $2)
             FROM orchestration_tasks task
            WHERE task.id = $1"#,
    )
    .bind(task.id)
    .bind(agent_id)
    .fetch_one(&pool)
    .await
    .expect("read rolled-back dispatch state");
    assert_eq!(status, "blocked");
    assert_eq!(run_count, 0);
    assert_eq!(participant_status, "available");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn image_identity_cannot_outlive_its_container_reference(pool: PgPool) {
    let (_org_id, _user_id, agent_id, _group_id) = seed_org_user_agent(&pool, "available").await;

    sqlx::query("UPDATE agents SET container_id = NULL WHERE id = $1")
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect_err("detached image identity must violate the database constraint");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn deleting_an_agent_preserves_its_task_run_snapshot(pool: PgPool) {
    let (org_id, user_id, agent_id, group_id) = seed_org_user_agent(&pool, "available").await;
    let scope = scope_for(org_id, user_id);
    let task_id = create_and_dispatch(&pool, &scope, group_id).await;
    apply_agent_result(&pool, &scope, task_id, TaskOutcome::Completed { stdout: "done".into() }).await;

    sqlx::query("DELETE FROM agents WHERE id = $1").bind(agent_id).execute(&pool).await.expect("delete agent");

    let (saved_agent_id, image): (Uuid, serde_json::Value) = sqlx::query_as(
        "SELECT agent_id, capability_profile -> 'image'
           FROM task_runs
          WHERE orchestration_task_id = $1",
    )
    .bind(task_id)
    .fetch_one(&pool)
    .await
    .expect("read retained task run");
    assert_eq!(saved_agent_id, agent_id);
    assert_eq!(image["trust"], "host-local");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn retry_creates_a_second_run(pool: PgPool) {
    let (org_id, user_id, agent_id, group_id) = seed_org_user_agent(&pool, "available").await;
    let image_a = signed_image_identity('a', "1.0.0");
    sqlx::query("UPDATE agents SET container_image_identity = $2 WHERE id = $1")
        .bind(agent_id)
        .bind(&image_a)
        .execute(&pool)
        .await
        .expect("set first image identity");
    let scope = scope_for(org_id, user_id);
    let task_id = create_and_dispatch(&pool, &scope, group_id).await;
    let service = orchestration_service(pool.clone());

    apply_agent_result(
        &pool,
        &scope,
        task_id,
        TaskOutcome::Failed { stderr: "first attempt failed".into(), exit_code: Some(1) },
    )
    .await;
    let image_b = signed_image_identity('b', "2.0.0");
    sqlx::query("UPDATE agents SET container_image_identity = $2 WHERE id = $1")
        .bind(agent_id)
        .bind(&image_b)
        .execute(&pool)
        .await
        .expect("set replacement image identity");
    let reset = service.retry_task(&scope, task_id).await.expect("retry task");
    assert_eq!(reset.status, "backlog");
    let working =
        service.update_task(&scope, task_id, Some("queued".into()), None, None, None).await.expect("dispatch retry");
    assert_eq!(working.status, "working");

    let runs = TaskRunRepository::new(pool.clone()).list_by_task(&scope, task_id).await.expect("list runs");
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].status, "failed");
    assert!(runs[0].finished_at.is_some());
    assert_eq!(runs[1].status, "working");
    assert_ne!(runs[0].idempotency_key, runs[1].idempotency_key);
    assert_eq!(runs[0].capability_profile.get("image"), Some(&image_a));
    assert_eq!(runs[1].capability_profile.get("image"), Some(&image_b));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn run_evidence_projects_events_messages_and_attachments(pool: PgPool) {
    let (org_id, user_id, agent_id, group_id) = seed_org_user_agent(&pool, "available").await;
    let scope = scope_for(org_id, user_id);
    let task_id = create_and_dispatch(&pool, &scope, group_id).await;
    let run = TaskRunRepository::new(pool.clone())
        .list_by_task(&scope, task_id)
        .await
        .expect("list runs")
        .pop()
        .expect("run created");

    sqlx::query(
        r#"INSERT INTO events (organization_id, agent_id, run_id, event_type, payload, session_id)
           VALUES ($1, $2, $3, 'tool_use', '{"tool":"Read"}', 'cli-session')"#,
    )
    .bind(org_id)
    .bind(agent_id)
    .bind(run.id)
    .execute(&pool)
    .await
    .expect("insert event evidence");
    sqlx::query(
        r#"INSERT INTO agent_messages (organization_id, agent_id, run_id, role, content)
           VALUES ($1, $2, $3, 'assistant', 'done')"#,
    )
    .bind(org_id)
    .bind(agent_id)
    .bind(run.id)
    .execute(&pool)
    .await
    .expect("insert message evidence");
    sqlx::query(
        r#"INSERT INTO attachments
              (organization_id, user_id, agent_id, run_id, filename, content_type, size_bytes, storage_path)
           VALUES ($1, $2, $3, $4, 'report.txt', 'text/plain', 4, '/tmp/report.txt')"#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(agent_id)
    .bind(run.id)
    .execute(&pool)
    .await
    .expect("insert attachment evidence");

    let evidence = EvidenceProjectionService::new(TaskRunRepository::new(pool.clone()))
        .for_run(&scope, run.id)
        .await
        .expect("run evidence");
    let source_types: Vec<&str> = evidence.iter().map(|row| row.source_type.as_str()).collect();
    assert!(source_types.contains(&"event"));
    assert!(source_types.contains(&"agent_message"));
    assert!(source_types.contains(&"attachment"));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn legacy_events_remain_visible_without_a_run(pool: PgPool) {
    let (org_id, user_id, agent_id, _group_id) = seed_org_user_agent(&pool, "available").await;
    let scope = scope_for(org_id, user_id);

    sqlx::query(
        r#"INSERT INTO events (organization_id, agent_id, event_type, payload, session_id)
           VALUES ($1, $2, 'legacy_event', '{"legacy":true}', 'old-session')"#,
    )
    .bind(org_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("insert legacy event");

    let evidence = EvidenceProjectionService::new(TaskRunRepository::new(pool.clone()))
        .legacy_for_agent(&scope, AgentId::from(agent_id).as_uuid())
        .await
        .expect("legacy evidence");
    assert_eq!(evidence.len(), 1);
    assert!(evidence[0].run_id.is_none());
    assert_eq!(evidence[0].source_type, "event");
}
