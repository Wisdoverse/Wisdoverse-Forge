//! Unit 3.4 coverage for immutable run context injection records.

use std::sync::Arc;

use agentforge_api::repositories::orchestration::run_context_injection::RunContextInjectionRepository;
use agentforge_api::repositories::orchestration::task_run::TaskRunRepository;
use agentforge_api::repositories::orchestration::{OrchestrationTaskRepository, ParticipantRepository};
use agentforge_api::repositories::runtime_capability::RuntimeCapabilityRepository;
use agentforge_api::services::context_resolver::ContextResolverService;
use agentforge_api::services::orchestration::OrchestrationService;
use agentforge_api::services::runtime_capability_registry::RuntimeCapabilityRegistryService;
use agentforge_api::test_support::tenant_scope_for_ids_with_axes;
use agentforge_core::TenantScope;
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

struct InjectionSeed {
    org_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
    project_id: Uuid,
    task_id: Uuid,
    scope: TenantScope,
}

async fn resolver(pool: PgPool, refresh_registry: bool) -> ContextResolverService {
    let registry = RuntimeCapabilityRegistryService::new(RuntimeCapabilityRepository::new(pool.clone()));
    if refresh_registry {
        registry.refresh_from_code().await.expect("refresh runtime capability registry");
    }
    ContextResolverService::new(pool, registry)
}

async fn seed_base(pool: &PgPool, task_text: &str) -> InjectionSeed {
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
         VALUES ($1, $2, $3, $4, $5, 'injection-agent', 'claude', 'idle')",
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
         VALUES ($1, $2, 'injection-agent', ARRAY['claude'], 'available', now())",
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

    InjectionSeed { org_id, workspace_id, user_id, project_id, task_id, scope }
}

async fn insert_memory(
    pool: &PgPool,
    seed: &InjectionSeed,
    title: &str,
    content: &str,
    confidence: f64,
    last_verified_offset: Duration,
) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO memory_items (
               organization_id, workspace_id, owner_user_id, scope_kind, scope_id,
               title, content, visibility, sensitivity, confidence, last_verified_at, state
           )
           VALUES ($1, $2, $3, 'project', $4, $5, $6, 'shared', 'internal', $7, $8, 'active')
           RETURNING id"#,
    )
    .bind(seed.org_id)
    .bind(seed.workspace_id)
    .bind(seed.user_id)
    .bind(seed.project_id)
    .bind(title)
    .bind(content)
    .bind(confidence)
    .bind(Utc::now() + last_verified_offset)
    .fetch_one(pool)
    .await
    .expect("insert memory")
}

async fn dispatch_with_resolver(pool: PgPool, seed: &InjectionSeed, refresh_registry: bool) -> Uuid {
    let context_resolver = Arc::new(resolver(pool.clone(), refresh_registry).await);
    let service = OrchestrationService::new(
        OrchestrationTaskRepository::new(pool.clone()),
        ParticipantRepository::new(pool.clone()),
    )
    .with_context_resolver(context_resolver);

    let working = service.dispatch_task(&seed.scope, seed.task_id).await.expect("dispatch with context resolver");
    assert_eq!(working.status, "working");

    let runs =
        TaskRunRepository::new(pool.clone()).list_by_task(&seed.scope, seed.task_id).await.expect("list task runs");
    assert_eq!(runs.len(), 1);
    runs[0].id
}

