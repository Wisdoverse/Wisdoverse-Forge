//! Integration tests verifying that container lifecycle services reject
//! `host_cli` and `api` agents with typed 422-style `LifecycleRejection`
//! messages before any Docker I/O takes place.
//!
//! Tests run against a fresh database with all migrations applied via
//! `#[sqlx::test(migrations = "../db/migrations")]`.
//!
//! The typestate check in `ContainerAgent::try_from` fires before docker is
//! consulted, so `docker = None` is intentional — the rejection path never
//! reaches the Docker guard.

use agentforge_api::{
    domain::agent::{HostCliIdentity, NewAgent},
    repositories::agent::AgentRepository,
    services::agent_container_lifecycle::AgentContainerLifecycleService,
};
use agentforge_core::{AgentId, CliToolKind, OrgId, TenantScope, UserId};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helper — same pattern as other integration tests in this crate.
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
// Tests — restart rejects host_cli
// ---------------------------------------------------------------------------

/// `restart` on a Host CLI agent must return an error containing "Host CLI".
///
/// The typestate check fires before the Docker guard, so `docker = None` is
/// intentional — the rejection path never reaches the Docker availability check.
#[sqlx::test(migrations = "../db/migrations")]
async fn restart_on_host_cli_returns_error_with_host_cli_message(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let scope = make_scope(org_id, user_id);
    let repo = AgentRepository::new(pool.clone());

    let identity = HostCliIdentity::generate();
    let new_agent =
        NewAgent::host_cli(&scope, CliToolKind::Codex, identity, Some("hcli-restart"), None, None, workspace_id, None)
            .expect("build host-cli NewAgent");

    let id = repo.create_aggregate(&scope, new_agent).await.expect("create host-cli agent");

    // docker = None: the typestate check fires before the docker guard.
    let svc = AgentContainerLifecycleService::new(repo, None);
    let res = svc.restart(&scope, AgentId::from(id)).await;

    let err = res.expect_err("restart on host_cli must be rejected");
    let msg = format!("{err}");
    assert!(msg.contains("Host CLI"), "error message must mention 'Host CLI', got: {msg}");
}

// ---------------------------------------------------------------------------
// Tests — restart rejects api
// ---------------------------------------------------------------------------

/// `restart` on an API/provider agent must return an error containing "API".
#[sqlx::test(migrations = "../db/migrations")]
async fn restart_on_api_agent_returns_error_with_api_message(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let scope = make_scope(org_id, user_id);
    let repo = AgentRepository::new(pool.clone());

    let new_agent =
        NewAgent::api(&scope, "anthropic", "claude-sonnet-4-6", Some("api-restart"), None, workspace_id, None)
            .expect("build api NewAgent");

    let id = repo.create_aggregate(&scope, new_agent).await.expect("create api agent");

    let svc = AgentContainerLifecycleService::new(repo, None);
    let res = svc.restart(&scope, AgentId::from(id)).await;

    let err = res.expect_err("restart on api agent must be rejected");
    let msg = format!("{err}");
    assert!(msg.contains("API"), "error message must mention 'API', got: {msg}");
}

// ---------------------------------------------------------------------------
// Tests — resume rejects host_cli
// ---------------------------------------------------------------------------

/// `resume` on a Host CLI agent must return an error containing "Host CLI".
#[sqlx::test(migrations = "../db/migrations")]
async fn resume_on_host_cli_returns_error_with_host_cli_message(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let scope = make_scope(org_id, user_id);
    let repo = AgentRepository::new(pool.clone());

    let identity = HostCliIdentity::generate();
    let new_agent =
        NewAgent::host_cli(&scope, CliToolKind::Claude, identity, Some("hcli-resume"), None, None, workspace_id, None)
            .expect("build host-cli NewAgent");

    let id = repo.create_aggregate(&scope, new_agent).await.expect("create host-cli agent");

    let svc = AgentContainerLifecycleService::new(repo, None);
    let res = svc.resume(&scope, AgentId::from(id)).await;

    let err = res.expect_err("resume on host_cli must be rejected");
    let msg = format!("{err}");
    assert!(msg.contains("Host CLI"), "error message must mention 'Host CLI', got: {msg}");
}

// ---------------------------------------------------------------------------
// Tests — resume rejects api
// ---------------------------------------------------------------------------

/// `resume` on an API/provider agent must return an error containing "API".
#[sqlx::test(migrations = "../db/migrations")]
async fn resume_on_api_agent_returns_error_with_api_message(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let scope = make_scope(org_id, user_id);
    let repo = AgentRepository::new(pool.clone());

    let new_agent =
        NewAgent::api(&scope, "anthropic", "claude-sonnet-4-6", Some("api-resume"), None, workspace_id, None)
            .expect("build api NewAgent");

    let id = repo.create_aggregate(&scope, new_agent).await.expect("create api agent");

    let svc = AgentContainerLifecycleService::new(repo, None);
    let res = svc.resume(&scope, AgentId::from(id)).await;

    let err = res.expect_err("resume on api agent must be rejected");
    let msg = format!("{err}");
    assert!(msg.contains("API"), "error message must mention 'API', got: {msg}");
}

// ---------------------------------------------------------------------------
// Tests — container agents pass the typestate check
// ---------------------------------------------------------------------------

/// A container agent must pass the typestate check (typestate does not reject
/// it). The operation will fail later because the agent has no container_id,
/// but the error must NOT mention "Host CLI" or "API".
#[sqlx::test(migrations = "../db/migrations")]
async fn restart_on_container_agent_passes_typestate_check(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let scope = make_scope(org_id, user_id);
    let repo = AgentRepository::new(pool.clone());

    let new_agent = NewAgent::container(
        &scope,
        CliToolKind::Claude,
        Some("container-restart"),
        None,
        None,
        workspace_id,
        None,
        None,
    )
    .expect("build container NewAgent");

    let id = repo.create_aggregate(&scope, new_agent).await.expect("create container agent");

    // Docker = None triggers lifecycle_docker_unavailable AFTER typestate passes.
    let svc = AgentContainerLifecycleService::new(repo, None);
    let res = svc.restart(&scope, AgentId::from(id)).await;

    // Must fail — but the failure must be the docker-unavailable error, not a
    // LifecycleRejection.
    let err = res.expect_err("restart without docker must fail");
    let msg = format!("{err}");
    assert!(!msg.contains("Host CLI"), "container agent must not produce a Host CLI rejection, got: {msg}");
    assert!(!msg.contains("API/provider"), "container agent must not produce an API rejection, got: {msg}");
}
