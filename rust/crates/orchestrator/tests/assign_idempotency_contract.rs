//! Contract tests for assign idempotency (#892 F040).
//!
//! A retried assign must not spawn a second agent session for the same task.
//! Two guards enforce this: the partial-unique index (migration 013) rejects a
//! second active dispatch, and `assign` only transitions a re-dispatchable
//! (pending/failed) task.

use sqlx::PgPool;

use agentforge_orchestrator::task::{PgTaskStore, Store, TaskError, TaskState};

async fn seed_participant_and_task(pool: &PgPool, org_id: &str, state: &str) -> (String, String) {
    let participant_id: String = sqlx::query_scalar(
        "INSERT INTO participants (type, display_name, casdoor_user_id, org_id)
         VALUES ('human', 'idem-user', 'casdoor-idem', $1)
         RETURNING id::text",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed participant");

    let task_id: String = sqlx::query_scalar(
        "INSERT INTO tasks (title, state, created_by, org_id)
         VALUES ('idem-task', $1, CAST($2 AS uuid), $3)
         RETURNING id::text",
    )
    .bind(state)
    .bind(&participant_id)
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed task");

    (participant_id, task_id)
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_active_dispatch_is_rejected(pool: PgPool) {
    let store = PgTaskStore::new(pool.clone());
    let (_participant, task_id) = seed_participant_and_task(&pool, "org-idem", "assigned").await;

    // First dispatch succeeds.
    store.create_dispatch(&task_id, "org-idem").await.expect("first dispatch should succeed");

    // A second active dispatch for the same task is rejected by the partial-unique index.
    let err = store.create_dispatch(&task_id, "org-idem").await.expect_err("second active dispatch must conflict");
    assert!(matches!(err, TaskError::Conflict(_)), "expected Conflict, got {err:?}");

    // Once the active dispatch is terminal (failed), a fresh dispatch is allowed.
    sqlx::query("UPDATE task_dispatches SET status = 'failed' WHERE task_id = $1")
        .bind(&task_id)
        .execute(&pool)
        .await
        .expect("fail the active dispatch");
    store.create_dispatch(&task_id, "org-idem").await.expect("re-dispatch allowed once the prior one failed");
}

#[sqlx::test(migrations = "./migrations")]
async fn assign_rejects_non_redispatchable_task(pool: PgPool) {
    let store = PgTaskStore::new(pool.clone());
    let (participant, task_id) = seed_participant_and_task(&pool, "org-idem2", "pending").await;

    // A pending task assigns fine (pending -> assigned).
    store.assign(&task_id, "org-idem2", participant.clone(), TaskState::Assigned).await.expect("assign pending task");

    // Re-assigning the now-assigned task is a conflict (state not pending/failed),
    // so a retry cannot silently spawn a second agent.
    let err = store
        .assign(&task_id, "org-idem2", participant.clone(), TaskState::Assigned)
        .await
        .expect_err("re-assigning an assigned task must conflict");
    assert!(matches!(err, TaskError::Conflict(_)), "expected Conflict, got {err:?}");

    // A genuinely missing task is still NotFound (not Conflict).
    let missing = store
        .assign("00000000-0000-0000-0000-000000000000", "org-idem2", participant, TaskState::Assigned)
        .await
        .expect_err("missing task must error");
    assert!(matches!(missing, TaskError::NotFound), "expected NotFound, got {missing:?}");
}
