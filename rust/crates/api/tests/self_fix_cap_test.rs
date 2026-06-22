//! Integration tests for the merge-retry cap (issue #800).
//!
//! Covers:
//! 1. `approve_and_merge` refuses with `merge_attempts_exhausted` and persists
//!    `review_status = 'changes_requested'` when `merge_attempts` equals the cap.
//! 2. `bump_merge_attempts` increments the counter (tenant-scoped) and a
//!    cross-org bump is a no-op.
//!
//! The cap check runs BEFORE the GitHub-client requirement, so `github = None`
//! is sufficient: the service short-circuits on the exhaustion guard, never
//! reaching the `github_not_configured` error.

use agentforge_api::repositories::orchestration::{CreateTaskRow, OrchestrationTaskRepository};
use agentforge_api::test_support::{app_state_with_mock_provider, tenant_scope_for_ids};
use agentforge_api::testing::self_fix_review::approve;
use agentforge_core::{ErrorKind, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

/// Seed one org + workspace + user. Tasks are created unassigned.
async fn seed_org(pool: &PgPool) -> (Uuid, Uuid) {
    let org_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

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
    (org_id, user_id)
}

fn scope_for(org_id: Uuid, user_id: Uuid) -> TenantScope {
    tenant_scope_for_ids(org_id, user_id)
}

/// Read back the persisted `(merge_attempts, review_status)` for a task.
async fn task_cap_columns(pool: &PgPool, task_id: Uuid) -> (i32, Option<String>) {
    sqlx::query_as::<_, (i32, Option<String>)>(
        "SELECT merge_attempts, review_status FROM orchestration_tasks WHERE id = $1",
    )
    .bind(task_id)
    .fetch_one(pool)
    .await
    .expect("read task_cap_columns")
}

// ---------------------------------------------------------------------------
// 1. Cap check: approve_and_merge refuses + sets changes_requested.
// ---------------------------------------------------------------------------

/// A self-fix task with `merge_attempts` already at the cap must be refused by
/// `approve_and_merge` with the `merge_attempts_exhausted` error code, and
/// the persisted `review_status` must flip to `changes_requested`.
///
/// The GitHub client is `None`: the cap guard fires BEFORE the github requirement,
/// proving it is unit-testable without GitHub.
#[sqlx::test(migrations = "../db/migrations")]
async fn approve_and_merge_refuses_when_merge_attempts_exhausted(pool: PgPool) {
    let (org_id, user_id) = seed_org(&pool).await;
    let scope = scope_for(org_id, user_id);
    let repo = OrchestrationTaskRepository::new(pool.clone());

    // Create a self-fix task wired with a PR linkage.
    let task = repo
        .create(
            &scope,
            CreateTaskRow {
                group_id: None,
                title: "cap test task",
                description: None,
                priority: "normal",
                params: None,
                assigned_agent_id: None,
                parent_task_id: None,
                initial_status: "backlog",
                initial_blocked_reason: None,
                initial_blocked_metadata: None,
                requires_approval: false,
                self_fix: true,
            },
        )
        .await
        .expect("create self_fix task");

    // Give it a PR linkage and put it in_review so the status gate passes.
    repo.set_pr_metadata(&scope, task.id, 42, "https://github.com/example/repo/pull/42", "deadbeef", "in_review")
        .await
        .expect("set_pr_metadata");

    // Directly set merge_attempts to the default cap (5).
    sqlx::query("UPDATE orchestration_tasks SET merge_attempts = 5 WHERE id = $1")
        .bind(task.id)
        .execute(&pool)
        .await
        .expect("set merge_attempts to cap");

    // Drive approve_and_merge through the testing helper (which wires the real
    // AppState service factory — test_app_config sets max_merge_attempts = 5 and
    // github = None, so the cap guard fires BEFORE the github requirement).
    let state = app_state_with_mock_provider(pool.clone(), "mock", "ok").await;

    let err = approve(&state, &scope, task.id, "operator@example.com")
        .await
        .expect_err("must refuse when merge_attempts == cap");

    // Verify the error code.
    assert!(
        matches!(
            &err.kind,
            ErrorKind::ValidationWithCode { code, .. } if *code == "errors.self_fix.merge_attempts_exhausted"
        ),
        "expected merge_attempts_exhausted, got {:?}",
        err.kind
    );

    // Verify the persisted review_status flipped to changes_requested.
    let (attempts_after, review_after) = task_cap_columns(&pool, task.id).await;
    assert_eq!(
        review_after.as_deref(),
        Some("changes_requested"),
        "review_status must flip to changes_requested on cap refusal"
    );
    // merge_attempts must NOT be incremented on the cap refusal path (the bump
    // comes after the cap check, so it is never reached).
    assert_eq!(attempts_after, 5, "merge_attempts must remain at cap value after refusal");
}

// ---------------------------------------------------------------------------
// 2. bump_merge_attempts increments + tenant isolation.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn bump_merge_attempts_increments_and_is_tenant_scoped(pool: PgPool) {
    let (org_a, user_a) = seed_org(&pool).await;
    let (org_b, user_b) = seed_org(&pool).await;
    let scope_a = scope_for(org_a, user_a);
    let scope_b = scope_for(org_b, user_b);
    let repo = OrchestrationTaskRepository::new(pool.clone());

    let task = repo
        .create(
            &scope_a,
            CreateTaskRow {
                group_id: None,
                title: "bump test",
                description: None,
                priority: "normal",
                params: None,
                assigned_agent_id: None,
                parent_task_id: None,
                initial_status: "backlog",
                initial_blocked_reason: None,
                initial_blocked_metadata: None,
                requires_approval: false,
                self_fix: true,
            },
        )
        .await
        .expect("create task");

    // Initial value is 0.
    let (before, _) = task_cap_columns(&pool, task.id).await;
    assert_eq!(before, 0, "merge_attempts defaults to 0");

    // Bumping with the correct scope increments.
    repo.bump_merge_attempts(&scope_a, task.id).await.expect("bump with owner scope");
    let (after_first, _) = task_cap_columns(&pool, task.id).await;
    assert_eq!(after_first, 1, "bump with owner scope increments to 1");

    repo.bump_merge_attempts(&scope_a, task.id).await.expect("second bump");
    let (after_second, _) = task_cap_columns(&pool, task.id).await;
    assert_eq!(after_second, 2, "second bump increments to 2");

    // A cross-org bump is a no-op: it returns Ok (UPDATE 0 rows) but must NOT
    // touch org A's task.
    repo.bump_merge_attempts(&scope_b, task.id).await.expect("cross-org bump returns Ok");
    let (after_cross, _) = task_cap_columns(&pool, task.id).await;
    assert_eq!(after_cross, 2, "cross-org bump must not touch org A's task");
}
