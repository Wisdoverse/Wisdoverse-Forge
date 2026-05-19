//! Unit 4.2 coverage for preview-first context publish determinism.

use std::sync::Arc;

use agentforge_api::repositories::context_preview::ContextPreviewRepository;
use agentforge_api::repositories::orchestration::run_context_injection::RunContextInjectionRepository;
use agentforge_api::repositories::orchestration::task_run::TaskRunRepository;
use agentforge_api::repositories::orchestration::{OrchestrationTaskRepository, ParticipantRepository};
use agentforge_api::repositories::runtime_capability::RuntimeCapabilityRepository;
use agentforge_api::services::context_preview::{
    ContextPreviewService, CreateContextPreviewInput, PublishWithContextInput,
};
use agentforge_api::services::context_resolver::ContextResolverService;
use agentforge_api::services::orchestration::OrchestrationService;
use agentforge_api::services::runtime_capability_registry::RuntimeCapabilityRegistryService;
use agentforge_api::test_support::tenant_scope_for_ids_with_axes;
use agentforge_core::{AgentId, ErrorKind, TenantScope};
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

struct PreviewSeed {
    org_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
    project_id: Uuid,
    agent_id: Uuid,
    task_id: Uuid,
    scope: TenantScope,
}

async fn resolver(pool: PgPool) -> Arc<ContextResolverService> {
    let registry = RuntimeCapabilityRegistryService::new(RuntimeCapabilityRepository::new(pool.clone()));
    registry.refresh_from_code().await.expect("refresh runtime capability registry");
    Arc::new(ContextResolverService::new(pool, registry))
}

fn orchestration(pool: PgPool, context_resolver: Arc<ContextResolverService>) -> OrchestrationService {
    OrchestrationService::new(OrchestrationTaskRepository::new(pool.clone()), ParticipantRepository::new(pool.clone()))
        .with_context_resolver(context_resolver)
}

fn preview_service(pool: PgPool, context_resolver: Arc<ContextResolverService>) -> ContextPreviewService {
    ContextPreviewService::new(
        ContextPreviewRepository::new(pool.clone()),
        OrchestrationTaskRepository::new(pool.clone()),
        ParticipantRepository::new(pool.clone()),
        context_resolver,
    )
}

async fn seed_base(pool: &PgPool, task_text: &str) -> PreviewSeed {
    let org_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();

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
         VALUES ($1, $2, $3, $4, $5, 'preview-agent', 'claude', 'idle')",
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
        "INSERT INTO participants (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
         VALUES ($1, $2, 'preview-agent', ARRAY['claude'], 'available', now())",
    )
    .bind(org_id)
    .bind(agent_id)
    .execute(pool)
    .await
    .expect("seed participant");
    sqlx::query(
        "INSERT INTO orchestration_tasks (id, organization_id, title, description, status, created_by)
         VALUES ($1, $2, 'Ship governed context', $3, 'queued', $4)",
    )
    .bind(task_id)
    .bind(org_id)
    .bind(task_text)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed task");

    let scope = tenant_scope_for_ids_with_axes(org_id, user_id, Some(workspace_id), Some(team_id), Some(project_id));

    PreviewSeed { org_id, workspace_id, user_id, project_id, agent_id, task_id, scope }
}

async fn insert_memory(pool: &PgPool, seed: &PreviewSeed, title: &str, content: &str, offset: Duration) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO memory_items (
               organization_id, workspace_id, owner_user_id, scope_kind, scope_id,
               title, content, visibility, sensitivity, confidence, last_verified_at, state
           )
           VALUES ($1, $2, $3, 'project', $4, $5, $6, 'shared', 'internal', 1.0, $7, 'active')
           RETURNING id"#,
    )
    .bind(seed.org_id)
    .bind(seed.workspace_id)
    .bind(seed.user_id)
    .bind(seed.project_id)
    .bind(title)
    .bind(content)
    .bind(Utc::now() + offset)
    .fetch_one(pool)
    .await
    .expect("insert memory")
}

#[sqlx::test(migrations = "../db/migrations")]
async fn publish_with_context_records_the_preview_selected_items(pool: PgPool) {
    let seed = seed_base(&pool, "Use prod-ext validation").await;
    let keep_id =
        insert_memory(&pool, &seed, "prod-ext keep", "Keep this prod-ext memory in the run.", Duration::minutes(2))
            .await;
    let remove_id = insert_memory(
        &pool,
        &seed,
        "prod-ext remove",
        "Remove this prod-ext memory from the run.",
        Duration::minutes(3),
    )
    .await;
    let context_resolver = resolver(pool.clone()).await;
    let preview = preview_service(pool.clone(), context_resolver.clone())
        .create(
            &seed.scope,
            CreateContextPreviewInput { task_id: seed.task_id, agent_id: AgentId::from(seed.agent_id) },
        )
        .await
        .expect("create preview");
    assert_eq!(preview.items.len(), 2);

    let task = preview_service(pool.clone(), context_resolver.clone())
        .publish_existing_task(
            &orchestration(pool.clone(), context_resolver),
            &seed.scope,
            seed.task_id,
            PublishWithContextInput {
                context_preview_id: preview.context_preview_id,
                preview_hash: preview.preview_hash,
                pinned_item_ids: Vec::new(),
                removed_item_ids: vec![remove_id],
            },
        )
        .await
        .expect("publish with preview");
    assert_eq!(task.status, "working");

    let runs = TaskRunRepository::new(pool.clone()).list_by_task(&seed.scope, seed.task_id).await.expect("runs");
    assert_eq!(runs.len(), 1);
    let injections = RunContextInjectionRepository::new(pool.clone())
        .list_by_run(&seed.scope, runs[0].id)
        .await
        .expect("injections");
    assert_eq!(injections.len(), 1);
    assert_eq!(injections[0].item_id, keep_id);
    assert_ne!(injections[0].item_id, remove_id);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn publish_rejects_a_stale_preview_without_starting_a_run(pool: PgPool) {
    let seed = seed_base(&pool, "Use prod-ext validation").await;
    insert_memory(&pool, &seed, "prod-ext memory", "Use this prod-ext memory.", Duration::minutes(1)).await;
    let context_resolver = resolver(pool.clone()).await;
    let preview = preview_service(pool.clone(), context_resolver.clone())
        .create(
            &seed.scope,
            CreateContextPreviewInput { task_id: seed.task_id, agent_id: AgentId::from(seed.agent_id) },
        )
        .await
        .expect("create preview");

    sqlx::query("UPDATE orchestration_tasks SET title = 'Changed governed context task' WHERE id = $1")
        .bind(seed.task_id)
        .execute(&pool)
        .await
        .expect("mutate task draft");

    let err = preview_service(pool.clone(), context_resolver.clone())
        .publish_existing_task(
            &orchestration(pool.clone(), context_resolver),
            &seed.scope,
            seed.task_id,
            PublishWithContextInput {
                context_preview_id: preview.context_preview_id,
                preview_hash: preview.preview_hash,
                pinned_item_ids: Vec::new(),
                removed_item_ids: Vec::new(),
            },
        )
        .await
        .expect_err("stale preview must fail");

    assert!(matches!(err.kind, ErrorKind::Conflict(message) if message == "preview_stale"));
    let runs = TaskRunRepository::new(pool.clone()).list_by_task(&seed.scope, seed.task_id).await.expect("runs");
    assert!(runs.is_empty());
}