#[sqlx::test(migrations = "../db/migrations")]
async fn dispatch_skips_context_injection_when_feature_flag_is_off(pool: PgPool) {
    let seed = seed_base(&pool, "Use prod-ext validation").await;
    insert_memory(
        &pool,
        &seed,
        "disabled injection memory",
        "This memory must not be injected while the rollout flag is off.",
        1.0,
        Duration::minutes(1),
    )
    .await;

    let context_resolver = Arc::new(resolver(pool.clone(), true).await);
    let service = OrchestrationService::new(
        OrchestrationTaskRepository::new(pool.clone()),
        ParticipantRepository::new(pool.clone()),
    )
    .with_context_resolver(context_resolver)
    .with_context_injection_enabled(false);

    let working = service.dispatch_task(&seed.scope, seed.task_id).await.expect("dispatch without context injection");
    assert_eq!(working.status, "working");

    let runs =
        TaskRunRepository::new(pool.clone()).list_by_task(&seed.scope, seed.task_id).await.expect("list task runs");
    assert_eq!(runs.len(), 1);
    let injections = RunContextInjectionRepository::new(pool.clone())
        .list_by_run(&seed.scope, runs[0].id)
        .await
        .expect("list injections");
    assert!(injections.is_empty());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn dispatch_records_immutable_context_injection_snapshot(pool: PgPool) {
    let seed = seed_base(&pool, "Use prod-ext validation").await;
    let first_memory_id = insert_memory(
        &pool,
        &seed,
        "prod-ext first injection",
        "Inject first prod-ext memory into the assigned run.",
        1.0,
        Duration::minutes(3),
    )
    .await;
    let second_memory_id = insert_memory(
        &pool,
        &seed,
        "prod-ext second injection",
        "Inject second prod-ext memory into the assigned run.",
        1.0,
        Duration::minutes(2),
    )
    .await;
    let third_memory_id = insert_memory(
        &pool,
        &seed,
        "prod-ext third injection",
        "Inject third prod-ext memory into the assigned run.",
        1.0,
        Duration::minutes(1),
    )
    .await;

    let run_id = dispatch_with_resolver(pool.clone(), &seed, true).await;
    let repo = RunContextInjectionRepository::new(pool.clone());
    let rows = repo.list_by_run(&seed.scope, run_id).await.expect("list injections");

    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].run_id, run_id);
    assert_eq!(rows[0].item_id, first_memory_id);
    assert_eq!(rows[0].item_kind, "memory");
    assert_eq!(rows[0].position, 0);
    assert_eq!(rows[0].adapter, "claude");
    assert_eq!(rows[0].envelope_version, "v1");
    assert_eq!(rows[0].degradation_reason, None);
    assert_eq!(rows[0].capability_profile["cli_tool"], "claude");
    assert_eq!(rows[0].capability_profile["runtime_kind"], "container");
    assert_eq!(rows[0].applied_snapshot["title"], "prod-ext first injection");
    assert_eq!(rows[0].applied_snapshot["content"], "Inject first prod-ext memory into the assigned run.");
    assert_eq!(rows[1].item_id, second_memory_id);
    assert_eq!(rows[1].position, 1);
    assert_eq!(rows[2].item_id, third_memory_id);
    assert_eq!(rows[2].position, 2);

    sqlx::query(
        "UPDATE memory_items
            SET title = 'revoked later',
                content = 'mutated later',
                state = 'revoked',
                revoked_at = now()
          WHERE id = $1",
    )
    .bind(first_memory_id)
    .execute(&pool)
    .await
    .expect("mutate source memory");

    let after_revoke = repo.list_by_run(&seed.scope, run_id).await.expect("list injections after revoke");
    assert_eq!(after_revoke[0].applied_snapshot["title"], "prod-ext first injection");
    assert_eq!(after_revoke[0].applied_snapshot["content"], "Inject first prod-ext memory into the assigned run.");

    let runs = repo.runs_for_item(&seed.scope, first_memory_id, "memory", 20, 0).await.expect("runs for item");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].run_id, run_id);

    let mut tx = pool.begin().await.expect("begin explain tx");
    sqlx::query("SET LOCAL enable_seqscan = off").execute(&mut *tx).await.expect("disable seqscan");
    let plan = RunContextInjectionRepository::explain_runs_for_item_in_tx(&mut tx, first_memory_id, "memory")
        .await
        .expect("explain");
    assert!(
        plan.iter().any(|line| line.contains("idx_run_context_injections_item_kind_applied")),
        "expected item index in plan: {plan:?}"
    );
    tx.rollback().await.expect("rollback");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn budget_truncated_items_are_not_recorded_as_injections(pool: PgPool) {
    let seed = seed_base(&pool, "Use prod-ext validation").await;
    insert_memory(
        &pool,
        &seed,
        "prod-ext too large",
        "This memory is deliberately too large for the conservative fallback context budget.",
        1.0,
        Duration::minutes(0),
    )
    .await;

    let run_id = dispatch_with_resolver(pool.clone(), &seed, false).await;
    let rows =
        RunContextInjectionRepository::new(pool).list_by_run(&seed.scope, run_id).await.expect("list injections");
    assert!(rows.is_empty(), "budget-truncated context must not be recorded as applied");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn runtime_capability_fallback_reason_is_recorded_for_applied_items(pool: PgPool) {
    let seed = seed_base(&pool, "Use prod-ext validation").await;
    insert_memory(&pool, &seed, "use", "u", 1.0, Duration::minutes(0)).await;

    let run_id = dispatch_with_resolver(pool.clone(), &seed, false).await;
    let rows =
        RunContextInjectionRepository::new(pool).list_by_run(&seed.scope, run_id).await.expect("list injections");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].degradation_reason.as_deref(), Some("runtime_capability_fallback"));
    assert_eq!(rows[0].envelope_version, "v1");
    assert_eq!(rows[0].capability_profile["max_context_tokens"], json!(1));
}
