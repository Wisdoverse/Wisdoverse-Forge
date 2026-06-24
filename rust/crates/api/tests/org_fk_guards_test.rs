//! Tenant-isolation tests for org-scoped FK guards (#887 F013/F017).
//!
//! A member of one org must not be able to create rows that reference another
//! org's agent or a foreign-org user. The referenced agent/user exists globally
//! (so the raw FK is satisfied), but it belongs to a different org — the guard
//! enforces the org boundary as a write-time invariant.
//!
//! - F017: event ingest with a foreign-org agent_id is rejected (NotFound).
//! - F013b: participant register with a foreign-org agent_id is rejected.
//! - F013a: adding a foreign-org user as a collaborator is rejected.

use agentforge_api::domain::agent::NewAgent;
use agentforge_api::repositories::agent::{AgentRepository, EventRepository};
use agentforge_api::repositories::orchestration::ParticipantRepository;
use agentforge_core::{AgentId, CliToolKind, ErrorKind, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

async fn seed_org(pool: &PgPool) -> Uuid {
    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");
    org_id
}

/// Seed a user that is a member of `org_id`.
async fn seed_member(pool: &PgPool, org_id: Uuid) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("u-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed membership");
    user_id
}

fn scope(org: Uuid, user: Uuid) -> TenantScope {
    agentforge_api::test_support::tenant_scope_for_ids(org, user)
}

async fn seed_agent(repo: &AgentRepository, owner_scope: &TenantScope, workspace_id: Uuid, name: &str) -> AgentId {
    let new_agent =
        NewAgent::container(owner_scope, CliToolKind::Claude, Some(name), None, None, workspace_id, None, None)
            .expect("build NewAgent");
    AgentId::from(repo.create_aggregate(owner_scope, new_agent).await.expect("create agent"))
}

// ---------------------------------------------------------------------------
// F017 — event ingest
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn event_ingest_rejects_foreign_org_agent(pool: PgPool) {
    let org_a = seed_org(&pool).await;
    let owner_a = seed_member(&pool, org_a).await;
    let org_b = seed_org(&pool).await;
    let owner_b = seed_member(&pool, org_b).await;

    let repo = AgentRepository::new(pool.clone());
    let agent_b = seed_agent(&repo, &scope(org_b, owner_b), org_b, "agent-b").await;

    let events = EventRepository::new(pool.clone());
    let err = events
        .insert(&scope(org_a, owner_a), agent_b, "session_start", serde_json::json!({}), None)
        .await
        .expect_err("foreign-org agent event must be rejected");
    assert!(matches!(err.kind, ErrorKind::NotFound(_)), "got: {:?}", err.kind);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn event_ingest_accepts_own_agent(pool: PgPool) {
    let org_a = seed_org(&pool).await;
    let owner_a = seed_member(&pool, org_a).await;
    let repo = AgentRepository::new(pool.clone());
    let scope_a = scope(org_a, owner_a);
    let agent_a = seed_agent(&repo, &scope_a, org_a, "agent-a").await;

    let events = EventRepository::new(pool.clone());
    events.insert(&scope_a, agent_a, "session_start", serde_json::json!({}), None).await.expect("own agent event ok");
}

// ---------------------------------------------------------------------------
// F013b — participant register
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn participant_register_rejects_foreign_org_agent(pool: PgPool) {
    let org_a = seed_org(&pool).await;
    let owner_a = seed_member(&pool, org_a).await;
    let org_b = seed_org(&pool).await;
    let owner_b = seed_member(&pool, org_b).await;

    let repo = AgentRepository::new(pool.clone());
    let agent_b = seed_agent(&repo, &scope(org_b, owner_b), org_b, "agent-b").await;

    let participants = ParticipantRepository::new(pool.clone());
    let err = participants
        .register(&scope(org_a, owner_a), agent_b, "p", &[])
        .await
        .expect_err("foreign-org agent participant must be rejected");
    assert!(matches!(err.kind, ErrorKind::NotFound(_)), "got: {:?}", err.kind);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn participant_register_accepts_own_agent(pool: PgPool) {
    let org_a = seed_org(&pool).await;
    let owner_a = seed_member(&pool, org_a).await;
    let repo = AgentRepository::new(pool.clone());
    let scope_a = scope(org_a, owner_a);
    let agent_a = seed_agent(&repo, &scope_a, org_a, "agent-a").await;

    let participants = ParticipantRepository::new(pool.clone());
    participants.register(&scope_a, agent_a, "p", &["chat".to_string()]).await.expect("own agent register ok");
}

// ---------------------------------------------------------------------------
// F013a — collaborator membership
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn add_collaborator_rejects_foreign_org_user(pool: PgPool) {
    let org_a = seed_org(&pool).await;
    let owner_a = seed_member(&pool, org_a).await;
    let org_b = seed_org(&pool).await;
    let foreign_user = seed_member(&pool, org_b).await; // member of B, NOT A

    let repo = AgentRepository::new(pool.clone());
    let scope_a = scope(org_a, owner_a);
    let agent_a = seed_agent(&repo, &scope_a, org_a, "agent-a").await;

    let err = repo
        .add_collaborator(&scope_a, agent_a, foreign_user, "view")
        .await
        .expect_err("foreign-org user must not become a collaborator");
    assert!(matches!(err.kind, ErrorKind::NotFound(_)), "got: {:?}", err.kind);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn add_collaborator_accepts_org_member(pool: PgPool) {
    let org_a = seed_org(&pool).await;
    let owner_a = seed_member(&pool, org_a).await;
    let local_member = seed_member(&pool, org_a).await;

    let repo = AgentRepository::new(pool.clone());
    let scope_a = scope(org_a, owner_a);
    let agent_a = seed_agent(&repo, &scope_a, org_a, "agent-a").await;

    let collab = repo.add_collaborator(&scope_a, agent_a, local_member, "view").await.expect("org member add ok");
    assert_eq!(collab.user_id.as_uuid(), local_member);
}
