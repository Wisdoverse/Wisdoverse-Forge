//! Integration tests verifying uniform 403 for non-owner intra-org callers on
//! lifecycle routes (restart, resume, delete).
//!
//! The owner check fires BEFORE the runtime-kind typestate check so that
//! non-owner callers never receive a 422 that could disclose the agent's
//! runtime kind.
//!
//! Design note: the check lives in the service layer (not the HTTP route
//! handler) so it can be exercised here without the full axum test harness.

use agentforge_api::{
    domain::agent::{AgentOwnerPolicy, HostCliIdentity, NewAgent},
    repositories::agent::AgentRepository,
    services::{agent::AgentService, agent_container_lifecycle::AgentContainerLifecycleService},
};
use agentforge_core::{AgentId, CliToolKind, ErrorKind, OrgId, TenantScope, UserId};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers (same pattern as lifecycle_rejection_routes.rs)
// ---------------------------------------------------------------------------

/// Seed a minimal organization + workspace + user triple.
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

    // workspace id == org_id (project-wide convention for tests)
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

/// Seed an additional user that belongs to the same org (intra-org non-owner).
async fn seed_extra_user(pool: &PgPool, _org_id: Uuid) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("extra-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed extra user");
    user_id
}

fn make_scope(org_id: Uuid, user_id: Uuid) -> TenantScope {
    TenantScope::new(OrgId::from(org_id), UserId::from(user_id))
}

// ---------------------------------------------------------------------------
// Unit-level tests for AgentOwnerPolicy (no DB needed)
// ---------------------------------------------------------------------------

#[test]
fn owner_policy_allows_same_user() {
    let user_id = Uuid::new_v4();
    assert!(AgentOwnerPolicy::require_owner(user_id, user_id).is_ok());
}

#[test]
fn owner_policy_denies_different_user() {
    let owner = Uuid::new_v4();
    let caller = Uuid::new_v4();
    let err = AgentOwnerPolicy::require_owner(caller, owner).expect_err("must be forbidden");
    assert!(
        matches!(err.kind, ErrorKind::Forbidden(_) | ErrorKind::ForbiddenWithCode { .. }),
        "expected Forbidden, got: {:?}",
        err.kind
    );
    let msg = format!("{}", err.kind);
    assert!(msg.contains("operation not permitted"), "message must say 'operation not permitted', got: {msg}");
}

#[test]
fn owner_policy_denied_error_does_not_mention_runtime_kind() {
    let owner = Uuid::new_v4();
    let caller = Uuid::new_v4();
    let err = AgentOwnerPolicy::require_owner(caller, owner).expect_err("must be forbidden");
    let msg = format!("{}", err);
    assert!(!msg.contains("Host CLI"), "error must NOT mention 'Host CLI': {msg}");
    assert!(!msg.contains("container"), "error must NOT mention 'container': {msg}");
    assert!(!msg.contains("API"), "error must NOT mention 'API': {msg}");
}

// ---------------------------------------------------------------------------
// Integration tests — restart rejects non-owner before typestate check
// ---------------------------------------------------------------------------

