//! Contract tests for the dispatch TTL reaper (issue #868).
//!
//! Seeds four `task_dispatches` rows and verifies that only the two stale
//! non-terminal ones (queued + starting, both 2 h old) are timed out by
//! `DispatchReaperWorker::tick`. The fresh queued row and the terminal
//! `started` row must be left untouched.
//!
//! Additional tests pin:
//! - `failed` rows are never touched even if stale (fix #3 race guard)
//! - TTL boundary: 59-minute-old rows survive, 61-minute-old rows are reaped
//! - Multi-org sweep: the reaper is global (not tenant-scoped)

use agentforge_orchestrator::dispatch_reaper::DispatchReaperWorker;

#[sqlx::test(migrations = "./migrations")]
async fn reaps_only_stale_non_terminal_dispatches(pool: sqlx::PgPool) {
    // Seed A: stale queued — should be reaped.
    sqlx::query(
        "INSERT INTO task_dispatches (task_id, org_id, status, updated_at)
         VALUES ('task-a', 'org-test', 'queued', NOW() - INTERVAL '2 hours')",
    )
    .execute(&pool)
    .await
    .expect("seed dispatch A");

    // Seed B: fresh queued — should NOT be reaped.
    sqlx::query(
        "INSERT INTO task_dispatches (task_id, org_id, status, updated_at)
         VALUES ('task-b', 'org-test', 'queued', NOW())",
    )
    .execute(&pool)
    .await
    .expect("seed dispatch B");

    // Seed C: stale started (terminal) — should NOT be reaped.
    sqlx::query(
        "INSERT INTO task_dispatches (task_id, org_id, status, updated_at)
         VALUES ('task-c', 'org-test', 'started', NOW() - INTERVAL '2 hours')",
    )
    .execute(&pool)
    .await
    .expect("seed dispatch C");

    // Seed D: stale starting — should be reaped.
    sqlx::query(
        "INSERT INTO task_dispatches (task_id, org_id, status, updated_at)
         VALUES ('task-d', 'org-test', 'starting', NOW() - INTERVAL '2 hours')",
    )
    .execute(&pool)
    .await
    .expect("seed dispatch D");

    // Run the reaper with a 3600s TTL.
    let reaped = DispatchReaperWorker::tick(&pool, 3600).await.expect("tick should succeed");
    assert_eq!(reaped.dispatches_reaped, 2, "expected exactly 2 dispatches reaped (A and D)");

    // A must now be 'failed' with last_error = 'dispatch_timeout'.
    let (status_a, err_a): (String, Option<String>) =
        sqlx::query_as("SELECT status, last_error FROM task_dispatches WHERE task_id = 'task-a'")
            .fetch_one(&pool)
            .await
            .expect("fetch dispatch A");
    assert_eq!(status_a, "failed", "dispatch A should be failed");
    assert_eq!(err_a.as_deref(), Some("dispatch_timeout"), "dispatch A last_error should be dispatch_timeout");

    // B must still be 'queued'.
    let status_b: String = sqlx::query_scalar("SELECT status FROM task_dispatches WHERE task_id = 'task-b'")
        .fetch_one(&pool)
        .await
        .expect("fetch dispatch B");
    assert_eq!(status_b, "queued", "dispatch B should still be queued");

    // C must still be 'started'.
    let status_c: String = sqlx::query_scalar("SELECT status FROM task_dispatches WHERE task_id = 'task-c'")
        .fetch_one(&pool)
        .await
        .expect("fetch dispatch C");
    assert_eq!(status_c, "started", "dispatch C should still be started");

    // D must now be 'failed' with last_error = 'dispatch_timeout'.
    let (status_d, err_d): (String, Option<String>) =
        sqlx::query_as("SELECT status, last_error FROM task_dispatches WHERE task_id = 'task-d'")
            .fetch_one(&pool)
            .await
            .expect("fetch dispatch D");
    assert_eq!(status_d, "failed", "dispatch D should be failed");
    assert_eq!(err_d.as_deref(), Some("dispatch_timeout"), "dispatch D last_error should be dispatch_timeout");
}

/// Fix #5: A dispatch row already in `failed` must never be overwritten by a
/// reaper tick, even if its `updated_at` is past the TTL. This guards the
/// race-condition where a late-completing spawn might reset a reaper-set
/// failure verdict.
#[sqlx::test(migrations = "./migrations")]
async fn failed_dispatch_is_not_reaped(pool: sqlx::PgPool) {
    // Seed E: stale failed with a sentinel last_error.
    sqlx::query(
        "INSERT INTO task_dispatches (task_id, org_id, status, last_error, updated_at)
         VALUES ('task-e', 'org-test', 'failed', 'oom_killed', NOW() - INTERVAL '2 hours')",
    )
    .execute(&pool)
    .await
    .expect("seed dispatch E");

    let reaped = DispatchReaperWorker::tick(&pool, 3600).await.expect("tick should succeed");
    assert_eq!(reaped.dispatches_reaped, 0, "no dispatches should be reaped when only failed rows are present");

    // E must still be 'failed' with its original last_error sentinel.
    let (status_e, err_e): (String, Option<String>) =
        sqlx::query_as("SELECT status, last_error FROM task_dispatches WHERE task_id = 'task-e'")
            .fetch_one(&pool)
            .await
            .expect("fetch dispatch E");
    assert_eq!(status_e, "failed", "dispatch E should still be failed");
    assert_eq!(err_e.as_deref(), Some("oom_killed"), "dispatch E last_error must not be overwritten");
}

