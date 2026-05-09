//! Issue #37 follow-up — integration coverage for the `complete_task`
//! transactional boundary.
//!
//! MR !528 transactionalized `set_result_in_tx` + `unblock_children_of_in_tx`
//! but deferred the full rollback proof because the Rust DB harness was
//! thought missing. The `#[sqlx::test]` harness already ships (see
//! `nav_regression_e2e_test.rs`), so this file closes the gap:
//!
//! 1. **Atomic happy path** — both statements commit together; parent goes
//!    `working → completed`, child goes `blocked/waiting_dependency → queued`.
//! 2. **Atomic rollback** — if the caller rolls back mid-tx, NEITHER change
//!    persists. This is the invariant that `complete_task` relies on to
//!    convert "unblock failed" into a safe retry instead of a permanent
//!    orphan of waiting children.
//! 3. **Cross-tenant isolation** — a sibling org's parent+child is not
//!    touched by either the tx or the rollback path.
//!
//! Tests seed against the real SQL (`SET_RESULT_SQL` / `UNBLOCK_CHILDREN_SQL`)
//! via the `_in_tx` helpers, matching exactly what `OrchestrationService::
//! complete_task` runs in production.

use agentforge_api::repositories::orchestration::OrchestrationTaskRepository;
use agentforge_api::test_support::tenant_scope_for_ids;
use agentforge_core::TenantScope;
use sqlx::PgPool;
use uuid::Uuid;

/// Seed one org + user + agent. The agent is required because
/// `orchestration_tasks.assigned_agent_id` FKs to `agents.id`.
async fn seed_org_with_agent(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

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
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("u-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");
    sqlx::query(
        "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status) VALUES ($1, $2, $2, $3, 'test-agent', 'idle')",
    )
    .bind(agent_id)
    .bind(org_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed agent");
    (org_id, user_id, agent_id)
}

/// Seed a parent task in `working` status assigned to `agent_id`, plus a
/// child task in `blocked/waiting_dependency` on that parent. Returns
/// `(parent_id, child_id)`.
async fn seed_parent_and_child(pool: &PgPool, org_id: Uuid, user_id: Uuid, agent_id: Uuid) -> (Uuid, Uuid) {
    let parent_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, created_by, assigned_agent_id, started_at)
           VALUES ($1, $2, 'parent', 'working', $3, $4, NOW())"#,
    )
    .bind(parent_id)
    .bind(org_id)
    .bind(user_id)
    .bind(agent_id)
    .execute(pool)
    .await
    .expect("seed parent");

    sqlx::query(
        r#"INSERT INTO orchestration_tasks
               (id, organization_id, title, status, created_by, parent_task_id, blocked_reason)
           VALUES ($1, $2, 'child', 'blocked', $3, $4, 'waiting_dependency')"#,
    )
    .bind(child_id)
    .bind(org_id)
    .bind(user_id)
    .bind(parent_id)
    .execute(pool)
    .await
    .expect("seed child");

    (parent_id, child_id)
}

async fn task_status(pool: &PgPool, task_id: Uuid) -> String {
    sqlx::query_scalar::<_, String>("SELECT status FROM orchestration_tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(pool)
        .await
        .expect("query task status")
}

async fn task_blocked_reason(pool: &PgPool, task_id: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT blocked_reason FROM orchestration_tasks WHERE id = $1")
        .bind(task_id)
        .fetch_one(pool)
        .await
        .expect("query blocked_reason")
}

fn scope_for(org_id: Uuid, user_id: Uuid) -> TenantScope {
    tenant_scope_for_ids(org_id, user_id)
}

// ---------------------------------------------------------------------------
// Happy path: tx commit flips parent + child together.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn complete_task_tx_commits_parent_and_children_atomically(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_with_agent(&pool).await;
    let (parent_id, child_id) = seed_parent_and_child(&pool, org_id, user_id, agent_id).await;
    let scope = scope_for(org_id, user_id);

    let mut tx = pool.begin().await.expect("begin tx");
    let updated_parent = OrchestrationTaskRepository::set_result_in_tx(
        &mut tx,
        &scope,
        parent_id,
        "completed",
        serde_json::json!({"stdout": "ok"}),
    )
    .await
    .expect("set_result_in_tx");
    let unblocked = OrchestrationTaskRepository::unblock_children_of_in_tx(&mut tx, &scope, parent_id)
        .await
        .expect("unblock_children_of_in_tx");
    tx.commit().await.expect("commit");

    assert_eq!(updated_parent.status, "completed", "tx return: parent status");
    assert_eq!(unblocked.len(), 1, "expected the single child to be unblocked");
    assert_eq!(unblocked[0].id, child_id, "child id preserved in returning *");

    assert_eq!(task_status(&pool, parent_id).await, "completed", "parent persisted as completed");
    assert_eq!(task_status(&pool, child_id).await, "queued", "child persisted as queued");
    assert_eq!(task_blocked_reason(&pool, child_id).await, None, "child blocked_reason cleared");
}

