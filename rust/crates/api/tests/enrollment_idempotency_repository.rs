//! Integration tests for EnrollmentIdempotencyRepository:
//!  1. lookup returns None for an unknown (org, user, key) triple.
//!  2. store_in_tx + lookup round-trips correctly.
//!  3. A duplicate store_in_tx with the same key is a no-op (ON CONFLICT DO NOTHING).
//!
//! Tests run against a fresh database with all migrations applied via
//! `#[sqlx::test(migrations = "../db/migrations")]`.

use agentforge_api::repositories::enrollment_idempotency::EnrollmentIdempotencyRepository;
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
// Tests
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn lookup_returns_none_for_unknown_key(pool: PgPool) {
    let repo = EnrollmentIdempotencyRepository::new(pool);
    let r = repo
        .lookup(Uuid::new_v4(), Uuid::new_v4(), "missing")
        .await
        .unwrap();
    assert!(r.is_none());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn store_and_lookup_roundtrip(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, status, runtime_kind, cli_tool)
         VALUES ($1, $2, $3, $4, 'offline', 'cli', 'codex')",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    let repo = EnrollmentIdempotencyRepository::new(pool.clone());
    let mut tx = pool.begin().await.unwrap();
    EnrollmentIdempotencyRepository::store_in_tx(&mut tx, org_id, user_id, "key-1", agent_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let got = repo.lookup(org_id, user_id, "key-1").await.unwrap();
    assert_eq!(got, Some(agent_id));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn duplicate_store_is_idempotent(pool: PgPool) {
    let (org_id, workspace_id, user_id) = seed_org_workspace_user(&pool).await;
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, status, runtime_kind, cli_tool)
         VALUES ($1, $2, $3, $4, 'offline', 'cli', 'codex')",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(workspace_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    EnrollmentIdempotencyRepository::store_in_tx(&mut tx, org_id, user_id, "k", agent_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // Second store with same key is a no-op (ON CONFLICT DO NOTHING).
    let mut tx2 = pool.begin().await.unwrap();
    EnrollmentIdempotencyRepository::store_in_tx(&mut tx2, org_id, user_id, "k", agent_id)
        .await
        .unwrap();
    tx2.commit().await.unwrap();
}
