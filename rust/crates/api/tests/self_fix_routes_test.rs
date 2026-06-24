//! Milestone 8 of the self-fix loop — integration coverage for the in-platform
//! review surface (`review_snapshot` / `approve_and_merge`) driven through the
//! REAL `AppState` service factory.
//!
//! A test `AppState` has GitHub unconfigured, so the live CI verdict fails closed
//! to `false` and `approve` short-circuits with a visible "GitHub not configured"
//! error before any merge — which is exactly what lets these assertions prove the
//! gate ordering without a network. The merge gate itself (sensitive refuse, red
//! CI, head-moved, happy-path merge) is covered against an in-memory fake
//! `GitProvider` in `self_fix_merge_test.rs`.
//!
//! Coverage:
//! 1. A self-fix task's snapshot reports its PR columns, derives `sensitive` from
//!    the persisted review status, and reports `checks_green = false` (fail closed
//!    with no GitHub configured).
//! 2. `sensitive_blocked` review status flips the snapshot's `sensitive` flag.
//! 3. Tenant isolation: another org cannot read this org's review snapshot.
//! 4. A non-self-fix task is rejected by both `review` and `approve`.
//! 5. `approve` on a self-fix task with no GitHub configured fails closed (no
//!    merge), and never reports success.

use agentforge_api::repositories::orchestration::{CreateTaskRow, OrchestrationTaskRepository};
use agentforge_api::test_support::{app_state_with_mock_provider, tenant_scope_for_ids};
use agentforge_api::testing::self_fix_review::{approve, review_fields};
use agentforge_core::TenantScope;
use sqlx::PgPool;
use uuid::Uuid;

/// Seed one org (+ workspace + user). Tasks are created unassigned.
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

fn base_row(title: &str, self_fix: bool) -> CreateTaskRow<'_> {
    CreateTaskRow {
        group_id: None,
        title,
        description: None,
        priority: "normal",
        params: None,
        assigned_agent_id: None,
        parent_task_id: None,
        initial_status: "backlog",
        initial_blocked_reason: None,
        initial_blocked_metadata: None,
        requires_approval: false,
        self_fix,
    }
}

// ---------------------------------------------------------------------------
// 1. Snapshot reports PR columns + fails CI closed when GitHub is unconfigured.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn review_snapshot_reports_columns_and_fails_ci_closed(pool: PgPool) {
    let (org_id, user_id) = seed_org(&pool).await;
    let scope = scope_for(org_id, user_id);
    let state = app_state_with_mock_provider(pool.clone(), "mock", "ok").await;
    let repo = OrchestrationTaskRepository::new(pool.clone());

    let task = repo.create(&scope, base_row("self-fix task", true)).await.expect("create self-fix task");
    let pr_url = "https://github.com/example/repo/pull/77";
    repo.set_pr_metadata(&scope, task.id, 77, pr_url, "headsha", "in_review").await.expect("set_pr_metadata");

    let (pr_number, checks_green, sensitive, review_status) =
        review_fields(&state, &scope, task.id).await.expect("review snapshot");

    assert_eq!(pr_number, Some(77), "pr_number is reported");
    assert!(!checks_green, "no GitHub configured → CI verdict fails closed to false");
    assert!(!sensitive, "in_review is not sensitive");
    assert_eq!(review_status.as_deref(), Some("in_review"));
}

// ---------------------------------------------------------------------------
// 2. sensitive_blocked review status flips the sensitive flag.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn review_snapshot_marks_sensitive_blocked(pool: PgPool) {
    let (org_id, user_id) = seed_org(&pool).await;
    let scope = scope_for(org_id, user_id);
    let state = app_state_with_mock_provider(pool.clone(), "mock", "ok").await;
    let repo = OrchestrationTaskRepository::new(pool.clone());

    let task = repo.create(&scope, base_row("sensitive self-fix", true)).await.expect("create");
    repo.set_pr_metadata(&scope, task.id, 9, "https://github.com/e/r/pull/9", "h", "sensitive_blocked")
        .await
        .expect("set_pr_metadata");

    let (_, checks_green, sensitive, review_status) =
        review_fields(&state, &scope, task.id).await.expect("review snapshot");

    assert!(sensitive, "sensitive_blocked must surface as sensitive = true (Approve disabled in FE)");
    assert!(!checks_green);
    assert_eq!(review_status.as_deref(), Some("sensitive_blocked"));
}

// ---------------------------------------------------------------------------
// 3. Tenant isolation: org B cannot read org A's review snapshot.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn review_snapshot_is_tenant_scoped(pool: PgPool) {
    let (org_a, user_a) = seed_org(&pool).await;
    let (org_b, user_b) = seed_org(&pool).await;
    let scope_a = scope_for(org_a, user_a);
    let scope_b = scope_for(org_b, user_b);
    let state = app_state_with_mock_provider(pool.clone(), "mock", "ok").await;
    let repo = OrchestrationTaskRepository::new(pool.clone());

    let task = repo.create(&scope_a, base_row("org A self-fix", true)).await.expect("create org A task");

    // Org B asks for org A's task review → no access (find_by_id is tenant-scoped).
    let cross = review_fields(&state, &scope_b, task.id).await;
    assert!(cross.is_err(), "org B must not read org A's self-fix review snapshot");

    // The owner still can.
    assert!(review_fields(&state, &scope_a, task.id).await.is_ok(), "owner reads its own snapshot");
}

// ---------------------------------------------------------------------------
// 4. A non-self-fix task is rejected by both review and approve.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn non_self_fix_task_is_rejected(pool: PgPool) {
    let (org_id, user_id) = seed_org(&pool).await;
    let scope = scope_for(org_id, user_id);
    let state = app_state_with_mock_provider(pool.clone(), "mock", "ok").await;
    let repo = OrchestrationTaskRepository::new(pool.clone());

    let task = repo.create(&scope, base_row("ordinary task", false)).await.expect("create ordinary task");

    assert!(review_fields(&state, &scope, task.id).await.is_err(), "review rejects a non-self-fix task");
    assert!(
        approve(&state, &scope, task.id, &user_id.to_string()).await.is_err(),
        "approve rejects a non-self-fix task"
    );
}

// ---------------------------------------------------------------------------
// 5. approve fails closed (no merge) when GitHub is unconfigured.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn approve_fails_closed_without_github(pool: PgPool) {
    let (org_id, user_id) = seed_org(&pool).await;
    let scope = scope_for(org_id, user_id);
    let state = app_state_with_mock_provider(pool.clone(), "mock", "ok").await;
    let repo = OrchestrationTaskRepository::new(pool.clone());

    let task = repo.create(&scope, base_row("approvable self-fix", true)).await.expect("create");
    repo.set_pr_metadata(&scope, task.id, 5, "https://github.com/e/r/pull/5", "head5", "approved")
        .await
        .expect("set_pr_metadata");

    // With no GitHub App configured the merge cannot proceed; approve must error
    // rather than report a phantom merge. The review_status stays non-merged.
    assert!(
        approve(&state, &scope, task.id, &user_id.to_string()).await.is_err(),
        "approve without a configured GitHub App must fail closed, never merge"
    );

    let (_, _, _, review_status) = review_fields(&state, &scope, task.id).await.expect("snapshot after failed approve");
    assert_eq!(review_status.as_deref(), Some("approved"), "a failed approve does not advance to merged");
}
