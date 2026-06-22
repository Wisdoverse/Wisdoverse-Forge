use sqlx::PgPool;

use agentforge_orchestrator::review::{PgReviewStore, ReviewState, Store as ReviewStore};
use agentforge_orchestrator::task::TaskState;

/// Seed a minimal participant, task, and code_review row.
/// Returns (task_id, review_id) as UUID strings.
async fn seed_review_and_task(pool: &PgPool, org_id: &str) -> (String, String) {
    // participants.created_by is a NOT NULL FK; seed one human participant.
    let participant_id: String = sqlx::query_scalar(
        "INSERT INTO participants (type, display_name, casdoor_user_id, org_id)
         VALUES ('human', 'test-user', 'casdoor-test', $1)
         RETURNING id::text",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed participant");

    // tasks: state must be 'review' (legal state when a review exists)
    let task_id: String = sqlx::query_scalar(
        "INSERT INTO tasks (title, state, created_by, org_id)
         VALUES ('verdict-tx-task', 'review', CAST($1 AS uuid), $2)
         RETURNING id::text",
    )
    .bind(&participant_id)
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed task");

    // code_reviews: state 'pending'
    let review_id: String = sqlx::query_scalar(
        "INSERT INTO code_reviews (task_id, session_id, diff_ref, state, org_id, created_by)
         VALUES (CAST($1 AS uuid), 'session-tx-test', 'HEAD', 'pending', $2, CAST($3 AS uuid))
         RETURNING id::text",
    )
    .bind(&task_id)
    .bind(org_id)
    .bind(&participant_id)
    .fetch_one(pool)
    .await
    .expect("seed code_review");

    (task_id, review_id)
}

/// Both `code_reviews.state` and `tasks.state` must be updated when apply_verdict
/// succeeds — proving the transaction commits both writes.
#[sqlx::test(migrations = "./migrations")]
async fn apply_verdict_commits_both_review_and_task(pool: PgPool) {
    let org_id = "org-verdict-tx";
    let (task_id, review_id) = seed_review_and_task(&pool, org_id).await;

    let store = PgReviewStore::new(pool.clone());
    store
        .apply_verdict(&review_id, org_id, ReviewState::Approved, &task_id, TaskState::Completed)
        .await
        .expect("apply_verdict should succeed");

    let review_state: String =
        sqlx::query_scalar("SELECT state FROM code_reviews WHERE id = CAST($1 AS uuid)")
            .bind(&review_id)
            .fetch_one(&pool)
            .await
            .expect("fetch review state");
    assert_eq!(review_state, "approved", "code_reviews.state should be 'approved'");

    let task_state: String =
        sqlx::query_scalar("SELECT state FROM tasks WHERE id = CAST($1 AS uuid)")
            .bind(&task_id)
            .fetch_one(&pool)
            .await
            .expect("fetch task state");
    assert_eq!(task_state, "completed", "tasks.state should be 'completed'");
}

/// When the task_id does not exist, apply_verdict must return Err and the
/// code_reviews row must remain unchanged — proving the transaction rolls back.
#[sqlx::test(migrations = "./migrations")]
async fn apply_verdict_rolls_back_when_task_missing(pool: PgPool) {
    let org_id = "org-verdict-rollback";
    let (_task_id, review_id) = seed_review_and_task(&pool, org_id).await;

    let nonexistent_task_id = "00000000-0000-0000-0000-000000000000";

    let store = PgReviewStore::new(pool.clone());
    let result = store
        .apply_verdict(&review_id, org_id, ReviewState::Approved, nonexistent_task_id, TaskState::Completed)
        .await;

    assert!(result.is_err(), "apply_verdict should return Err when task is missing");

    // The review UPDATE must have been rolled back: state still 'pending'.
    let review_state: String =
        sqlx::query_scalar("SELECT state FROM code_reviews WHERE id = CAST($1 AS uuid)")
            .bind(&review_id)
            .fetch_one(&pool)
            .await
            .expect("fetch review state after rollback");
    assert_eq!(
        review_state, "pending",
        "code_reviews.state must still be 'pending' after rollback (atomicity)"
    );
}
