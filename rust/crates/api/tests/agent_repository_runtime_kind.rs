//! Verifies AgentListItem.runtime_kind round-trips from the agents table.
//!
//! Tests run against a fresh database with all migrations applied via
//! `#[sqlx::test(migrations = "../db/migrations")]`.

use agentforge_api::repositories::agent::AgentRepository;
use agentforge_core::RuntimeKind;
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers — mirrors the pattern from agents_runtime_kind_constraint.rs
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
// Test: list_with_owner returns runtime_kind
// ---------------------------------------------------------------------------

/// Inserts one agent of each runtime_kind and confirms that
/// `AgentRepository::list_with_owner` populates the `runtime_kind` field
/// correctly on each returned `AgentListItem`.
#[sqlx::test(migrations = "../db/migrations")]
async fn list_with_owner_returns_runtime_kind(pool: PgPool) {
    let (org_id, ws_id, user_id) = seed_org_workspace_user(&pool).await;

    // Insert a cli agent (host-cli runtime shape)
    let cli_agent_id = Uuid::new_v4();
    let runtime_id = format!("host-{}", Uuid::new_v4());
    sqlx::query(
        r#"INSERT INTO agents (id, organization_id, workspace_id, user_id, status,
                               runtime_kind, cli_tool, runtime_id)
           VALUES ($1, $2, $3, $4, 'offline', 'cli', 'codex', $5)"#,
    )
    .bind(cli_agent_id)
    .bind(org_id)
    .bind(ws_id)
    .bind(user_id)
    .bind(&runtime_id)
    .execute(&pool)
    .await
    .expect("insert cli agent");

    // Insert a container agent
    let container_agent_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO agents (id, organization_id, workspace_id, user_id, status,
                               runtime_kind, cli_tool)
           VALUES ($1, $2, $3, $4, 'idle', 'container', 'claude')"#,
    )
    .bind(container_agent_id)
    .bind(org_id)
    .bind(ws_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("insert container agent");

    // Insert an api agent
    let api_agent_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO agents (id, organization_id, workspace_id, user_id, status,
                               runtime_kind)
           VALUES ($1, $2, $3, $4, 'idle', 'api')"#,
    )
    .bind(api_agent_id)
    .bind(org_id)
    .bind(ws_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("insert api agent");

    // Use the actual repository
    let repo = AgentRepository::new(pool.clone());
    let scope = agentforge_api::test_support::tenant_scope_for_ids(org_id, user_id);
    let items = repo.list_with_owner(&scope, 100, 0).await.expect("list_with_owner");

    assert_eq!(items.len(), 3, "expected 3 agents, got {}", items.len());

    let find = |id: Uuid| {
        items
            .iter()
            .find(|a| a.id == id)
            .unwrap_or_else(|| panic!("agent {id} not found in list"))
    };

    assert_eq!(
        find(cli_agent_id).runtime_kind,
        RuntimeKind::Cli,
        "cli agent must have RuntimeKind::Cli"
    );
    assert_eq!(
        find(container_agent_id).runtime_kind,
        RuntimeKind::Container,
        "container agent must have RuntimeKind::Container"
    );
    assert_eq!(
        find(api_agent_id).runtime_kind,
        RuntimeKind::Api,
        "api agent must have RuntimeKind::Api"
    );
}

// ---------------------------------------------------------------------------
// Test: find_with_owner_by_id returns runtime_kind
// ---------------------------------------------------------------------------

/// Confirms that `find_with_owner_by_id` also populates the `runtime_kind`
/// field correctly — it uses the same `AGENT_ENRICHED_SELECT` constant.
#[sqlx::test(migrations = "../db/migrations")]
async fn find_with_owner_by_id_returns_runtime_kind(pool: PgPool) {
    let (org_id, ws_id, user_id) = seed_org_workspace_user(&pool).await;

    let agent_id = Uuid::new_v4();
    let runtime_id = format!("host-{}", Uuid::new_v4());
    sqlx::query(
        r#"INSERT INTO agents (id, organization_id, workspace_id, user_id, status,
                               runtime_kind, cli_tool, runtime_id)
           VALUES ($1, $2, $3, $4, 'offline', 'cli', 'claude', $5)"#,
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(ws_id)
    .bind(user_id)
    .bind(&runtime_id)
    .execute(&pool)
    .await
    .expect("insert cli agent");

    let repo = AgentRepository::new(pool);
    let scope = agentforge_api::test_support::tenant_scope_for_ids(org_id, user_id);
    let item = repo
        .find_with_owner_by_id(&scope, agentforge_core::AgentId::from(agent_id))
        .await
        .expect("find_with_owner_by_id");

    assert_eq!(
        item.runtime_kind,
        RuntimeKind::Cli,
        "find_with_owner_by_id must return RuntimeKind::Cli for cli agent"
    );
}
