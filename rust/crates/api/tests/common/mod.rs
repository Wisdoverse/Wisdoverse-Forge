//! Shared helpers for API integration tests. Each test file under
//! `rust/crates/api/tests/` that needs these brings them in with
//! `mod common;` at the top.
//!
//! Keep this module small — it exists to eliminate cross-file duplication
//! of the "seed a user + build a throwaway TenantScope" boilerplate. Resist
//! the urge to add test-specific setup here; per-test fixtures belong in the
//! test file itself.

use agentforge_api::test_support::{tenant_scope_for_ids, tenant_scope_for_user};
use agentforge_core::TenantScope;
use sqlx::PgPool;
use uuid::Uuid;

/// Insert a users row with a unique email. Returns the new user_id.
#[allow(dead_code)]
pub async fn seed_user(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(id)
        .bind(format!("test-{id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");
    id
}

/// Build a `TenantScope` for tests. The legacy cli-credential path keys off
/// `user_id` only, so the `org_id` is a throwaway UUID — intentional.
#[allow(dead_code)]
pub fn scope_for(user: Uuid) -> TenantScope {
    tenant_scope_for_user(user)
}

/// Build a `TenantScope` for integration tests from explicit tenant IDs.
#[allow(dead_code)]
pub fn scope_for_ids(org_id: Uuid, user_id: Uuid) -> TenantScope {
    tenant_scope_for_ids(org_id, user_id)
}