/// Fix #6: TTL boundary test. At TTL=3600 s (60 min):
/// - A row 59 minutes old must survive (not yet expired).
/// - A row 61 minutes old must be reaped (past TTL).
///
/// This pins the `<` direction of the timestamp predicate and verifies that
/// the `make_interval(secs => $1)` bind treats the value as seconds.
#[sqlx::test(migrations = "./migrations")]
async fn ttl_boundary_respected(pool: sqlx::PgPool) {
    // Seed F: 59 minutes old — must survive at TTL=3600 s.
    sqlx::query(
        "INSERT INTO task_dispatches (task_id, org_id, status, updated_at)
         VALUES ('task-f', 'org-boundary', 'queued', NOW() - INTERVAL '59 minutes')",
    )
    .execute(&pool)
    .await
    .expect("seed dispatch F");

    // Seed G: 61 minutes old — must be reaped at TTL=3600 s.
    sqlx::query(
        "INSERT INTO task_dispatches (task_id, org_id, status, updated_at)
         VALUES ('task-g', 'org-boundary', 'queued', NOW() - INTERVAL '61 minutes')",
    )
    .execute(&pool)
    .await
    .expect("seed dispatch G");

    let reaped = DispatchReaperWorker::tick(&pool, 3600).await.expect("tick should succeed");
    assert_eq!(reaped.dispatches_reaped, 1, "only the 61-minute-old dispatch should be reaped");

    // F must still be 'queued'.
    let status_f: String = sqlx::query_scalar("SELECT status FROM task_dispatches WHERE task_id = 'task-f'")
        .fetch_one(&pool)
        .await
        .expect("fetch dispatch F");
    assert_eq!(status_f, "queued", "dispatch F (59 min old) must survive TTL=3600");

    // G must now be 'failed'.
    let status_g: String = sqlx::query_scalar("SELECT status FROM task_dispatches WHERE task_id = 'task-g'")
        .fetch_one(&pool)
        .await
        .expect("fetch dispatch G");
    assert_eq!(status_g, "failed", "dispatch G (61 min old) must be reaped at TTL=3600");
}

/// Fix #7: Multi-org sweep. The reaper is deliberately global (not
/// tenant-scoped). Two stale queued rows from different org_ids must both be
/// reaped in a single tick.
#[sqlx::test(migrations = "./migrations")]
async fn multi_org_sweep(pool: sqlx::PgPool) {
    // Seed H: stale queued under org-alpha.
    sqlx::query(
        "INSERT INTO task_dispatches (task_id, org_id, status, updated_at)
         VALUES ('task-h', 'org-alpha', 'queued', NOW() - INTERVAL '2 hours')",
    )
    .execute(&pool)
    .await
    .expect("seed dispatch H");

    // Seed I: stale queued under org-beta.
    sqlx::query(
        "INSERT INTO task_dispatches (task_id, org_id, status, updated_at)
         VALUES ('task-i', 'org-beta', 'queued', NOW() - INTERVAL '2 hours')",
    )
    .execute(&pool)
    .await
    .expect("seed dispatch I");

    let reaped = DispatchReaperWorker::tick(&pool, 3600).await.expect("tick should succeed");
    assert_eq!(reaped.dispatches_reaped, 2, "both cross-org stale dispatches should be reaped in one pass");

    // H must be 'failed'.
    let status_h: String = sqlx::query_scalar("SELECT status FROM task_dispatches WHERE task_id = 'task-h'")
        .fetch_one(&pool)
        .await
        .expect("fetch dispatch H");
    assert_eq!(status_h, "failed", "dispatch H (org-alpha) should be reaped");

    // I must be 'failed'.
    let status_i: String = sqlx::query_scalar("SELECT status FROM task_dispatches WHERE task_id = 'task-i'")
        .fetch_one(&pool)
        .await
        .expect("fetch dispatch I");
    assert_eq!(status_i, "failed", "dispatch I (org-beta) should be reaped");
}

