//! Milestone 1 of the self-fix loop — integration coverage for the PR /
//! base-SHA / review-status columns added to `orchestration_tasks` in
//! migration 068.
//!
//! These columns carry a self-fix task's draft-PR linkage:
//! - `self_fix` — marks a code-fix task against this repo (set at create time).
//! - `base_commit_sha` — the pinned `origin/main` SHA the PR Bridge rebuilds onto.
//! - `pr_number` / `pr_url` / `pr_head_sha` — GitHub opaque draft-PR values.
//! - `review_status` — mirrors the orchestrator `ReviewState` vocabulary,
//!   driven API-side on the task.
//!
//! Coverage:
//! 1. A normal task defaults `self_fix = false` and leaves the PR columns NULL.
//! 2. A `self_fix = true` task, after `set_base_commit_sha` + `set_pr_metadata`
//!    + `set_review_status`, round-trips all six columns on a fresh SELECT.
//! 3. A DIFFERENT org's `set_review_status` updates 0 rows — the tenant guard
//!    on every UPDATE is load-bearing.

use agentforge_api::repositories::orchestration::{CreateTaskRow, OrchestrationTaskRepository};
use agentforge_api::test_support::tenant_scope_for_ids;
use agentforge_core::TenantScope;
use sqlx::PgPool;
use uuid::Uuid;

/// Seed one org + workspace + user. No agent is needed because these tasks are
/// created unassigned (`assigned_agent_id` stays NULL).
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

/// Read back the six self-fix columns for a task.
async fn pr_columns(
    pool: &PgPool,
    task_id: Uuid,
) -> (bool, Option<String>, Option<i32>, Option<String>, Option<String>, Option<String>) {
    sqlx::query_as::<_, (bool, Option<String>, Option<i32>, Option<String>, Option<String>, Option<String>)>(
        r#"SELECT self_fix, base_commit_sha, pr_number, pr_url, pr_head_sha, review_status
           FROM orchestration_tasks WHERE id = $1"#,
    )
    .bind(task_id)
    .fetch_one(pool)
    .await
    .expect("query pr columns")
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
// 1. A normal task defaults self_fix = false; PR columns are NULL.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn normal_task_defaults_self_fix_false_and_null_pr_columns(pool: PgPool) {
    let (org_id, user_id) = seed_org(&pool).await;
    let scope = scope_for(org_id, user_id);
    let repo = OrchestrationTaskRepository::new(pool.clone());

    let task = repo.create(&scope, base_row("normal task", false)).await.expect("create normal task");

    // Entity hydration carries the defaults straight off `RETURNING *`.
    assert!(!task.self_fix, "default self_fix must be false");
    assert_eq!(task.base_commit_sha, None);
    assert_eq!(task.pr_number, None);
    assert_eq!(task.pr_url, None);
    assert_eq!(task.pr_head_sha, None);
    assert_eq!(task.review_status, None);

    // And the persisted row agrees.
    let (self_fix, base, num, url, head, review) = pr_columns(&pool, task.id).await;
    assert!(!self_fix);
    assert_eq!(base, None);
    assert_eq!(num, None);
    assert_eq!(url, None);
    assert_eq!(head, None);
    assert_eq!(review, None);
}

// ---------------------------------------------------------------------------
// 2. self_fix=true + the three setters round-trip all six columns.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn self_fix_task_round_trips_all_pr_columns(pool: PgPool) {
    let (org_id, user_id) = seed_org(&pool).await;
    let scope = scope_for(org_id, user_id);
    let repo = OrchestrationTaskRepository::new(pool.clone());

    let task = repo.create(&scope, base_row("self-fix task", true)).await.expect("create self-fix task");
    assert!(task.self_fix, "self_fix=true must round-trip on create");

    let base_sha = format!("base-{}", Uuid::new_v4());
    let pr_url = format!("https://github.com/example/repo/pull/{}", Uuid::new_v4());
    let head_sha = format!("head-{}", Uuid::new_v4());

    repo.set_base_commit_sha(&scope, task.id, &base_sha).await.expect("set_base_commit_sha");
    repo.set_pr_metadata(&scope, task.id, 4242, &pr_url, &head_sha, "pending").await.expect("set_pr_metadata");
    repo.set_review_status(&scope, task.id, "approved").await.expect("set_review_status");

    let (self_fix, base, num, url, head, review) = pr_columns(&pool, task.id).await;
    assert!(self_fix, "self_fix stays true");
    assert_eq!(base.as_deref(), Some(base_sha.as_str()), "base_commit_sha round-trips");
    assert_eq!(num, Some(4242), "pr_number round-trips");
    assert_eq!(url.as_deref(), Some(pr_url.as_str()), "pr_url round-trips");
    assert_eq!(head.as_deref(), Some(head_sha.as_str()), "pr_head_sha round-trips");
    // set_review_status ran last, so the final value wins over set_pr_metadata's "pending".
    assert_eq!(review.as_deref(), Some("approved"), "review_status reflects the latest setter");
}

// ---------------------------------------------------------------------------
// 3. Tenant boundary: a different org cannot mutate this org's task. The
//    tenant clause on every UPDATE is load-bearing — drop it and this turns
//    into a cross-org write.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn set_review_status_is_tenant_scoped(pool: PgPool) {
    let (org_a, user_a) = seed_org(&pool).await;
    let (org_b, user_b) = seed_org(&pool).await;
    let scope_a = scope_for(org_a, user_a);
    let scope_b = scope_for(org_b, user_b);
    let repo = OrchestrationTaskRepository::new(pool.clone());

    // Task belongs to org A.
    let task = repo.create(&scope_a, base_row("org A self-fix", true)).await.expect("create org A task");
    repo.set_review_status(&scope_a, task.id, "pending").await.expect("org A owner can set status");

    // Org B attempts the same id with its own scope. The method returns Ok
    // (UPDATE of 0 rows is not an error) but must NOT touch org A's row.
    repo.set_review_status(&scope_b, task.id, "rejected").await.expect("cross-tenant call returns Ok with 0 rows");

    let (_, _, _, _, _, review) = pr_columns(&pool, task.id).await;
    assert_eq!(
        review.as_deref(),
        Some("pending"),
        "org B must not overwrite org A's review_status — tenant guard on the UPDATE is load-bearing"
    );
}
