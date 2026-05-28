//! Unit 3.2 coverage for scoped context resolution.

use std::sync::Arc;

use agentforge_api::repositories::orchestration::task_run::TaskRunRepository;
use agentforge_api::repositories::orchestration::{OrchestrationTaskRepository, ParticipantRepository};
use agentforge_api::repositories::runtime_capability::RuntimeCapabilityRepository;
use agentforge_api::services::context_resolver::{
    ContextItemKind, ContextResolverService, DegradationReason, ResolveContextInput,
};
use agentforge_api::services::orchestration::OrchestrationService;
use agentforge_api::services::runtime_capability_registry::RuntimeCapabilityRegistryService;
use agentforge_api::test_support::tenant_scope_for_ids_with_axes;
use agentforge_core::{AgentId, ErrorKind, RuntimeCapability, RuntimeKind, TenantScope};
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

struct ResolverSeed {
    org_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
    team_id: Uuid,
    project_id: Uuid,
    agent_id: Uuid,
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

async fn seed_base(pool: &PgPool, task_text: &str) -> ResolverSeed {
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
         VALUES ($1, $2, $3, $4, 'Control Plane', $5)",
    )
    .bind(project_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(team_id)
    .bind(format!("control-plane-{project_id}"))
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
        "INSERT INTO agents (id, organization_id, workspace_id, project_id, user_id, name, cli_tool, status, runtime_kind)
         VALUES ($1, $2, $3, $4, $5, 'resolver-agent', 'claude', 'idle', 'container')",
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
         VALUES ($1, $2, 'resolver-agent', ARRAY['claude'], 'available', now())",
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

    ResolverSeed { org_id, workspace_id, user_id, team_id, project_id, agent_id, task_id, scope }
}

async fn insert_memory(
    pool: &PgPool,
    seed: &ResolverSeed,
    scope_kind: &str,
    scope_id: Uuid,
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
           VALUES ($1, $2, $3, $4, $5, $6, $7, 'shared', 'internal', $8, $9, 'active')
           RETURNING id"#,
    )
    .bind(seed.org_id)
    .bind(seed.workspace_id)
    .bind(seed.user_id)
    .bind(scope_kind)
    .bind(scope_id)
    .bind(title)
    .bind(content)
    .bind(confidence)
    .bind(Utc::now() + last_verified_offset)
    .fetch_one(pool)
    .await
    .expect("insert memory")
}

async fn insert_skill(pool: &PgPool, seed: &ResolverSeed, trigger_pattern: &str, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        r#"INSERT INTO skills (
               organization_id, workspace_id, scope_kind, scope_id, owner_user_id,
               name, description, trigger_pattern, content, enabled, state, sensitivity
           )
           VALUES ($1, $2, 'project', $3, $4, $5, 'Deployment helper', $6, 'Use the deployment helper.', TRUE, 'active', 'internal')
           RETURNING id"#,
    )
    .bind(seed.org_id)
    .bind(seed.workspace_id)
    .bind(seed.project_id)
    .bind(seed.user_id)
    .bind(name)
    .bind(trigger_pattern)
    .fetch_one(pool)
    .await
    .expect("insert skill")
}

