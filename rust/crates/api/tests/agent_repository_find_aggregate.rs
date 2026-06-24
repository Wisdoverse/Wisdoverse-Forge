//! Verifies AgentRepository::find_aggregate:
//!  1. Returns a typed `AgentAggregate` with the correct `runtime_kind`.
//!  2. Returns a not-found error when the agent exists in a different org.
//!
//! Tests run against a fresh database with all migrations applied via
//! `#[sqlx::test(migrations = "../db/migrations")]`.

use agentforge_api::{domain::agent::NewAgent, repositories::agent::AgentRepository};
use agentforge_core::{CliToolKind, RuntimeKind, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helper — copied verbatim from agent_repository_create_aggregate.rs
// ---------------------------------------------------------------------------

/// Seed a minimal (organization + workspace + user) triple.
///
/// Uses workspace_id == org_id (the same UUID trick used project-wide).
/// Returns (org_id, workspace_id, user_id).
async fn seed_org_workspace_user(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Test Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed organization");

    // workspace id == org_id
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

    (org_id, org_id, user_id) // (org_id, workspace_id, user_id)
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_scope(org_id: Uuid, user_id: Uuid) -> TenantScope {
    agentforge_api::test_support::tenant_scope_for_ids(org_id, user_id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `find_aggregate` returns a typed `AgentAggregate` with `RuntimeKind::Container`
/// for a container agent created via `create_aggregate`.
#[sqlx::test(migrations = "../db/migrations")]
async fn find_aggregate_returns_typed_runtime_kind(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let scope = make_scope(org_id, user_id);
    let repo = AgentRepository::new(pool.clone());

    let new_agent =
        NewAgent::container(&scope, CliToolKind::Codex, Some("a"), None, None, workspace_id, None, None).unwrap();
    let id = repo.create_aggregate(&scope, new_agent).await.unwrap();

    let agg = repo.find_aggregate(&scope, id).await.unwrap();
    assert_eq!(agg.runtime_kind(), RuntimeKind::Container);
}

/// `find_aggregate` returns a not-found error when the agent belongs to a
/// different organization (cross-org lookup must be rejected).
#[sqlx::test(migrations = "../db/migrations")]
async fn find_aggregate_404s_cross_org(pool: PgPool) {
    let (org_a, workspace_a, user_a) = seed_org_workspace_user(&pool).await;
    let (org_b, _workspace_b, user_b) = seed_org_workspace_user(&pool).await;
    let scope_a = make_scope(org_a, user_a);
    let scope_b = make_scope(org_b, user_b);
    let repo = AgentRepository::new(pool.clone());

    let new_agent =
        NewAgent::container(&scope_a, CliToolKind::Codex, Some("a"), None, None, workspace_a, None, None).unwrap();
    let id = repo.create_aggregate(&scope_a, new_agent).await.unwrap();

    let res = repo.find_aggregate(&scope_b, id).await;
    assert!(res.is_err(), "cross-org lookup must fail");
}
