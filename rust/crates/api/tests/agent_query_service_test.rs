//! Integration tests for [`AgentQueryService::find_by_runtime_kind`].
//!
//! Each test runs against a fresh database with all migrations applied via
//! `#[sqlx::test(migrations = "../db/migrations")]`.

use agentforge_api::{
    domain::agent::{HostCliIdentity, NewAgent},
    repositories::agent::AgentRepository,
    services::agent_query::AgentQueryService,
};
use agentforge_core::{CliToolKind, OrgId, RuntimeKind, TenantScope, UserId};
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

fn make_scope(org_id: Uuid, user_id: Uuid) -> TenantScope {
    TenantScope::new(OrgId::from(org_id), UserId::from(user_id))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `find_by_runtime_kind` must return only agents whose `runtime_kind`
/// matches the requested kind and must cover each known variant.
#[sqlx::test(migrations = "../db/migrations")]
async fn finds_only_matching_kind(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let scope = make_scope(org_id, user_id);
    let repo = AgentRepository::new(pool.clone());

    // Create a container agent.
    let _ = repo
        .create_aggregate(
            &scope,
            NewAgent::container(
                &scope,
                CliToolKind::Codex,
                Some("container-agent"),
                None,
                None,
                workspace_id,
                None,
                None,
            )
            .expect("build container NewAgent"),
        )
        .await
        .expect("create container agent");

    // Create an API agent.
    let _ = repo
        .create_aggregate(
            &scope,
            NewAgent::api(&scope, "anthropic", "claude-sonnet-4-6", Some("api-agent"), None, workspace_id, None)
                .expect("build api NewAgent"),
        )
        .await
        .expect("create api agent");

    // Create a host-cli agent.
    let _ = repo
        .create_aggregate(
            &scope,
            NewAgent::host_cli(
                &scope,
                CliToolKind::Claude,
                HostCliIdentity::generate(),
                Some("host-cli-agent"),
                None,
                None,
                workspace_id,
                None,
            )
            .expect("build host-cli NewAgent"),
        )
        .await
        .expect("create host-cli agent");

    let svc = AgentQueryService::from_pool(pool);

    let containers = svc.find_by_runtime_kind(&scope, RuntimeKind::Container, 100, 0).await.expect("find containers");
    let apis = svc.find_by_runtime_kind(&scope, RuntimeKind::Api, 100, 0).await.expect("find api agents");
    let clis = svc.find_by_runtime_kind(&scope, RuntimeKind::Cli, 100, 0).await.expect("find cli agents");

    assert!(!containers.is_empty(), "expected at least one container agent");
    assert!(!apis.is_empty(), "expected at least one api agent");
    assert!(!clis.is_empty(), "expected at least one cli agent");

    assert!(
        containers.iter().all(|a| a.runtime_kind == RuntimeKind::Container),
        "all container results must have RuntimeKind::Container"
    );
    assert!(apis.iter().all(|a| a.runtime_kind == RuntimeKind::Api), "all api results must have RuntimeKind::Api");
    assert!(clis.iter().all(|a| a.runtime_kind == RuntimeKind::Cli), "all cli results must have RuntimeKind::Cli");
}

/// Results must be tenant-scoped: agents from a different organization must
/// not appear in the query results.
#[sqlx::test(migrations = "../db/migrations")]
async fn results_are_tenant_scoped(pool: PgPool) {
    // Org A — the querying tenant.
    let (org_a, ws_a, user_a) = seed_org_workspace_user(&pool).await;
    let scope_a = make_scope(org_a, user_a);

    // Org B — a different tenant.
    let (org_b, ws_b, user_b) = seed_org_workspace_user(&pool).await;
    let scope_b = make_scope(org_b, user_b);

    let repo = AgentRepository::new(pool.clone());

    // Insert one container agent in each org.
    let _ = repo
        .create_aggregate(
            &scope_a,
            NewAgent::container(&scope_a, CliToolKind::Claude, Some("org-a-agent"), None, None, ws_a, None, None)
                .expect("build NewAgent for org A"),
        )
        .await
        .expect("create org-A agent");

    let _ = repo
        .create_aggregate(
            &scope_b,
            NewAgent::container(&scope_b, CliToolKind::Claude, Some("org-b-agent"), None, None, ws_b, None, None)
                .expect("build NewAgent for org B"),
        )
        .await
        .expect("create org-B agent");

    let svc = AgentQueryService::from_pool(pool);

    let result_a = svc.find_by_runtime_kind(&scope_a, RuntimeKind::Container, 100, 0).await.expect("query org A");

    // Org A's results must not include any agent belonging to org B.
    assert!(result_a.iter().all(|a| a.organization_id == org_a), "org-A query must not return org-B agents");
    assert!(!result_a.is_empty(), "org-A must see its own container agent");
}

/// When there are no agents of the requested kind the service must return an
/// empty vec (not an error).
#[sqlx::test(migrations = "../db/migrations")]
async fn returns_empty_vec_when_no_match(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let scope = make_scope(org_id, user_id);
    let repo = AgentRepository::new(pool.clone());

    // Insert only a container agent — no api agents exist.
    let _ = repo
        .create_aggregate(
            &scope,
            NewAgent::container(
                &scope,
                CliToolKind::Codex,
                Some("only-container"),
                None,
                None,
                workspace_id,
                None,
                None,
            )
            .expect("build container NewAgent"),
        )
        .await
        .expect("create container agent");

    let svc = AgentQueryService::from_pool(pool);
    let apis = svc.find_by_runtime_kind(&scope, RuntimeKind::Api, 100, 0).await.expect("find api agents");

    assert!(apis.is_empty(), "expected empty result when no api agents exist");
}
