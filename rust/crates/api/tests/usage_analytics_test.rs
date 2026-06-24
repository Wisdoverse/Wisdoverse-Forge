//! Unit 5.1 coverage for governed context usage analytics.

use agentforge_api::services::usage_analytics::{ContextUsageQuery, RefreshOutcome, UsageAnalyticsService};
use agentforge_api::test_support::tenant_scope_for_ids_with_axes;
use agentforge_core::TenantScope;
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

struct AnalyticsSeed {
    org_id: Uuid,
    workspace_id: Uuid,
    user_id: Uuid,
    project_id: Uuid,
    agent_id: Uuid,
    memory_id: Uuid,
    stale_memory_id: Uuid,
    skill_id: Uuid,
    other_org_memory_id: Uuid,
    scope: TenantScope,
}

struct RunItemSeed<'a> {
    item_id: Uuid,
    item_kind: &'a str,
    title: &'a str,
    status: &'a str,
    feedback_label: Option<&'a str>,
    applied_offset: Duration,
    idempotency_key: String,
}

async fn seed_identity(pool: &PgPool, org_label: &str) -> (Uuid, Uuid, Uuid, Uuid, Uuid, Uuid, TenantScope) {
    let org_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("{org_label} Org"))
        .bind(format!("{org_label}-{org_id}"))
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
        .bind(format!("{org_label}-{user_id}@example.com"))
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
        .bind(format!("{org_label}-team-{team_id}"))
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
    .bind(format!("{org_label}-project-{project_id}"))
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
         VALUES ($1, $2, $3, $4, $5, $6, 'claude', 'idle', 'container')",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(project_id)
    .bind(user_id)
    .bind(format!("{org_label} agent"))
    .execute(pool)
    .await
    .expect("seed agent");

    let scope = tenant_scope_for_ids_with_axes(org_id, user_id, Some(workspace_id), Some(team_id), Some(project_id));
    (org_id, workspace_id, user_id, team_id, project_id, agent_id, scope)
}

async fn seed_item(pool: &PgPool, seed: &AnalyticsSeed, kind: &str, title: &str) -> Uuid {
    let item_id = Uuid::new_v4();
    if kind == "memory" {
        sqlx::query(
            "INSERT INTO memory_items (
                 id, organization_id, workspace_id, owner_user_id, scope_kind, scope_id,
                 title, content, visibility, sensitivity, confidence, last_verified_at, state
             )
             VALUES ($1, $2, $3, $4, 'project', $5, $6, 'content', 'shared', 'internal', 0.95, now(), 'active')",
        )
        .bind(item_id)
        .bind(seed.org_id)
        .bind(seed.workspace_id)
        .bind(seed.user_id)
        .bind(seed.project_id)
        .bind(title)
        .execute(pool)
        .await
        .expect("seed memory");
    } else {
        sqlx::query(
            "INSERT INTO skills (
                 id, organization_id, workspace_id, scope_kind, scope_id, name, content,
                 enabled, state, owner_user_id, sensitivity, provenance
             )
             VALUES ($1, $2, $3, 'project', $4, $5, 'skill body', true, 'active', $6, 'internal', '{}'::jsonb)",
        )
        .bind(item_id)
        .bind(seed.org_id)
        .bind(seed.workspace_id)
        .bind(seed.project_id)
        .bind(title)
        .bind(seed.user_id)
        .execute(pool)
        .await
        .expect("seed skill");
    }
    item_id
}

