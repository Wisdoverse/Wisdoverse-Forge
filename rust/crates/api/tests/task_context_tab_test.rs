//! Unit 4.1 coverage for the task detail Context tab read model.

use agentforge_api::repositories::orchestration::{OrchestrationTaskRepository, ParticipantRepository};
use agentforge_api::repositories::task_context::TaskContextRepository;
use agentforge_api::services::orchestration::OrchestrationService;
use agentforge_api::services::task_context::TaskContextService;
use agentforge_api::test_support::tenant_scope_for_ids_with_axes;
use agentforge_core::TenantScope;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

struct ContextSeed {
    org_id: Uuid,
    user_id: Uuid,
    task_id: Uuid,
    memory_id: Uuid,
    revoked_memory_id: Uuid,
    skill_id: Uuid,
    scope: TenantScope,
}

async fn seed_context_task(pool: &PgPool) -> ContextSeed {
    let org_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let memory_id = Uuid::new_v4();
    let revoked_memory_id = Uuid::new_v4();
    let skill_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, 'Default')")
        .bind(workspace_id)
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
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed org member");
    sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Platform', $3)")
        .bind(team_id)
        .bind(org_id)
        .bind(format!("platform-{team_id}"))
        .execute(pool)
        .await
        .expect("seed team");
    sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(team_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed team member");
    sqlx::query(
        "INSERT INTO projects (id, organization_id, workspace_id, team_id, name, slug)
         VALUES ($1, $2, $3, $4, 'Context', $5)",
    )
    .bind(project_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(team_id)
    .bind(format!("context-{project_id}"))
    .execute(pool)
    .await
    .expect("seed project");
    sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1, $2, 'maintainer')")
        .bind(project_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed project member");
    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, project_id, user_id, name, cli_tool, status)
         VALUES ($1, $2, $3, $4, $5, 'context-agent', 'claude', 'idle')",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(project_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed agent");
    sqlx::query(
        "INSERT INTO orchestration_tasks (id, organization_id, title, description, status, created_by)
         VALUES ($1, $2, 'Show context tab', 'render applied context', 'completed', $3)",
    )
    .bind(task_id)
    .bind(org_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed task");
    sqlx::query(
        "INSERT INTO task_runs (
             id, organization_id, workspace_id, orchestration_task_id, agent_id,
             idempotency_key, status, started_at, finished_at, capability_profile
         )
         VALUES ($1, $2, $3, $4, $5, 'unit-4-1', 'completed', now(), now(), $6)",
    )
    .bind(run_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(task_id)
    .bind(agent_id)
    .bind(json!({ "cliTool": "claude" }))
    .execute(pool)
    .await
    .expect("seed run");

    sqlx::query(
        "INSERT INTO memory_items (
             id, organization_id, workspace_id, owner_user_id, scope_kind, scope_id,
             source_task_id, source_run_id, title, content, visibility, sensitivity,
             confidence, last_used_at, last_verified_at, state
         )
         VALUES ($1, $2, $3, $4, 'project', $5, $6, $7, 'Prod deploy memory',
                 'Run prod-ext and inspect health.', 'shared', 'internal', 0.95, now(), now(), 'active')",
    )
    .bind(memory_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(project_id)
    .bind(task_id)
    .bind(run_id)
    .execute(pool)
    .await
    .expect("seed memory");
    sqlx::query(
        "INSERT INTO memory_items (
             id, organization_id, workspace_id, owner_user_id, scope_kind, scope_id,
             title, content, visibility, sensitivity, state, revoked_at
         )
         VALUES ($1, $2, $3, $4, 'project', $5, 'Old deploy path',
                 'Do not use this path.', 'shared', 'internal', 'revoked', now())",
    )
    .bind(revoked_memory_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(project_id)
    .execute(pool)
    .await
    .expect("seed revoked memory");
    sqlx::query(
        "INSERT INTO skills (
             id, organization_id, workspace_id, scope_kind, scope_id, name, content,
             enabled, state, owner_user_id, sensitivity, provenance
         )
         VALUES ($1, $2, $3, 'org', $2, 'Release checklist', 'Check evidence.',
                 true, 'active', $4, 'internal', '{}'::jsonb)",
    )
    .bind(skill_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed skill");

    for (position, item_id, item_kind, title, content) in [
        (0, memory_id, "memory", "Prod deploy memory", "Run prod-ext and inspect health."),
        (1, skill_id, "skill", "Release checklist", "Check evidence."),
        (2, revoked_memory_id, "memory", "Old deploy path", "Do not use this path."),
    ] {
        sqlx::query(
            "INSERT INTO run_context_injections (
                 organization_id, workspace_id, run_id, item_id, item_kind, position,
                 adapter, envelope_version, capability_profile, applied_snapshot, degradation_reason
             )
             VALUES ($1, $2, $3, $4, $5, $6, 'claude', 'v1', $7, $8, NULL)",
        )
        .bind(org_id)
        .bind(workspace_id)
        .bind(run_id)
        .bind(item_id)
        .bind(item_kind)
        .bind(position)
        .bind(json!({ "cliTool": "claude" }))
        .bind(json!({
            "id": item_id,
            "kind": item_kind,
            "title": title,
            "content": content,
            "content_ref": format!("{item_kind}/{item_id}"),
            "sensitivity": "internal",
            "source": {
                "source_type": item_kind,
                "source_id": item_id,
                "title": title
            }
        }))
        .execute(pool)
        .await
        .expect("seed injection");
    }

    sqlx::query(
        "INSERT INTO context_feedback (
             organization_id, workspace_id, run_id, item_id, item_kind, label, user_id
         )
         VALUES ($1, $2, $3, $4, 'memory', 'useful', $5)",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(run_id)
    .bind(memory_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed feedback");
    sqlx::query(
        "INSERT INTO context_candidates (
             organization_id, workspace_id, source_run_id, item_kind, proposed_content, owner_user_id
         )
         VALUES ($1, $2, $3, 'memory', $4, $5),
                ($1, $2, $3, 'skill', $6, $5)",
    )
    .bind(org_id)
    .bind(workspace_id)
    .bind(run_id)
    .bind(json!({ "title": "New prod memory", "content": "Record the release path." }))
    .bind(user_id)
    .bind(json!({ "name": "Release operator", "content": "Run the release checklist." }))
    .execute(pool)
    .await
    .expect("seed candidates");
    sqlx::query(
        "INSERT INTO events (organization_id, agent_id, run_id, event_type, payload, session_id)
         VALUES ($1, $2, $3, 'task_result', $4, 'session-1')",
    )
    .bind(org_id)
    .bind(agent_id)
    .bind(run_id)
    .bind(json!({ "ok": true }))
    .execute(pool)
    .await
    .expect("seed evidence");

    let scope = tenant_scope_for_ids_with_axes(org_id, user_id, Some(workspace_id), Some(team_id), Some(project_id));
    ContextSeed { org_id, user_id, task_id, memory_id, revoked_memory_id, skill_id, scope }
}

