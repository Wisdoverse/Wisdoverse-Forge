//! Verifies AgentRepository::create_aggregate:
//!  1. For host-cli agents an `agent.enrolled` audit event is written atomically
//!     in the same transaction as the agents INSERT.
//!  2. Container agents produce no audit event from this path.
//!
//! Tests run against a fresh database with all migrations applied via
//! `#[sqlx::test(migrations = "../db/migrations")]`.

use agentforge_api::{
    domain::agent::{HostCliIdentity, NewAgent},
    repositories::agent::AgentRepository,
};
use agentforge_core::{CliToolKind, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helper — copied verbatim from agent_repository_runtime_kind.rs
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

/// A host-cli NewAgent must produce exactly one `agent.enrolled` event scoped
/// to the same agent_id, atomically with the agents INSERT.
#[sqlx::test(migrations = "../db/migrations")]
async fn create_host_cli_emits_atomic_audit_event(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let scope = make_scope(org_id, user_id);

    let repo = AgentRepository::new(pool.clone());
    let identity = HostCliIdentity::generate();
    let new_agent =
        NewAgent::host_cli(&scope, CliToolKind::Codex, identity, Some("hcli"), None, None, workspace_id, None)
            .expect("build host-cli NewAgent");

    let id = repo.create_aggregate(&scope, new_agent).await.expect("create_aggregate");

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM events WHERE event_type = 'agent.enrolled' AND agent_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("count enrolled events");

    assert_eq!(count.0, 1, "expected exactly one agent.enrolled event for host-cli");
}

/// The enrolled event payload carries the expected fields.
#[sqlx::test(migrations = "../db/migrations")]
async fn create_host_cli_enrolled_event_payload_contains_expected_fields(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let scope = make_scope(org_id, user_id);

    let repo = AgentRepository::new(pool.clone());
    let identity = HostCliIdentity::generate();
    let new_agent =
        NewAgent::host_cli(&scope, CliToolKind::Claude, identity, Some("hcli-payload"), None, None, workspace_id, None)
            .expect("build host-cli NewAgent");

    let id = repo.create_aggregate(&scope, new_agent).await.expect("create_aggregate");

    let row: (serde_json::Value,) =
        sqlx::query_as("SELECT payload FROM events WHERE event_type = 'agent.enrolled' AND agent_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("fetch enrolled event payload");

    let payload = row.0;
    assert_eq!(payload["runtime_kind"], "cli", "runtime_kind must be 'cli'");
    assert_eq!(payload["cli_tool"], "claude", "cli_tool must match");
    assert_eq!(payload["actor_user_id"], serde_json::json!(user_id), "actor_user_id must match the scope user");
}

/// Container agents must not emit any events from create_aggregate.
#[sqlx::test(migrations = "../db/migrations")]
async fn create_container_does_not_emit_enrolled_event(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let scope = make_scope(org_id, user_id);

    let repo = AgentRepository::new(pool.clone());
    let new_agent = NewAgent::container(&scope, CliToolKind::Codex, Some("c1"), None, None, workspace_id, None, None)
        .expect("build container NewAgent");

    let id = repo.create_aggregate(&scope, new_agent).await.expect("create_aggregate");

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events WHERE agent_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("count all events");

    assert_eq!(count.0, 0, "no audit event expected for container kind");
}

/// API agents must not emit any events from create_aggregate.
#[sqlx::test(migrations = "../db/migrations")]
async fn create_api_agent_does_not_emit_event(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let scope = make_scope(org_id, user_id);

    let repo = AgentRepository::new(pool.clone());
    let new_agent =
        NewAgent::api(&scope, "anthropic", "claude-sonnet-4-6", Some("api-agent"), None, workspace_id, None)
            .expect("build api NewAgent");

    let id = repo.create_aggregate(&scope, new_agent).await.expect("create_aggregate");

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events WHERE agent_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("count all events");

    assert_eq!(count.0, 0, "no audit event expected for api kind");
}

/// The UUID returned by create_aggregate matches the row inserted in agents.
#[sqlx::test(migrations = "../db/migrations")]
async fn create_aggregate_returns_correct_agent_id(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let scope = make_scope(org_id, user_id);

    let repo = AgentRepository::new(pool.clone());
    let identity = HostCliIdentity::generate();
    let new_agent =
        NewAgent::host_cli(&scope, CliToolKind::Claude, identity, Some("id-check"), None, None, workspace_id, None)
            .expect("build host-cli NewAgent");

    let id = repo.create_aggregate(&scope, new_agent).await.expect("create_aggregate");

    let exists: (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM agents WHERE id = $1)")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("check agent exists");

    assert!(exists.0, "agents row must exist with the returned UUID");
}
