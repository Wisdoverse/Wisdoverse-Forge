//! Integration tests for in-org agent authorization (#887).
//!
//! Mere org membership is NOT enough to mutate another user's agent. The
//! documented owner/edit/admin access model must be ENFORCED in the service,
//! not just projected to the client.
//!
//! - F011: `update` / `update_status` require `edit` access (owner or
//!   edit/admin collaborator). A bare member or a `view` collaborator is 403.
//! - F012: `add/update/remove_collaborator` require `admin` access, so an
//!   `edit` collaborator (or a bare member) cannot self-escalate by granting
//!   themselves a higher permission on someone else's agent.
//!
//! The check lives in the service layer so it is exercised here without the
//! full axum HTTP harness.

use agentforge_api::{domain::agent::NewAgent, repositories::agent::AgentRepository, services::agent::AgentService};
use agentforge_core::{AgentId, AgentStatus, CliToolKind, ErrorKind, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers (same pattern as lifecycle_owner_acl.rs)
// ---------------------------------------------------------------------------

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

    (org_id, org_id, user_id)
}

/// Seed an intra-org user (a member of `org_id`). Collaborators must be org
/// members (#887/F013), so every extra user joins the org.
async fn seed_extra_user(pool: &PgPool, org_id: Uuid) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("extra-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed extra user");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed extra membership");
    user_id
}

fn make_scope(org_id: Uuid, user_id: Uuid) -> TenantScope {
    agentforge_api::test_support::tenant_scope_for_ids(org_id, user_id)
}

async fn seed_container_agent(
    repo: &AgentRepository,
    owner_scope: &TenantScope,
    workspace_id: Uuid,
    name: &str,
) -> AgentId {
    let new_agent =
        NewAgent::container(owner_scope, CliToolKind::Claude, Some(name), None, None, workspace_id, None, None)
            .expect("build container NewAgent");
    let agent_id = repo.create_aggregate(owner_scope, new_agent).await.expect("create agent");
    AgentId::from(agent_id)
}

fn assert_forbidden(err: &agentforge_core::AppError, ctx: &str) {
    assert!(
        matches!(err.kind, ErrorKind::Forbidden(_) | ErrorKind::ForbiddenWithCode { .. }),
        "{ctx}: expected Forbidden(403), got: {:?}",
        err.kind
    );
    let msg = format!("{}", err.kind);
    assert!(msg.contains("operation not permitted"), "{ctx}: body must say 'operation not permitted', got: {msg}");
}

// ---------------------------------------------------------------------------
// F012 — add_collaborator requires admin (anti self-escalation IDOR)
// ---------------------------------------------------------------------------

/// A bare org member (no collaborator row) cannot add collaborators.
#[sqlx::test(migrations = "../db/migrations")]
async fn add_collaborator_by_bare_member_returns_forbidden(pool: PgPool) {
    let (org_id, workspace_id, owner_id) = seed_org_workspace_user(&pool).await;
    let attacker_id = seed_extra_user(&pool, org_id).await;
    let victim_id = seed_extra_user(&pool, org_id).await;

    let repo = AgentRepository::new(pool.clone());
    let owner_scope = make_scope(org_id, owner_id);
    let agent_id = seed_container_agent(&repo, &owner_scope, workspace_id, "authz-add-member").await;

    let svc = AgentService::new(repo);
    let attacker_scope = make_scope(org_id, attacker_id);
    let err = svc
        .add_collaborator(&attacker_scope, agent_id, victim_id, "admin")
        .await
        .expect_err("bare member must not add collaborators");
    assert_forbidden(&err, "add_collaborator/bare-member");
}

/// An `edit` collaborator cannot escalate by granting access (admin-only op).
#[sqlx::test(migrations = "../db/migrations")]
async fn add_collaborator_by_edit_collaborator_returns_forbidden(pool: PgPool) {
    let (org_id, workspace_id, owner_id) = seed_org_workspace_user(&pool).await;
    let editor_id = seed_extra_user(&pool, org_id).await;
    let target_id = seed_extra_user(&pool, org_id).await;

    let repo = AgentRepository::new(pool.clone());
    let owner_scope = make_scope(org_id, owner_id);
    let agent_id = seed_container_agent(&repo, &owner_scope, workspace_id, "authz-add-editor").await;

    let svc = AgentService::new(repo);
    // Owner seeds the editor (authorized path).
    svc.add_collaborator(&owner_scope, agent_id, editor_id, "edit").await.expect("owner seeds editor");

    let editor_scope = make_scope(org_id, editor_id);
    let err = svc
        .add_collaborator(&editor_scope, agent_id, target_id, "admin")
        .await
        .expect_err("edit collaborator must not grant access");
    assert_forbidden(&err, "add_collaborator/edit-collaborator");
}

/// The owner can add collaborators.
#[sqlx::test(migrations = "../db/migrations")]
async fn add_collaborator_by_owner_succeeds(pool: PgPool) {
    let (org_id, workspace_id, owner_id) = seed_org_workspace_user(&pool).await;
    let collab_id = seed_extra_user(&pool, org_id).await;

    let repo = AgentRepository::new(pool.clone());
    let owner_scope = make_scope(org_id, owner_id);
    let agent_id = seed_container_agent(&repo, &owner_scope, workspace_id, "authz-add-owner").await;

    let svc = AgentService::new(repo);
    svc.add_collaborator(&owner_scope, agent_id, collab_id, "edit").await.expect("owner add must succeed");
}