async fn seed_run_with_item(pool: &PgPool, seed: &AnalyticsSeed, input: RunItemSeed<'_>) {
    let task_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO orchestration_tasks (id, organization_id, title, description, status, created_by, params)
         VALUES ($1, $2, $3, 'analytics fixture', $4, $5, $6)",
    )
    .bind(task_id)
    .bind(seed.org_id)
    .bind(format!("Analytics {}", input.title))
    .bind(input.status)
    .bind(seed.user_id)
    .bind(json!({ "taskKind": "release" }))
    .execute(pool)
    .await
    .expect("seed task");
    sqlx::query(
        "INSERT INTO task_runs (
             id, organization_id, workspace_id, orchestration_task_id, agent_id,
             idempotency_key, status, started_at, finished_at, capability_profile
         )
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(run_id)
    .bind(seed.org_id)
    .bind(seed.workspace_id)
    .bind(task_id)
    .bind(seed.agent_id)
    .bind(input.idempotency_key)
    .bind(input.status)
    .bind(Utc::now() + input.applied_offset)
    .bind(Utc::now() + input.applied_offset + Duration::minutes(3))
    .bind(json!({ "runtime_capability": { "runtime_kind": "container" } }))
    .execute(pool)
    .await
    .expect("seed run");
    sqlx::query(
        "INSERT INTO run_context_injections (
             organization_id, workspace_id, run_id, item_id, item_kind, position,
             adapter, envelope_version, capability_profile, applied_snapshot, applied_at
         )
         VALUES ($1, $2, $3, $4, $5, 0, 'claude', 'v1', $6, $7, $8)",
    )
    .bind(seed.org_id)
    .bind(seed.workspace_id)
    .bind(run_id)
    .bind(input.item_id)
    .bind(input.item_kind)
    .bind(json!({ "cli_tool": "claude", "runtime_kind": "container" }))
    .bind(json!({ "id": input.item_id, "kind": input.item_kind, "title": input.title }))
    .bind(Utc::now() + input.applied_offset)
    .execute(pool)
    .await
    .expect("seed injection");

    if let Some(label) = input.feedback_label {
        sqlx::query(
            "INSERT INTO context_feedback (
                 organization_id, workspace_id, run_id, item_id, item_kind, label, user_id, created_at, updated_at
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now())",
        )
        .bind(seed.org_id)
        .bind(seed.workspace_id)
        .bind(run_id)
        .bind(input.item_id)
        .bind(input.item_kind)
        .bind(label)
        .bind(seed.user_id)
        .execute(pool)
        .await
        .expect("seed feedback");
    }
}

async fn seed_analytics(pool: &PgPool) -> AnalyticsSeed {
    let (org_id, workspace_id, user_id, _, project_id, agent_id, scope) = seed_identity(pool, "usage").await;
    let mut seed = AnalyticsSeed {
        org_id,
        workspace_id,
        user_id,
        project_id,
        agent_id,
        memory_id: Uuid::nil(),
        stale_memory_id: Uuid::nil(),
        skill_id: Uuid::nil(),
        other_org_memory_id: Uuid::nil(),
        scope,
    };
    seed.memory_id = seed_item(pool, &seed, "memory", "Prod deploy memory").await;
    seed.stale_memory_id = seed_item(pool, &seed, "memory", "Old deploy memory").await;
    seed.skill_id = seed_item(pool, &seed, "skill", "Release checklist").await;

    for idx in 0..10 {
        let status = if idx < 8 { "completed" } else { "failed" };
        let feedback = if idx < 3 { Some("useful") } else { None };
        seed_run_with_item(
            pool,
            &seed,
            RunItemSeed {
                item_id: seed.memory_id,
                item_kind: "memory",
                title: "Prod deploy memory",
                status,
                feedback_label: feedback,
                applied_offset: Duration::minutes(i64::from(idx)),
                idempotency_key: format!("top-{idx}"),
            },
        )
        .await;
    }

    seed_run_with_item(
        pool,
        &seed,
        RunItemSeed {
            item_id: seed.stale_memory_id,
            item_kind: "memory",
            title: "Old deploy memory",
            status: "completed",
            feedback_label: None,
            applied_offset: Duration::days(-45),
            idempotency_key: "stale".to_string(),
        },
    )
    .await;

    for idx in 0..3 {
        let feedback = if idx < 2 { Some("wrong") } else { Some("useful") };
        seed_run_with_item(
            pool,
            &seed,
            RunItemSeed {
                item_id: seed.skill_id,
                item_kind: "skill",
                title: "Release checklist",
                status: "completed",
                feedback_label: feedback,
                applied_offset: Duration::minutes(30 + i64::from(idx)),
                idempotency_key: format!("review-{idx}"),
            },
        )
        .await;
    }

    let (other_org_id, other_workspace_id, other_user_id, _, other_project_id, other_agent_id, _) =
        seed_identity(pool, "other").await;
    let other_seed = AnalyticsSeed {
        org_id: other_org_id,
        workspace_id: other_workspace_id,
        user_id: other_user_id,
        project_id: other_project_id,
        agent_id: other_agent_id,
        memory_id: Uuid::nil(),
        stale_memory_id: Uuid::nil(),
        skill_id: Uuid::nil(),
        other_org_memory_id: Uuid::nil(),
        scope: tenant_scope_for_ids_with_axes(
            other_org_id,
            other_user_id,
            Some(other_workspace_id),
            None,
            Some(other_project_id),
        ),
    };
    seed.other_org_memory_id = seed_item(pool, &other_seed, "memory", "Other org memory").await;
    for idx in 0..10 {
        seed_run_with_item(
            pool,
            &other_seed,
            RunItemSeed {
                item_id: seed.other_org_memory_id,
                item_kind: "memory",
                title: "Other org memory",
                status: "completed",
                feedback_label: Some("useful"),
                applied_offset: Duration::minutes(i64::from(idx)),
                idempotency_key: format!("other-{idx}"),
            },
        )
        .await;
    }

    seed
}