// ---------------------------------------------------------------------------
// Rollback: if the caller rolls back, NEITHER change persists. This is the
// invariant that converts an unblock failure from "permanent orphan" into
// "safe retry". Without this guarantee, `complete_task`'s transactional
// contract is a lie.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn complete_task_tx_rollback_leaves_parent_and_children_untouched(pool: PgPool) {
    let (org_id, user_id, agent_id) = seed_org_with_agent(&pool).await;
    let (parent_id, child_id) = seed_parent_and_child(&pool, org_id, user_id, agent_id).await;
    let scope = scope_for(org_id, user_id);

    let mut tx = pool.begin().await.expect("begin tx");
    OrchestrationTaskRepository::set_result_in_tx(
        &mut tx,
        &scope,
        parent_id,
        "completed",
        serde_json::json!({"stdout": "ok"}),
    )
    .await
    .expect("set_result_in_tx inside tx");
    OrchestrationTaskRepository::unblock_children_of_in_tx(&mut tx, &scope, parent_id)
        .await
        .expect("unblock_children_of_in_tx inside tx");
    tx.rollback().await.expect("rollback");

    assert_eq!(
        task_status(&pool, parent_id).await,
        "working",
        "rollback must restore parent to working — otherwise complete_task cannot safely retry"
    );
    assert_eq!(task_status(&pool, child_id).await, "blocked", "rollback must leave child blocked");
    assert_eq!(
        task_blocked_reason(&pool, child_id).await.as_deref(),
        Some("waiting_dependency"),
        "rollback must preserve child blocked_reason"
    );
}

// ---------------------------------------------------------------------------
// Cross-tenant: org A's tx must not touch org B's parent+child.
// UNBLOCK_CHILDREN_SQL's tenant guard is the load-bearing invariant here;
// pinning this test means a future "simplification" that drops the
// `organization_id = $1` predicate shows up as a cross-org regression.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn complete_task_tx_does_not_leak_across_tenants(pool: PgPool) {
    let (org_a, user_a, agent_a) = seed_org_with_agent(&pool).await;
    let (parent_a, child_a) = seed_parent_and_child(&pool, org_a, user_a, agent_a).await;

    let (org_b, user_b, agent_b) = seed_org_with_agent(&pool).await;
    let (parent_b, child_b) = seed_parent_and_child(&pool, org_b, user_b, agent_b).await;

    let scope_a = scope_for(org_a, user_a);

    let mut tx = pool.begin().await.expect("begin tx");
    OrchestrationTaskRepository::set_result_in_tx(
        &mut tx,
        &scope_a,
        parent_a,
        "completed",
        serde_json::json!({"ok": true}),
    )
    .await
    .expect("set_result_in_tx");
    let unblocked = OrchestrationTaskRepository::unblock_children_of_in_tx(&mut tx, &scope_a, parent_a)
        .await
        .expect("unblock_children_of_in_tx");
    tx.commit().await.expect("commit");

    assert_eq!(unblocked.len(), 1, "org A unblocked its own one child only");
    assert_eq!(unblocked[0].id, child_a);

    assert_eq!(task_status(&pool, parent_a).await, "completed", "org A parent committed");
    assert_eq!(task_status(&pool, child_a).await, "queued", "org A child unblocked");

    assert_eq!(task_status(&pool, parent_b).await, "working", "org B parent untouched");
    assert_eq!(task_status(&pool, child_b).await, "blocked", "org B child untouched");
    assert_eq!(
        task_blocked_reason(&pool, child_b).await.as_deref(),
        Some("waiting_dependency"),
        "org B child reason preserved"
    );

    // Also probe the reverse: try to set_result on org B's parent with
    // org A's scope. The tenant clause must reject it as NotFound, not
    // cross-tenant write.
    let mut tx = pool.begin().await.expect("begin tx");
    let cross = OrchestrationTaskRepository::set_result_in_tx(
        &mut tx,
        &scope_a,
        parent_b,
        "completed",
        serde_json::json!({"stolen": true}),
    )
    .await;
    tx.rollback().await.expect("rollback");
    assert!(cross.is_err(), "scope A must not be able to terminate org B's task — got {cross:?}");
    assert_eq!(task_status(&pool, parent_b).await, "working", "org B parent still working");
}
