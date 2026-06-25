//! Schema-contract test for migrations 075/076 (#895 DB schema hardening).
//!
//! Asserts a fresh database carries the corrective integrity constraints and
//! indexes the hardening batch adds:
//!   - F048: a CHECK on `orchestration_tasks.review_status` pinning the canonical
//!     self-fix vocabulary (behavioural + structural).
//!   - F049: FKs from `enrollment_idempotency`/`agent_join_codes` tenant columns
//!     to `organizations(id)`/`users(id)` (structural).
//!   - F050: per-consumer partial composite indexes on `orchestration_outbox`.
//!
//! These fail loudly on a fresh DB if a later migration drops/weakens any of
//! them, instead of surfacing as a silent integrity hole.
//!
//! ```text
//! cargo test -p agentforge-db --test schema_hardening_contract
//! ```

use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Postgres SQLSTATE for `check_violation`.
const SQLSTATE_CHECK_VIOLATION: &str = "23514";

async fn constraint_exists(pool: &PgPool, name: &str) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = $1)")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("query pg_constraint")
}

async fn index_exists(pool: &PgPool, name: &str) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = $1)")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("query pg_indexes")
}

async fn seed_org_user(pool: &PgPool) -> (Uuid, Uuid) {
    let org = Uuid::new_v4();
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org)
        .bind(format!("Org {org}"))
        .bind(format!("org-{org}"))
        .execute(pool)
        .await
        .expect("seed org");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user)
        .bind(format!("u-{user}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");
    (org, user)
}

/// F048: the CHECK accepts every canonical value (and NULL) and rejects anything
/// else, so a typo'd review state cannot be persisted.
#[sqlx::test(migrations = "./migrations")]
async fn review_status_check_accepts_canonical_and_rejects_unknown(pool: PgPool) {
    let (org, user) = seed_org_user(&pool).await;
    let task: Uuid = sqlx::query_scalar(
        "INSERT INTO orchestration_tasks (organization_id, title, created_by) VALUES ($1,$2,$3) RETURNING id",
    )
    .bind(org)
    .bind("hardening task")
    .bind(user)
    .fetch_one(&pool)
    .await
    .expect("seed task");

    for value in ["pending", "in_review", "approved", "changes_requested", "rejected", "merged", "sensitive_blocked"] {
        sqlx::query("UPDATE orchestration_tasks SET review_status = $1 WHERE id = $2")
            .bind(value)
            .bind(task)
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("canonical value {value} must be accepted: {e}"));
    }
    // NULL is the resting state for non-self-fix tasks.
    sqlx::query("UPDATE orchestration_tasks SET review_status = NULL WHERE id = $1")
        .bind(task)
        .execute(&pool)
        .await
        .expect("NULL review_status must be accepted");

    // A non-canonical value (typo) must be rejected at the DB boundary.
    let err = sqlx::query("UPDATE orchestration_tasks SET review_status = 'aproved' WHERE id = $1")
        .bind(task)
        .execute(&pool)
        .await
        .expect_err("a non-canonical review_status must violate the CHECK");
    let code = err.as_database_error().and_then(|e| e.code()).map(|c| c.into_owned());
    assert_eq!(code.as_deref(), Some(SQLSTATE_CHECK_VIOLATION), "expected check_violation, got {err:?}");
}

/// F048/F049/F050: the corrective constraints and indexes all exist on a fresh
/// migrated database. Fails before 075/076, passes after.
#[sqlx::test(migrations = "./migrations")]
async fn hardening_constraints_and_indexes_present(pool: PgPool) {
    // F048
    assert!(
        constraint_exists(&pool, "orchestration_tasks_review_status_check").await,
        "review_status CHECK must exist"
    );
    // F049
    assert!(constraint_exists(&pool, "enrollment_idempotency_org_id_fkey").await, "enrollment org FK must exist");
    assert!(constraint_exists(&pool, "enrollment_idempotency_user_id_fkey").await, "enrollment user FK must exist");
    assert!(constraint_exists(&pool, "agent_join_codes_organization_id_fkey").await, "join-code org FK must exist");
    // F050
    assert!(
        index_exists(&pool, "idx_orchestration_outbox_event_type_unpublished").await,
        "outbox event_type partial index must exist"
    );
    assert!(
        index_exists(&pool, "idx_orchestration_outbox_aggregate_type_unpublished").await,
        "outbox aggregate_type partial index must exist"
    );

    // The FK constraints must actually be VALIDATED (convalidated), not left
    // NOT VALID — otherwise existing/future drift is not enforced on read paths.
    let validated: bool = sqlx::query(
        "SELECT bool_and(convalidated) FROM pg_constraint
         WHERE conname IN ('enrollment_idempotency_org_id_fkey','enrollment_idempotency_user_id_fkey','agent_join_codes_organization_id_fkey')",
    )
    .fetch_one(&pool)
    .await
    .expect("query convalidated")
    .get(0);
    assert!(validated, "all F049 FKs must be VALIDATEd");
}