#[sqlx::test(migrations = "../db/migrations")]
async fn resolves_visible_memory_and_triggered_skills(pool: PgPool) {
    let seed = seed_base(&pool, "Run make prod-ext and verify Prometheus signals").await;
    let recent = insert_memory(
        &pool,
        &seed,
        "team",
        seed.team_id,
        "prod-ext validation",
        "After merge, run make prod-ext and check health.",
        0.80,
        Duration::minutes(10),
    )
    .await;
    let older = insert_memory(
        &pool,
        &seed,
        "user",
        seed.user_id,
        "Prometheus local",
        "Prometheus and Grafana are available locally for governed context checks.",
        0.99,
        Duration::minutes(-10),
    )
    .await;
    let out_of_scope_team = Uuid::new_v4();
    sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Other', $3)")
        .bind(out_of_scope_team)
        .bind(seed.org_id)
        .bind(format!("other-{out_of_scope_team}"))
        .execute(&pool)
        .await
        .expect("seed other team");
    let hidden = insert_memory(
        &pool,
        &seed,
        "team",
        out_of_scope_team,
        "hidden prod-ext note",
        "This should not be visible.",
        1.0,
        Duration::minutes(20),
    )
    .await;
    let skill_id = insert_skill(&pool, &seed, "prod-ext", "prod-ext helper").await;
    let nonmatching_skill = insert_skill(&pool, &seed, "unrelated-trigger", "unrelated helper").await;

    let service = resolver(pool.clone(), true).await;
    let resolved = service
        .resolve(
            &seed.scope.scoped_read(),
            ResolveContextInput { task_id: seed.task_id, agent_id: AgentId::from(seed.agent_id) },
        )
        .await
        .expect("resolve context");

    assert_eq!(resolved.envelope_version, "v1");
    assert_eq!(
        resolved.capability,
        RuntimeCapability::for_cli_tool(agentforge_core::CliToolKind::Claude, RuntimeKind::Container)
    );
    assert!(resolved.degradation.is_empty());

    let applied_ids: Vec<Uuid> = resolved.applied.iter().map(|item| item.id).collect();
    assert_eq!(applied_ids, vec![recent, older]);
    assert!(!applied_ids.contains(&hidden));
    assert!(resolved.applied.iter().all(|item| item.kind == ContextItemKind::Memory));

    let suggested_ids: Vec<Uuid> = resolved.suggested.iter().map(|item| item.id).collect();
    assert_eq!(suggested_ids, vec![skill_id]);
    assert!(!suggested_ids.contains(&nonmatching_skill));
    assert_eq!(resolved.suggested[0].kind, ContextItemKind::Skill);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn excludes_revoked_and_expired_memory(pool: PgPool) {
    let seed = seed_base(&pool, "Use prod-ext validation").await;
    let expired = insert_memory(
        &pool,
        &seed,
        "project",
        seed.project_id,
        "expired prod-ext",
        "Expired prod-ext advice.",
        0.9,
        Duration::minutes(0),
    )
    .await;
    sqlx::query("UPDATE memory_items SET ttl_expires_at = now() - interval '1 minute' WHERE id = $1")
        .bind(expired)
        .execute(&pool)
        .await
        .expect("expire memory");
    let revoked = insert_memory(
        &pool,
        &seed,
        "project",
        seed.project_id,
        "revoked prod-ext",
        "Revoked prod-ext advice.",
        0.9,
        Duration::minutes(0),
    )
    .await;
    sqlx::query("UPDATE memory_items SET state = 'revoked', revoked_at = now() WHERE id = $1")
        .bind(revoked)
        .execute(&pool)
        .await
        .expect("revoke memory");

    let service = resolver(pool.clone(), true).await;
    let resolved = service
        .resolve(
            &seed.scope.scoped_read(),
            ResolveContextInput { task_id: seed.task_id, agent_id: AgentId::from(seed.agent_id) },
        )
        .await
        .expect("resolve context");

    assert!(resolved.applied.is_empty());
    assert!(resolved.suggested.is_empty());
    assert!(resolved.degradation.is_empty());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn budget_truncation_drops_lowest_ranked_items(pool: PgPool) {
    let seed = seed_base(&pool, "Use prod-ext validation").await;
    insert_memory(
        &pool,
        &seed,
        "project",
        seed.project_id,
        "prod-ext high",
        "This item is too large for the conservative fallback budget.",
        1.0,
        Duration::minutes(0),
    )
    .await;

    let service = resolver(pool.clone(), false).await;
    let resolved = service
        .resolve(
            &seed.scope.scoped_read(),
            ResolveContextInput { task_id: seed.task_id, agent_id: AgentId::from(seed.agent_id) },
        )
        .await
        .expect("resolve context with fallback budget");

    assert_eq!(resolved.capability.max_context_tokens, 1);
    assert!(resolved.applied.is_empty());
    assert!(resolved.degradation.contains(&DegradationReason::BudgetTruncated));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn memo_keeps_preview_stable_within_ttl(pool: PgPool) {
    let seed = seed_base(&pool, "Use prod-ext validation").await;
    let first = insert_memory(
        &pool,
        &seed,
        "project",
        seed.project_id,
        "prod-ext first",
        "Initial prod-ext context.",
        0.5,
        Duration::minutes(0),
    )
    .await;
    let service = resolver(pool.clone(), true).await;

    let before = service
        .resolve(
            &seed.scope.scoped_read(),
            ResolveContextInput { task_id: seed.task_id, agent_id: AgentId::from(seed.agent_id) },
        )
        .await
        .expect("resolve initial context");
    assert_eq!(before.applied.iter().map(|item| item.id).collect::<Vec<_>>(), vec![first]);

    let later = insert_memory(
        &pool,
        &seed,
        "project",
        seed.project_id,
        "prod-ext later",
        "A newer prod-ext context row should wait for memo expiry.",
        1.0,
        Duration::minutes(30),
    )
    .await;

    let after = service
        .resolve(
            &seed.scope.scoped_read(),
            ResolveContextInput { task_id: seed.task_id, agent_id: AgentId::from(seed.agent_id) },
        )
        .await
        .expect("resolve memoized context");
    assert_eq!(after.applied.iter().map(|item| item.id).collect::<Vec<_>>(), vec![first]);
    assert!(!after.applied.iter().any(|item| item.id == later));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn cross_tenant_task_or_agent_access_yields_not_found(pool: PgPool) {
    let seed = seed_base(&pool, "Use prod-ext validation").await;
    let other_org = Uuid::new_v4();
    let scope = tenant_scope_for_ids_with_axes(
        other_org,
        seed.user_id,
        Some(seed.workspace_id),
        Some(seed.team_id),
        Some(seed.project_id),
    );

    let service = resolver(pool.clone(), true).await;
    let err = service
        .resolve(
            &scope.scoped_read(),
            ResolveContextInput { task_id: seed.task_id, agent_id: AgentId::from(seed.agent_id) },
        )
        .await
        .expect_err("cross-tenant resolve must fail");

    assert!(matches!(err.kind, ErrorKind::NotFound(_)), "expected not found, got {}", err.kind);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn orchestration_assignment_records_resolved_context_profile(pool: PgPool) {
    let seed = seed_base(&pool, "Use prod-ext validation").await;
    let memory_id = insert_memory(
        &pool,
        &seed,
        "project",
        seed.project_id,
        "prod-ext injection",
        "Inject this prod-ext memory into the assigned run.",
        1.0,
        Duration::minutes(0),
    )
    .await;

    let context_resolver = Arc::new(resolver(pool.clone(), true).await);
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
    assert_eq!(runs[0].capability_profile["runtime_capability"]["runtime_kind"], "container");
    assert_eq!(runs[0].capability_profile["context_resolution"]["applied"][0]["id"], json!(memory_id));
    assert_eq!(runs[0].capability_profile["context_resolution"]["envelope_version"], "v1");

    let assignment_payload: serde_json::Value = sqlx::query_scalar(
        "SELECT payload FROM orchestration_outbox WHERE aggregate_id = $1 AND event_type = 'assignment'",
    )
    .bind(seed.task_id)
    .fetch_one(&pool)
    .await
    .expect("assignment outbox payload");
    assert_eq!(assignment_payload["context_envelope"]["envelope_version"], "v1");
    assert_eq!(assignment_payload["context_envelope"]["run_id"], json!(runs[0].id));
    assert_eq!(
        assignment_payload["context_envelope"]["applied"][0]["content"],
        "Inject this prod-ext memory into the assigned run."
    );
}
