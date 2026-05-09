//! Unit 5.2 coverage for governed context audit projection.

use agentforge_api::repositories::governance_audit::{
    GOVERNANCE_CONTEXT_AUDIT_PREFIX, GovernanceAuditFilter, GovernanceAuditRepository,
};
use agentforge_api::test_support::tenant_scope_for_ids_with_axes;
use agentforge_core::TenantScope;
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

struct AuditSeed {
    org_id: Uuid,
    user_id: Uuid,
    other_user_id: Uuid,
    project_id: Uuid,
    other_project_id: Uuid,
    memory_id: Uuid,
    other_memory_id: Uuid,
    scope: TenantScope,
}

async fn seed_identity(pool: &PgPool) -> AuditSeed {
    let org_id = Uuid::new_v4();
    let workspace_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let other_user_id = Uuid::new_v4();
    let team_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let other_project_id = Uuid::new_v4();
    let memory_id = Uuid::new_v4();
    let other_memory_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Audit Org', $2)")
        .bind(org_id)
        .bind(format!("audit-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, 'Audit')")
        .bind(workspace_id)
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");
    for (user, label) in [(user_id, "main"), (other_user_id, "other")] {
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user)
            .bind(format!("audit-{label}-{user}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')")
            .bind(org_id)
            .bind(user)
            .execute(pool)
            .await
            .expect("seed org member");
    }
    sqlx::query("INSERT INTO teams (id, organization_id, name, slug) VALUES ($1, $2, 'Audit Team', $3)")
        .bind(team_id)
        .bind(org_id)
        .bind(format!("audit-team-{team_id}"))
        .execute(pool)
        .await
        .expect("seed team");
    sqlx::query("INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(team_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed team member");
    for (project, slug) in [(project_id, "visible"), (other_project_id, "hidden")] {
        sqlx::query(
            "INSERT INTO projects (id, organization_id, workspace_id, team_id, name, slug)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(project)
        .bind(org_id)
        .bind(workspace_id)
        .bind(team_id)
        .bind(format!("Audit {slug}"))
        .bind(format!("audit-{slug}-{project}"))
        .execute(pool)
        .await
        .expect("seed project");
    }
    sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1, $2, 'maintainer')")
        .bind(project_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed project member");

    for (memory, owner, project, title) in [
        (memory_id, user_id, project_id, "Visible memory"),
        (other_memory_id, other_user_id, other_project_id, "Hidden memory"),
    ] {
        sqlx::query(
            "INSERT INTO memory_items (
                 id, organization_id, workspace_id, owner_user_id, scope_kind, scope_id,
                 title, content, visibility, sensitivity, confidence, state
             )
             VALUES ($1, $2, $3, $4, 'project', $5, $6, 'content', 'shared', 'internal', 0.9, 'active')",
        )
        .bind(memory)
        .bind(org_id)
        .bind(workspace_id)
        .bind(owner)
        .bind(project)
        .bind(title)
        .execute(pool)
        .await
        .expect("seed memory");
    }

    let scope = tenant_scope_for_ids_with_axes(org_id, user_id, Some(workspace_id), Some(team_id), Some(project_id));
    AuditSeed { org_id, user_id, other_user_id, project_id, other_project_id, memory_id, other_memory_id, scope }
}

async fn insert_governance_audit(
    pool: &PgPool,
    seed: &AuditSeed,
    actor: Uuid,
    action: &str,
    item_id: Uuid,
    created_offset: Duration,
) -> Uuid {
    let row_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO audit_log (
             id, organization_id, user_id, action, resource_type, resource_id, details, created_at
         )
         VALUES ($1, $2, $3, $4, 'context_feedback', NULL, $5, $6)",
    )
    .bind(row_id)
    .bind(seed.org_id)
    .bind(actor)
    .bind(action)
    .bind(json!({
        "item_id": item_id,
        "item_kind": "memory",
        "label": "useful"
    }))
    .bind(Utc::now() + created_offset)
    .execute(pool)
    .await
    .expect("seed audit row");
    row_id
}

#[sqlx::test(migrations = "../db/migrations")]
async fn governance_audit_projection_enforces_scope_and_hides_cross_scope_raw_id(pool: PgPool) {
    let seed = seed_identity(&pool).await;
    let visible_row = insert_governance_audit(
        &pool,
        &seed,
        seed.other_user_id,
        "governance.context.feedback.recorded",
        seed.memory_id,
        Duration::minutes(-2),
    )
    .await;
    let authored_hidden_row = insert_governance_audit(
        &pool,
        &seed,
        seed.user_id,
        "governance.context.feedback.recorded",
        seed.other_memory_id,
        Duration::minutes(-1),
    )
    .await;
    insert_governance_audit(
        &pool,
        &seed,
        seed.other_user_id,
        "governance.context.feedback.recorded",
        seed.other_memory_id,
        Duration::minutes(0),
    )
    .await;

    let repo = GovernanceAuditRepository::new(pool);
    let rows = repo
        .list(
            &seed.scope,
            GovernanceAuditFilter {
                event_type: None,
                event_prefix: Some(GOVERNANCE_CONTEXT_AUDIT_PREFIX),
                item_kind: Some("memory"),
                scope_kind: None,
                scope_id: None,
                user_id: None,
                from: None,
                to: None,
                limit: Some(10),
                offset: Some(0),
            },
            false,
        )
        .await
        .expect("list audit projection");

    assert_eq!(rows.len(), 2);
    let visible = rows.iter().find(|row| row.id == visible_row).expect("visible row");
    assert_eq!(visible.subject_item_id, Some(seed.memory_id));
    assert_eq!(visible.subject_scope_kind.as_deref(), Some("project"));
    assert_eq!(visible.subject_scope_id, Some(seed.project_id));
    assert!(visible.visible_by_scope);

    let authored_hidden = rows.iter().find(|row| row.id == authored_hidden_row).expect("authored hidden row");
    assert_eq!(authored_hidden.subject_item_id, Some(seed.other_memory_id));
    assert_eq!(authored_hidden.subject_scope_id, Some(seed.other_project_id));
    assert!(!authored_hidden.visible_by_scope);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn governance_audit_projection_filters_time_range_and_event_type(pool: PgPool) {
    let seed = seed_identity(&pool).await;
    insert_governance_audit(
        &pool,
        &seed,
        seed.user_id,
        "governance.context.feedback.recorded",
        seed.memory_id,
        Duration::hours(-2),
    )
    .await;
    let expected = insert_governance_audit(
        &pool,
        &seed,
        seed.user_id,
        "governance.context.memory.updated",
        seed.memory_id,
        Duration::minutes(-5),
    )
    .await;

    let repo = GovernanceAuditRepository::new(pool);
    let rows = repo
        .list(
            &seed.scope,
            GovernanceAuditFilter {
                event_type: Some("governance.context.memory.updated"),
                event_prefix: Some(GOVERNANCE_CONTEXT_AUDIT_PREFIX),
                item_kind: Some("memory"),
                scope_kind: Some("project"),
                scope_id: Some(seed.project_id),
                user_id: Some(seed.user_id),
                from: Some(Utc::now() - Duration::hours(1)),
                to: Some(Utc::now() + Duration::minutes(1)),
                limit: Some(10),
                offset: Some(0),
            },
            false,
        )
        .await
        .expect("filtered audit projection");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, expected);
    assert!(rows[0].visible_by_scope);
}
