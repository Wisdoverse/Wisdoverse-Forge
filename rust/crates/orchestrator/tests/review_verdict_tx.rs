use sqlx::PgPool;

use agentforge_orchestrator::review::{PgReviewStore, ReviewComment, ReviewState, Store as ReviewStore};
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
        .apply_verdict(&review_id, org_id, ReviewState::Approved, &task_id, TaskState::Completed, None)
        .await
        .expect("apply_verdict should succeed");

    let review_state: String = sqlx::query_scalar("SELECT state FROM code_reviews WHERE id = CAST($1 AS uuid)")
        .bind(&review_id)
        .fetch_one(&pool)
        .await
        .expect("fetch review state");
    assert_eq!(review_state, "approved", "code_reviews.state should be 'approved'");

    let task_state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE id = CAST($1 AS uuid)")
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
        .apply_verdict(&review_id, org_id, ReviewState::Approved, nonexistent_task_id, TaskState::Completed, None)
        .await;

    assert!(result.is_err(), "apply_verdict should return Err when task is missing");

    // The review UPDATE must have been rolled back: state still 'pending'.
    let review_state: String = sqlx::query_scalar("SELECT state FROM code_reviews WHERE id = CAST($1 AS uuid)")
        .bind(&review_id)
        .fetch_one(&pool)
        .await
        .expect("fetch review state after rollback");
    assert_eq!(review_state, "pending", "code_reviews.state must still be 'pending' after rollback (atomicity)");
}

/// Helper: read the `created_by` participant uuid of a review (valid FK for
/// `review_comments.author_id`).
async fn review_author(pool: &PgPool, review_id: &str) -> String {
    sqlx::query_scalar("SELECT created_by::text FROM code_reviews WHERE id = CAST($1 AS uuid)")
        .bind(review_id)
        .fetch_one(pool)
        .await
        .expect("fetch review author")
}

fn feedback_comment(review_id: &str, author_id: String, body: &str) -> ReviewComment {
    ReviewComment {
        id: String::new(),
        review_id: review_id.to_string(),
        author_id,
        body: body.to_string(),
        file_path: None,
        line: None,
        created_at: chrono::Utc::now(),
    }
}

/// Reject path: the feedback comment is written inside the verdict transaction,
/// so a successful verdict commits review state, task state, AND the comment.
#[sqlx::test(migrations = "./migrations")]
async fn apply_verdict_with_feedback_commits_comment(pool: PgPool) {
    let org_id = "org-verdict-feedback";
    let (task_id, review_id) = seed_review_and_task(&pool, org_id).await;
    let author = review_author(&pool, &review_id).await;
    let feedback = feedback_comment(&review_id, author, "please fix the failing test");

    let store = PgReviewStore::new(pool.clone());
    store
        .apply_verdict(
            &review_id,
            org_id,
            ReviewState::ChangesRequested,
            &task_id,
            TaskState::ChangesRequested,
            Some(&feedback),
        )
        .await
        .expect("apply_verdict with feedback should succeed");

    let review_state: String = sqlx::query_scalar("SELECT state FROM code_reviews WHERE id = CAST($1 AS uuid)")
        .bind(&review_id)
        .fetch_one(&pool)
        .await
        .expect("fetch review state");
    assert_eq!(review_state, "changes_requested");

    let (count, body): (i64, Option<String>) =
        sqlx::query_as("SELECT COUNT(*), MAX(body) FROM review_comments WHERE review_id = CAST($1 AS uuid)")
            .bind(&review_id)
            .fetch_one(&pool)
            .await
            .expect("fetch comments");
    assert_eq!(count, 1, "exactly one feedback comment committed with the verdict");
    assert_eq!(body.as_deref(), Some("please fix the failing test"));
}

/// Reject path atomicity: when the task UPDATE matches no row the whole tx rolls
/// back — the review stays pending AND no orphan feedback comment is left behind.
#[sqlx::test(migrations = "./migrations")]
async fn apply_verdict_with_feedback_rolls_back_comment_when_task_missing(pool: PgPool) {
    let org_id = "org-verdict-feedback-rollback";
    let (_task_id, review_id) = seed_review_and_task(&pool, org_id).await;
    let author = review_author(&pool, &review_id).await;
    let feedback = feedback_comment(&review_id, author, "orphan-if-not-atomic");
    let nonexistent_task_id = "00000000-0000-0000-0000-000000000000";

    let store = PgReviewStore::new(pool.clone());
    let result = store
        .apply_verdict(
            &review_id,
            org_id,
            ReviewState::ChangesRequested,
            nonexistent_task_id,
            TaskState::ChangesRequested,
            Some(&feedback),
        )
        .await;
    assert!(result.is_err(), "apply_verdict should fail when task is missing");

    let review_state: String = sqlx::query_scalar("SELECT state FROM code_reviews WHERE id = CAST($1 AS uuid)")
        .bind(&review_id)
        .fetch_one(&pool)
        .await
        .expect("fetch review state");
    assert_eq!(review_state, "pending", "review must roll back to pending");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM review_comments WHERE review_id = CAST($1 AS uuid)")
        .bind(&review_id)
        .fetch_one(&pool)
        .await
        .expect("count comments");
    assert_eq!(count, 0, "no orphan feedback comment after rollback");
}