fn service(pool: PgPool) -> TaskContextService {
    TaskContextService::new(OrchestrationTaskRepository::new(pool.clone()), TaskContextRepository::new(pool))
}

#[sqlx::test(migrations = "../db/migrations")]
async fn task_context_includes_applied_items_candidates_evidence_and_provenance(pool: PgPool) {
    let seed = seed_context_task(&pool).await;

    let response = service(pool.clone()).for_task(&seed.scope, seed.task_id).await.expect("task context");

    assert_eq!(response.task_id, seed.task_id);
    assert_eq!(response.runs.len(), 1);
    assert_eq!(response.applied_items.len(), 3);
    assert!(response.applied_items.iter().any(|item| item.item_id == seed.memory_id
        && item.item_kind == "memory"
        && item.title == "Prod deploy memory"
        && item.feedback.as_ref().map(|feedback| feedback.label.as_str()) == Some("useful")));
    assert!(
        response.applied_items.iter().any(|item| item.item_id == seed.skill_id
            && item.item_kind == "skill"
            && item.title == "Release checklist")
    );
    assert!(response.applied_items.iter().any(|item| item.item_id == seed.revoked_memory_id && item.revoked));
    assert_eq!(response.suggested_memory_updates.len(), 1);
    assert_eq!(response.skill_candidates.len(), 1);
    assert!(response.evidence.iter().any(|item| item.source_type == "event"));
    assert_eq!(response.provenance.len(), 3);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn task_context_returns_empty_context_for_task_without_runs(pool: PgPool) {
    let seed = seed_context_task(&pool).await;
    let empty_task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO orchestration_tasks (id, organization_id, title, description, status, created_by)
         VALUES ($1, $2, 'No context', 'empty', 'backlog', $3)",
    )
    .bind(empty_task_id)
    .bind(seed.org_id)
    .bind(seed.user_id)
    .execute(&pool)
    .await
    .expect("seed empty task");

    let response = service(pool.clone()).for_task(&seed.scope, empty_task_id).await.expect("task context");

    assert!(response.runs.is_empty());
    assert!(response.applied_items.is_empty());
    assert!(response.suggested_memory_updates.is_empty());
    assert!(response.skill_candidates.is_empty());
    assert!(response.evidence.is_empty());
    assert!(response.provenance.is_empty());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn task_summary_includes_context_counts_from_run_injections(pool: PgPool) {
    let seed = seed_context_task(&pool).await;
    let task_repo = OrchestrationTaskRepository::new(pool.clone());
    let task = task_repo.find_by_id(&seed.scope, seed.task_id).await.expect("task");
    let service = OrchestrationService::new(task_repo, ParticipantRepository::new(pool.clone()));

    let summary = service.summarize_task(&seed.scope, task).await.expect("summary");

    assert_eq!(summary.context_counts.applied_memories, 2);
    assert_eq!(summary.context_counts.applied_skills, 1);
    assert_eq!(summary.context_counts.total, 3);
}