/// A non-owner intra-org caller attempting restart on a host_cli agent gets 403,
/// NOT a runtime-kind-disclosing 422.
#[sqlx::test(migrations = "../db/migrations")]
async fn restart_by_non_owner_intra_org_returns_forbidden_before_runtime_kind_check(pool: PgPool) {
    let (org_id, workspace_id, owner_id) = seed_org_workspace_user(&pool).await;
    let other_user_id = seed_extra_user(&pool, org_id).await;

    let owner_scope = make_scope(org_id, owner_id);
    let other_scope = make_scope(org_id, other_user_id);

    let repo = AgentRepository::new(pool.clone());
    let identity = HostCliIdentity::generate();
    let new_agent = NewAgent::host_cli(
        &owner_scope,
        CliToolKind::Codex,
        identity,
        Some("hcli-acl-restart"),
        None,
        None,
        workspace_id,
        None,
    )
    .expect("build host-cli NewAgent");
    let agent_id = repo.create_aggregate(&owner_scope, new_agent).await.expect("create agent");

    // docker = None: we must never reach the docker guard — the owner check fires first.
    let svc = AgentContainerLifecycleService::new(repo, None);
    let err = svc.restart(&other_scope, AgentId::from(agent_id)).await.expect_err("non-owner restart must fail");

    let msg = format!("{}", err);
    assert!(
        matches!(err.kind, ErrorKind::Forbidden(_) | ErrorKind::ForbiddenWithCode { .. }),
        "expected Forbidden(403), got kind: {:?}",
        err.kind
    );
    assert!(msg.contains("operation not permitted"), "body must say 'operation not permitted', got: {msg}");
    // Critically: must NOT disclose the runtime kind.
    assert!(!msg.contains("Host CLI"), "body must NOT disclose runtime kind 'Host CLI', got: {msg}");
}

/// Owner can still call restart (error is docker-unavailable, not 403/422).
#[sqlx::test(migrations = "../db/migrations")]
async fn restart_by_owner_passes_acl_check(pool: PgPool) {
    let (org_id, workspace_id, owner_id) = seed_org_workspace_user(&pool).await;
    let owner_scope = make_scope(org_id, owner_id);

    let repo = AgentRepository::new(pool.clone());
    let identity = HostCliIdentity::generate();
    let new_agent = NewAgent::host_cli(
        &owner_scope,
        CliToolKind::Codex,
        identity,
        Some("hcli-acl-restart-owner"),
        None,
        None,
        workspace_id,
        None,
    )
    .expect("build host-cli NewAgent");
    let agent_id = repo.create_aggregate(&owner_scope, new_agent).await.expect("create agent");

    let svc = AgentContainerLifecycleService::new(repo, None);
    let err = svc.restart(&owner_scope, AgentId::from(agent_id)).await.expect_err("restart must fail for other reason");

    // Owner passes the ACL check. The failure must be runtime-kind (LifecycleRejection),
    // NOT a Forbidden error.
    assert!(
        !matches!(err.kind, ErrorKind::Forbidden(_) | ErrorKind::ForbiddenWithCode { .. }),
        "owner must not get Forbidden; got: {:?}",
        err.kind
    );
}

// ---------------------------------------------------------------------------
// Integration tests — resume rejects non-owner before typestate check
// ---------------------------------------------------------------------------

/// Non-owner intra-org caller attempting resume on a host_cli agent gets 403.
#[sqlx::test(migrations = "../db/migrations")]
async fn resume_by_non_owner_intra_org_returns_forbidden_before_runtime_kind_check(pool: PgPool) {
    let (org_id, workspace_id, owner_id) = seed_org_workspace_user(&pool).await;
    let other_user_id = seed_extra_user(&pool, org_id).await;

    let owner_scope = make_scope(org_id, owner_id);
    let other_scope = make_scope(org_id, other_user_id);

    let repo = AgentRepository::new(pool.clone());
    let identity = HostCliIdentity::generate();
    let new_agent = NewAgent::host_cli(
        &owner_scope,
        CliToolKind::Claude,
        identity,
        Some("hcli-acl-resume"),
        None,
        None,
        workspace_id,
        None,
    )
    .expect("build host-cli NewAgent");
    let agent_id = repo.create_aggregate(&owner_scope, new_agent).await.expect("create agent");

    let svc = AgentContainerLifecycleService::new(repo, None);
    let err = svc.resume(&other_scope, AgentId::from(agent_id)).await.expect_err("non-owner resume must fail");

    let msg = format!("{}", err);
    assert!(
        matches!(err.kind, ErrorKind::Forbidden(_) | ErrorKind::ForbiddenWithCode { .. }),
        "expected Forbidden(403), got kind: {:?}",
        err.kind
    );
    assert!(!msg.contains("Host CLI"), "body must NOT disclose runtime kind 'Host CLI', got: {msg}");
}