#[sqlx::test(migrations = "../db/migrations")]
async fn context_usage_analytics_groups_effectiveness_and_enforces_tenant_scope(pool: PgPool) {
    let seed = seed_analytics(&pool).await;
    let service = UsageAnalyticsService::new(pool.clone());

    assert_eq!(service.refresh_context_usage_snapshot().await.expect("refresh"), RefreshOutcome::Refreshed);

    let data = service
        .context_usage(
            &seed.scope,
            ContextUsageQuery {
                limit: 5,
                min_applied: 10,
                stale_after_days: 30,
                min_success_rate: 0.70,
                negative_rate: 0.30,
            },
        )
        .await
        .expect("context usage analytics");

    assert_eq!(data.summary.applied_count, 14);
    assert_eq!(data.summary.completed_count, 12);
    assert_eq!(data.summary.distinct_items, 3);
    assert!(data.top_useful.iter().any(|item| item.item_id == seed.memory_id && item.success_rate >= 0.8));
    assert!(data.stale_items.iter().any(|item| item.item_id == seed.stale_memory_id));
    assert!(data.needs_review.iter().any(|item| item.item_id == seed.skill_id && item.negative_feedback_rate >= 0.6));
    assert!(data.top_useful.iter().all(|item| item.item_id != seed.other_org_memory_id));
    assert!(data.stale_items.iter().all(|item| item.item_id != seed.other_org_memory_id));
    assert!(data.needs_review.iter().all(|item| item.item_id != seed.other_org_memory_id));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn stale_refresh_metadata_surfaces_dashboard_staleness(pool: PgPool) {
    let seed = seed_analytics(&pool).await;
    let service = UsageAnalyticsService::new(pool.clone());
    service.refresh_context_usage_snapshot().await.expect("refresh");

    sqlx::query(
        "UPDATE context_usage_analytics_refreshes
            SET last_refreshed_at = now() - interval '25 hours'
          WHERE name = 'context_usage_analytics'",
    )
    .execute(&pool)
    .await
    .expect("make snapshot stale");

    let data = service.context_usage(&seed.scope, ContextUsageQuery::default()).await.expect("context usage analytics");

    assert!(data.is_stale);
    assert_eq!(data.stale_after_hours, 24);
}
