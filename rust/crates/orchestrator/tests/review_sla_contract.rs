//! Contract tests for review SLA: `due_at` create-path and overdue dashboard
//! visibility.  No state-changing reaper is tested here — the scope is
//! visibility-only per issue #801.

use chrono::{Duration, Utc};
use sqlx::PgPool;

use agentforge_orchestrator::metrics::{PgMetricsStore, Store as MetricsStore};
use agentforge_orchestrator::review::{CodeReview, PgReviewStore, ReviewState, Store as ReviewStore};

/// Seed one participant, one task, and one code_review row.
/// Returns (participant_id, task_id, review_id) as UUID text strings.
async fn seed_participant_task(pool: &PgPool, org_id: &str) -> (String, String) {
    let participant_id: String = sqlx::query_scalar(
        "INSERT INTO participants (type, display_name, casdoor_user_id, org_id)
         VALUES ('human', 'sla-test-user', 'casdoor-sla', $1)
         RETURNING id::text",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed participant");

    let task_id: String = sqlx::query_scalar(
        "INSERT INTO tasks (title, state, created_by, org_id)
         VALUES ('sla-task', 'review', CAST($1 AS uuid), $2)
         RETURNING id::text",
    )
    .bind(&participant_id)
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed task");

    (participant_id, task_id)
}

/// Insert a code_review row directly with an explicit `due_at` value so tests
/// control whether the review is overdue without relying on wall-clock timing.
async fn insert_review_with_due_at(
    pool: &PgPool,
    org_id: &str,
    task_id: &str,
    participant_id: &str,
    state: &str,
    due_at: Option<chrono::DateTime<Utc>>,
) -> String {
    sqlx::query_scalar(
        "INSERT INTO code_reviews (task_id, session_id, diff_ref, state, org_id, created_by, due_at)
         VALUES (CAST($1 AS uuid), 'sla-session', 'HEAD', $2, $3, CAST($4 AS uuid), $5)
         RETURNING id::text",
    )
    .bind(task_id)
    .bind(state)
    .bind(org_id)
    .bind(participant_id)
    .bind(due_at)
    .fetch_one(pool)
    .await
    .expect("insert review with due_at")
}

/// A pending review with `due_at` in the past counts as overdue.
/// A fresh pending review with `due_at` in the future does not.
/// Tenant isolation: org B always sees zero overdue.
#[sqlx::test(migrations = "./migrations")]
async fn overdue_review_counted_and_tenant_isolated(pool: PgPool) {
    let org_a = "org-sla-a";
    let org_b = "org-sla-b";

    let (pid_a, task_a) = seed_participant_task(&pool, org_a).await;
    let (pid_b, task_b) = seed_participant_task(&pool, org_b).await;

    // org_a: one overdue pending review (due_at in the past)
    insert_review_with_due_at(&pool, org_a, &task_a, &pid_a, "pending", Some(Utc::now() - Duration::hours(1))).await;

    // org_a: one fresh pending review (due_at in the future) — NOT overdue
    insert_review_with_due_at(&pool, org_a, &task_a, &pid_a, "pending", Some(Utc::now() + Duration::hours(23))).await;

    // org_b: fresh review — org B must always see 0 overdue
    insert_review_with_due_at(&pool, org_b, &task_b, &pid_b, "pending", Some(Utc::now() + Duration::hours(23))).await;

    let store = PgMetricsStore::new(pool.clone());

    let metrics_a = store.dashboard(org_a).await.expect("dashboard org_a");
    assert_eq!(metrics_a.overdue_reviews, 1, "org_a: exactly one overdue review");
    assert_eq!(metrics_a.pending_reviews, 2, "org_a: two pending reviews total");

    let metrics_b = store.dashboard(org_b).await.expect("dashboard org_b");
    assert_eq!(metrics_b.overdue_reviews, 0, "org_b: tenant isolation — zero overdue");
}

/// A newly created review (via PgReviewStore::create) must have a non-None
/// `due_at` value written to the database.
#[sqlx::test(migrations = "./migrations")]
async fn create_review_persists_due_at(pool: PgPool) {
    let org_id = "org-sla-create";
    let (pid, task_id) = seed_participant_task(&pool, org_id).await;

    let sla = Duration::hours(24);
    let expected_due_at = Utc::now() + sla;

    let mut review = CodeReview {
        id: String::new(),
        task_id: task_id.clone(),
        session_id: "sla-create-session".to_string(),
        diff_ref: "HEAD".to_string(),
        diff_snapshot: None,
        state: ReviewState::Pending,
        assigned_to: None,
        org_id: org_id.to_string(),
        created_by: pid.clone(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        due_at: Some(expected_due_at),
    };

    let store = PgReviewStore::new(pool.clone());
    store.create(&mut review).await.expect("create review");

    assert!(review.due_at.is_some(), "created review must have due_at set");

    // Verify the value was persisted to the database
    let db_due_at: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT due_at FROM code_reviews WHERE id = CAST($1 AS uuid)")
            .bind(&review.id)
            .fetch_one(&pool)
            .await
            .expect("fetch due_at");

    assert!(db_due_at.is_some(), "due_at must be persisted in the database");

    // Due_at should be close to what we set (within a few seconds of clock drift)
    let diff = (db_due_at.unwrap() - expected_due_at).num_seconds().abs();
    assert!(diff < 5, "persisted due_at must be close to the value set at create time (diff={diff}s)");
}

/// An in_review overdue review is also counted.
#[sqlx::test(migrations = "./migrations")]
async fn in_review_overdue_is_counted(pool: PgPool) {
    let org_id = "org-sla-in-review";
    let (pid, task_id) = seed_participant_task(&pool, org_id).await;

    insert_review_with_due_at(&pool, org_id, &task_id, &pid, "in_review", Some(Utc::now() - Duration::minutes(30)))
        .await;

    let store = PgMetricsStore::new(pool.clone());
    let metrics = store.dashboard(org_id).await.expect("dashboard");
    assert_eq!(metrics.overdue_reviews, 1, "in_review overdue review must be counted");
}

/// A review with no due_at (NULL) is never counted as overdue even if it is old.
#[sqlx::test(migrations = "./migrations")]
async fn review_without_due_at_is_not_overdue(pool: PgPool) {
    let org_id = "org-sla-null-due";
    let (pid, task_id) = seed_participant_task(&pool, org_id).await;

    insert_review_with_due_at(&pool, org_id, &task_id, &pid, "pending", None).await;

    let store = PgMetricsStore::new(pool.clone());
    let metrics = store.dashboard(org_id).await.expect("dashboard");
    assert_eq!(metrics.overdue_reviews, 0, "NULL due_at must not count as overdue");
}