// ---------------------------------------------------------------------------
// Integration tests — delete rejects non-owner
// ---------------------------------------------------------------------------

/// Non-owner intra-org caller attempting delete on any agent gets 403.
#[sqlx::test(migrations = "../db/migrations")]
async fn delete_by_non_owner_intra_org_returns_forbidden(pool: PgPool) {
    let (org_id, workspace_id, owner_id) = seed_org_workspace_user(&pool).await;
    let other_user_id = seed_extra_user(&pool, org_id).await;

    let owner_scope = make_scope(org_id, owner_id);
    let other_scope = make_scope(org_id, other_user_id);

    let repo = AgentRepository::new(pool.clone());
    let new_agent = NewAgent::container(
        &owner_scope,
        CliToolKind::Claude,
        Some("container-acl-delete"),
        None,
        None,
        workspace_id,
        None,
        None,
    )
    .expect("build container NewAgent");
    let agent_id = repo.create_aggregate(&owner_scope, new_agent).await.expect("create agent");

    let svc = AgentService::new(repo);
    let err = svc.delete(&other_scope, AgentId::from(agent_id)).await.expect_err("non-owner delete must fail");

    let msg = format!("{}", err);
    assert!(
        matches!(err.kind, ErrorKind::Forbidden(_) | ErrorKind::ForbiddenWithCode { .. }),
        "expected Forbidden(403), got kind: {:?}",
        err.kind
    );
    assert!(msg.contains("operation not permitted"), "body must say 'operation not permitted', got: {msg}");
}

/// Owner can delete their own agent.
#[sqlx::test(migrations = "../db/migrations")]
async fn delete_by_owner_succeeds(pool: PgPool) {
    let (org_id, workspace_id, owner_id) = seed_org_workspace_user(&pool).await;
    let owner_scope = make_scope(org_id, owner_id);

    let repo = AgentRepository::new(pool.clone());
    let new_agent = NewAgent::container(
        &owner_scope,
        CliToolKind::Claude,
        Some("container-acl-delete-owner"),
        None,
        None,
        workspace_id,
        None,
        None,
    )
    .expect("build container NewAgent");
    let agent_id = repo.create_aggregate(&owner_scope, new_agent).await.expect("create agent");

    let svc = AgentService::new(repo);
    svc.delete(&owner_scope, AgentId::from(agent_id)).await.expect("owner delete must succeed");
}

// ---------------------------------------------------------------------------
// Integration tests — cross-org access returns 404 (not 403 or 422)
// ---------------------------------------------------------------------------

/// A caller from a different organization (not just a different user) gets 404
/// when attempting restart, preserving the existing cross-org isolation behavior.
#[sqlx::test(migrations = "../db/migrations")]
async fn restart_cross_org_returns_not_found(pool: PgPool) {
    // Org A owns the agent.
    let (org_a_id, workspace_a_id, owner_a_id) = seed_org_workspace_user(&pool).await;
    // Org B is a completely different org; seed it independently.
    let (org_b_id, _workspace_b_id, user_b_id) = seed_org_workspace_user(&pool).await;

    let _ = (org_b_id, _workspace_b_id); // suppress unused warning

    let scope_a = make_scope(org_a_id, owner_a_id);
    let scope_b = make_scope(org_b_id, user_b_id);

    let repo = AgentRepository::new(pool.clone());
    let new_agent = NewAgent::container(
        &scope_a,
        CliToolKind::Claude,
        Some("container-cross-org"),
        None,
        None,
        workspace_a_id,
        None,
        None,
    )
    .expect("build container NewAgent");
    let agent_id = repo.create_aggregate(&scope_a, new_agent).await.expect("create agent");

    let svc = AgentContainerLifecycleService::new(repo, None);
    let err = svc.restart(&scope_b, AgentId::from(agent_id)).await.expect_err("cross-org restart must fail");

    assert!(
        matches!(err.kind, ErrorKind::NotFound(_)),
        "cross-org access must return NotFound(404), got: {:?}",
        err.kind
    );
}