/// An `admin` collaborator can grant access to others.
#[sqlx::test(migrations = "../db/migrations")]
async fn add_collaborator_by_admin_collaborator_succeeds(pool: PgPool) {
    let (org_id, workspace_id, owner_id) = seed_org_workspace_user(&pool).await;
    let admin_id = seed_extra_user(&pool, org_id).await;
    let target_id = seed_extra_user(&pool, org_id).await;

    let repo = AgentRepository::new(pool.clone());
    let owner_scope = make_scope(org_id, owner_id);
    let agent_id = seed_container_agent(&repo, &owner_scope, workspace_id, "authz-add-admin").await;

    let svc = AgentService::new(repo);
    svc.add_collaborator(&owner_scope, agent_id, admin_id, "admin").await.expect("owner seeds admin");

    let admin_scope = make_scope(org_id, admin_id);
    svc.add_collaborator(&admin_scope, agent_id, target_id, "view").await.expect("admin collaborator add must succeed");
}

// ---------------------------------------------------------------------------
// F011 — update / update_status require edit access
// ---------------------------------------------------------------------------

/// A bare org member cannot update another user's agent.
#[sqlx::test(migrations = "../db/migrations")]
async fn update_by_bare_member_returns_forbidden(pool: PgPool) {
    let (org_id, workspace_id, owner_id) = seed_org_workspace_user(&pool).await;
    let attacker_id = seed_extra_user(&pool, org_id).await;

    let repo = AgentRepository::new(pool.clone());
    let owner_scope = make_scope(org_id, owner_id);
    let agent_id = seed_container_agent(&repo, &owner_scope, workspace_id, "authz-update-member").await;

    let svc = AgentService::new(repo);
    let attacker_scope = make_scope(org_id, attacker_id);
    let err = svc
        .update(&attacker_scope, agent_id, Some("hijacked"), None, None, None)
        .await
        .expect_err("bare member must not update");
    assert_forbidden(&err, "update/bare-member");
}

/// A `view` collaborator cannot update (read-only).
#[sqlx::test(migrations = "../db/migrations")]
async fn update_by_view_collaborator_returns_forbidden(pool: PgPool) {
    let (org_id, workspace_id, owner_id) = seed_org_workspace_user(&pool).await;
    let viewer_id = seed_extra_user(&pool, org_id).await;

    let repo = AgentRepository::new(pool.clone());
    let owner_scope = make_scope(org_id, owner_id);
    let agent_id = seed_container_agent(&repo, &owner_scope, workspace_id, "authz-update-view").await;

    let svc = AgentService::new(repo);
    svc.add_collaborator(&owner_scope, agent_id, viewer_id, "view").await.expect("owner seeds viewer");

    let viewer_scope = make_scope(org_id, viewer_id);
    let err = svc
        .update(&viewer_scope, agent_id, Some("hijacked"), None, None, None)
        .await
        .expect_err("view collaborator must not update");
    assert_forbidden(&err, "update/view-collaborator");
}

/// An `edit` collaborator can update.
#[sqlx::test(migrations = "../db/migrations")]
async fn update_by_edit_collaborator_succeeds(pool: PgPool) {
    let (org_id, workspace_id, owner_id) = seed_org_workspace_user(&pool).await;
    let editor_id = seed_extra_user(&pool, org_id).await;

    let repo = AgentRepository::new(pool.clone());
    let owner_scope = make_scope(org_id, owner_id);
    let agent_id = seed_container_agent(&repo, &owner_scope, workspace_id, "authz-update-edit").await;

    let svc = AgentService::new(repo);
    svc.add_collaborator(&owner_scope, agent_id, editor_id, "edit").await.expect("owner seeds editor");

    let editor_scope = make_scope(org_id, editor_id);
    let updated = svc
        .update(&editor_scope, agent_id, Some("renamed-by-editor"), None, None, None)
        .await
        .expect("edit collaborator update must succeed");
    assert_eq!(updated.name.as_deref(), Some("renamed-by-editor"));
}

/// A bare org member cannot change another user's agent status.
#[sqlx::test(migrations = "../db/migrations")]
async fn update_status_by_bare_member_returns_forbidden(pool: PgPool) {
    let (org_id, workspace_id, owner_id) = seed_org_workspace_user(&pool).await;
    let attacker_id = seed_extra_user(&pool, org_id).await;

    let repo = AgentRepository::new(pool.clone());
    let owner_scope = make_scope(org_id, owner_id);
    let agent_id = seed_container_agent(&repo, &owner_scope, workspace_id, "authz-status-member").await;

    let svc = AgentService::new(repo);
    let attacker_scope = make_scope(org_id, attacker_id);
    let err = svc
        .update_status(&attacker_scope, agent_id, AgentStatus::Working)
        .await
        .expect_err("bare member must not change status");
    assert_forbidden(&err, "update_status/bare-member");
}