/// F051: the bounded sweep caps each statement at 1000 rows, so a stuck backlog
/// LARGER than one batch must still be fully drained in a single `tick()` — the
/// loop runs batches until a pass returns fewer than the limit. This fails if the
/// loop is removed (only the first 1000 would be reaped) while still proving the
/// statement is bounded (asserted structurally by the SQL-pin unit test).
#[sqlx::test(migrations = "./migrations")]
async fn tick_drains_backlog_larger_than_one_batch(pool: sqlx::PgPool) {
    // Seed 1001 stale queued dispatches — one more than the 1000-row batch cap.
    sqlx::query(
        "INSERT INTO task_dispatches (task_id, org_id, status, updated_at)
         SELECT 'task-batch-' || g, 'org-batch', 'queued', NOW() - INTERVAL '2 hours'
         FROM generate_series(1, 1001) AS g",
    )
    .execute(&pool)
    .await
    .expect("seed 1001 stale dispatches");

    let outcome = DispatchReaperWorker::tick(&pool, 3600).await.expect("tick should succeed");
    assert_eq!(outcome.dispatches_reaped, 1001, "one tick must drain the whole backlog across batches");

    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM task_dispatches WHERE org_id = 'org-batch' AND status <> 'failed'")
            .fetch_one(&pool)
            .await
            .expect("count remaining");
    assert_eq!(remaining, 0, "no stuck dispatch may be left behind after a single tick");
}

/// F039: reaping a stuck dispatch must close the loop on the owning task. A
/// task parked in `assigned` (its dispatch never started) is reset to `pending`
/// so it is re-dispatchable, in the same atomic pass that fails the dispatch.
/// A `working` task (agent actively on it) and tasks whose dispatch was NOT
/// reaped must be left untouched.
#[sqlx::test(migrations = "./migrations")]
async fn reaped_dispatch_resets_owning_assigned_task_to_pending(pool: sqlx::PgPool) {
    let participant_id = "22222222-2222-2222-2222-222222222222";
    // assigned task with a stale dispatch — should be reset to pending.
    let assigned_task = "11111111-1111-1111-1111-111111111111";
    // working task with a stale dispatch — must NOT be reset (agent is on it).
    let working_task = "33333333-3333-3333-3333-333333333333";
    // assigned task whose dispatch is fresh — must NOT be reset (not reaped).
    let fresh_task = "44444444-4444-4444-4444-444444444444";

    sqlx::query(
        "INSERT INTO participants (id, type, display_name, agent_session_id, org_id)
         VALUES ($1::uuid, 'agent', 'Recon Agent', 'sess-recon', 'org-recon')",
    )
    .bind(participant_id)
    .execute(&pool)
    .await
    .expect("seed participant");

    for (tid, state) in [(assigned_task, "assigned"), (working_task, "working"), (fresh_task, "assigned")] {
        sqlx::query(
            "INSERT INTO tasks (id, title, state, created_by, org_id)
             VALUES ($1::uuid, 'Recon Task', $2, $3::uuid, 'org-recon')",
        )
        .bind(tid)
        .bind(state)
        .bind(participant_id)
        .execute(&pool)
        .await
        .expect("seed task");
    }

    // Stale dispatches for the assigned + working tasks (both reaped); fresh
    // dispatch for fresh_task (not reaped).
    for (tid, age) in [(assigned_task, "2 hours"), (working_task, "2 hours")] {
        sqlx::query(&format!(
            "INSERT INTO task_dispatches (task_id, org_id, status, updated_at)
             VALUES ($1, 'org-recon', 'starting', NOW() - INTERVAL '{age}')"
        ))
        .bind(tid)
        .execute(&pool)
        .await
        .expect("seed stale dispatch");
    }
    sqlx::query(
        "INSERT INTO task_dispatches (task_id, org_id, status, updated_at)
         VALUES ($1, 'org-recon', 'starting', NOW())",
    )
    .bind(fresh_task)
    .execute(&pool)
    .await
    .expect("seed fresh dispatch");

    let outcome = DispatchReaperWorker::tick(&pool, 3600).await.expect("tick should succeed");
    assert_eq!(outcome.dispatches_reaped, 2, "the two stale dispatches (assigned + working) are reaped");
    assert_eq!(outcome.tasks_reconciled, 1, "only the assigned task is reset to pending");

    let assigned_state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE id = $1::uuid")
        .bind(assigned_task)
        .fetch_one(&pool)
        .await
        .expect("fetch assigned task");
    assert_eq!(assigned_state, "pending", "orphaned assigned task must return to pending");

    let working_state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE id = $1::uuid")
        .bind(working_task)
        .fetch_one(&pool)
        .await
        .expect("fetch working task");
    assert_eq!(working_state, "working", "a working task must not be reset by the reaper");

    let fresh_state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE id = $1::uuid")
        .bind(fresh_task)
        .fetch_one(&pool)
        .await
        .expect("fetch fresh task");
    assert_eq!(fresh_state, "assigned", "a task whose dispatch was not reaped must be left alone");

    let dispatch_status: String = sqlx::query_scalar("SELECT status FROM task_dispatches WHERE task_id = $1")
        .bind(assigned_task)
        .fetch_one(&pool)
        .await
        .expect("fetch dispatch");
    assert_eq!(dispatch_status, "failed", "the stale dispatch is failed");
}
