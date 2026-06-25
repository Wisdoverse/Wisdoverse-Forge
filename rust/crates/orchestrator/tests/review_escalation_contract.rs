//! Contract tests for the overdue review escalation reaper (#871).
//!
//! The reaper is non-destructive: it sets `code_reviews.escalated_at` exactly
//! once on overdue, non-terminal reviews and NEVER touches `state`. These tests
//! drive `ReviewEscalationReaperWorker::tick` against a real Postgres schema and
//! assert each acceptance criterion from the plan.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;

use agentforge_orchestrator::audit::{AuditFilter, MemoryStore as MemoryAuditStore, Store as AuditStore};
use agentforge_orchestrator::realtime::Broadcaster;
use agentforge_orchestrator::review_escalation_reaper::ReviewEscalationReaperWorker;

/// Grace window used by every test. A review escalates only when
/// `due_at < now - GRACE_SECS`.
const GRACE_SECS: u64 = 3600;

/// Seed one participant + one task for the given org and return their UUID text.
async fn seed_participant_task(pool: &PgPool, org_id: &str, tag: &str) -> (String, String) {
    let participant_id: String = sqlx::query_scalar(
        "INSERT INTO participants (type, display_name, casdoor_user_id, org_id)
         VALUES ('human', 'esc-test-user', $1, $2)
         RETURNING id::text",
    )
    .bind(format!("casdoor-{tag}"))
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed participant");

    let task_id: String = sqlx::query_scalar(
        "INSERT INTO tasks (title, state, created_by, org_id)
         VALUES ('esc-task', 'review', CAST($1 AS uuid), $2)
         RETURNING id::text",
    )
    .bind(&participant_id)
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed task");

    (participant_id, task_id)
}

/// Insert one code_review with an explicit `state`, `due_at`, and `escalated_at`
/// so tests control overdue-ness and prior-escalation without wall-clock races.
async fn insert_review(
    pool: &PgPool,
    org_id: &str,
    task_id: &str,
    participant_id: &str,
    state: &str,
    due_at: Option<DateTime<Utc>>,
    escalated_at: Option<DateTime<Utc>>,
) -> String {
    sqlx::query_scalar(
        "INSERT INTO code_reviews (task_id, session_id, diff_ref, state, org_id, created_by, due_at, escalated_at)
         VALUES (CAST($1 AS uuid), 'esc-session', 'HEAD', $2, $3, CAST($4 AS uuid), $5, $6)
         RETURNING id::text",
    )
    .bind(task_id)
    .bind(state)
    .bind(org_id)
    .bind(participant_id)
    .bind(due_at)
    .bind(escalated_at)
    .fetch_one(pool)
    .await
    .expect("insert review")
}

/// Read back (state, escalated_at) for a review id.
async fn read_review(pool: &PgPool, review_id: &str) -> (String, Option<DateTime<Utc>>) {
    let row: (String, Option<DateTime<Utc>>) =
        sqlx::query_as("SELECT state, escalated_at FROM code_reviews WHERE id = CAST($1 AS uuid)")
            .bind(review_id)
            .fetch_one(pool)
            .await
            .expect("read review");
    row
}

fn worker(pool: &PgPool) -> ReviewEscalationReaperWorker {
    ReviewEscalationReaperWorker::new(pool.clone(), Arc::new(Broadcaster::new()), None, GRACE_SECS, false)
}

/// (a) Overdue non-terminal unescalated review → escalated_at set, state unchanged.
#[sqlx::test(migrations = "./migrations")]
async fn overdue_review_is_escalated_state_unchanged(pool: PgPool) {
    let org = "org-esc-a";
    let (pid, task) = seed_participant_task(&pool, org, "a").await;

    // due 2h ago, well past the 1h grace window
    let review_id =
        insert_review(&pool, org, &task, &pid, "pending", Some(Utc::now() - Duration::hours(2)), None).await;

    let escalated = worker(&pool).tick().await.expect("tick");
    assert_eq!(escalated, 1, "exactly one review escalated");

    let (state, escalated_at) = read_review(&pool, &review_id).await;
    assert_eq!(state, "pending", "state must be unchanged by escalation");
    assert!(escalated_at.is_some(), "escalated_at must be set");
}

/// (b) Fresh review (due_at within the grace window) → untouched.
#[sqlx::test(migrations = "./migrations")]
async fn fresh_review_is_not_escalated(pool: PgPool) {
    let org = "org-esc-fresh";
    let (pid, task) = seed_participant_task(&pool, org, "fresh").await;

    // due only 10 minutes ago — inside the 1h grace, not yet escalatable
    let review_id =
        insert_review(&pool, org, &task, &pid, "pending", Some(Utc::now() - Duration::minutes(10)), None).await;

    let escalated = worker(&pool).tick().await.expect("tick");
    assert_eq!(escalated, 0, "review inside grace window must not escalate");

    let (_, escalated_at) = read_review(&pool, &review_id).await;
    assert!(escalated_at.is_none(), "fresh review escalated_at must stay NULL");
}

/// A future-due review is likewise never escalated.
#[sqlx::test(migrations = "./migrations")]
async fn future_due_review_is_not_escalated(pool: PgPool) {
    let org = "org-esc-future";
    let (pid, task) = seed_participant_task(&pool, org, "future").await;

    let review_id =
        insert_review(&pool, org, &task, &pid, "in_review", Some(Utc::now() + Duration::hours(23)), None).await;

    let escalated = worker(&pool).tick().await.expect("tick");
    assert_eq!(escalated, 0);

    let (_, escalated_at) = read_review(&pool, &review_id).await;
    assert!(escalated_at.is_none());
}

/// A NULL-due review is never escalated even though it is old.
#[sqlx::test(migrations = "./migrations")]
async fn null_due_review_is_not_escalated(pool: PgPool) {
    let org = "org-esc-null";
    let (pid, task) = seed_participant_task(&pool, org, "null").await;

    let review_id = insert_review(&pool, org, &task, &pid, "pending", None, None).await;

    let escalated = worker(&pool).tick().await.expect("tick");
    assert_eq!(escalated, 0, "NULL due_at must never escalate");

    let (_, escalated_at) = read_review(&pool, &review_id).await;
    assert!(escalated_at.is_none());
}

/// (c) Terminal review (approved) past-due → never escalated.
#[sqlx::test(migrations = "./migrations")]
async fn terminal_review_is_not_escalated(pool: PgPool) {
    let org = "org-esc-terminal";
    let (pid, task) = seed_participant_task(&pool, org, "term").await;

    let approved =
        insert_review(&pool, org, &task, &pid, "approved", Some(Utc::now() - Duration::hours(5)), None).await;
    let changes =
        insert_review(&pool, org, &task, &pid, "changes_requested", Some(Utc::now() - Duration::hours(5)), None).await;
    let rejected =
        insert_review(&pool, org, &task, &pid, "rejected", Some(Utc::now() - Duration::hours(5)), None).await;

    let escalated = worker(&pool).tick().await.expect("tick");
    assert_eq!(escalated, 0, "terminal verdicts must never escalate");

    for id in [&approved, &changes, &rejected] {
        let (_, escalated_at) = read_review(&pool, id).await;
        assert!(escalated_at.is_none(), "terminal review {id} must not be escalated");
    }
}

/// (d) Already-escalated review → not re-escalated (idempotent), even when
/// overdue and non-terminal.
#[sqlx::test(migrations = "./migrations")]
async fn already_escalated_review_is_not_re_escalated(pool: PgPool) {
    let org = "org-esc-idem";
    let (pid, task) = seed_participant_task(&pool, org, "idem").await;

    let prior = Utc::now() - Duration::hours(3);
    let review_id =
        insert_review(&pool, org, &task, &pid, "pending", Some(Utc::now() - Duration::hours(5)), Some(prior)).await;

    let escalated = worker(&pool).tick().await.expect("tick");
    assert_eq!(escalated, 0, "already-escalated review must not re-escalate");

    let (_, escalated_at) = read_review(&pool, &review_id).await;
    // Unchanged from the prior timestamp (within a couple seconds of equality).
    let diff = (escalated_at.expect("escalated_at present") - prior).num_seconds().abs();
    assert!(diff < 2, "escalated_at must not be rewritten (diff={diff}s)");
}

/// A second tick over the same data escalates nothing more (idempotent sweep).
#[sqlx::test(migrations = "./migrations")]
async fn second_tick_is_idempotent(pool: PgPool) {
    let org = "org-esc-twice";
    let (pid, task) = seed_participant_task(&pool, org, "twice").await;
    insert_review(&pool, org, &task, &pid, "pending", Some(Utc::now() - Duration::hours(2)), None).await;

    let worker = worker(&pool);
    assert_eq!(worker.tick().await.expect("first tick"), 1);
    assert_eq!(worker.tick().await.expect("second tick"), 0, "nothing left to escalate");
}

/// (e) Multi-org sweep escalates rows across orgs in a single tick.
#[sqlx::test(migrations = "./migrations")]
async fn multi_org_sweep_escalates_all_orgs(pool: PgPool) {
    let org_a = "org-esc-multi-a";
    let org_b = "org-esc-multi-b";
    let (pid_a, task_a) = seed_participant_task(&pool, org_a, "ma").await;
    let (pid_b, task_b) = seed_participant_task(&pool, org_b, "mb").await;

    let a = insert_review(&pool, org_a, &task_a, &pid_a, "pending", Some(Utc::now() - Duration::hours(2)), None).await;
    let b =
        insert_review(&pool, org_b, &task_b, &pid_b, "in_review", Some(Utc::now() - Duration::hours(2)), None).await;

    let escalated = worker(&pool).tick().await.expect("tick");
    assert_eq!(escalated, 2, "one sweep escalates rows from both orgs");

    for id in [&a, &b] {
        let (_, escalated_at) = read_review(&pool, id).await;
        assert!(escalated_at.is_some(), "review {id} must be escalated");
    }
}

/// (f) Batch bound is present in the pinned SQL (`LIMIT $2`) so a large backlog
/// drains over successive ticks rather than in one unbounded transaction.
/// The exhaustive 200-row seed is intentionally NOT created here (cost); the
/// LIMIT clause is enforced by the in-module SQL-pin unit test. This asserts the
/// happy-path multi-row tick still drains everything when under the cap.
#[sqlx::test(migrations = "./migrations")]
async fn batch_drains_small_backlog_in_one_tick(pool: PgPool) {
    let org = "org-esc-batch";
    let (pid, task) = seed_participant_task(&pool, org, "batch").await;

    for _ in 0..5 {
        insert_review(&pool, org, &task, &pid, "pending", Some(Utc::now() - Duration::hours(2)), None).await;
    }

    let escalated = worker(&pool).tick().await.expect("tick");
    assert_eq!(escalated, 5, "all overdue rows under the batch cap escalate in one tick");
}

/// The per-row `review.escalated` audit log is written (best-effort durable trail).
#[sqlx::test(migrations = "./migrations")]
async fn escalation_writes_audit_log(pool: PgPool) {
    let org = "org-esc-audit";
    let (pid, task) = seed_participant_task(&pool, org, "audit").await;
    let review_id =
        insert_review(&pool, org, &task, &pid, "pending", Some(Utc::now() - Duration::hours(2)), None).await;

    let audit_store: Arc<dyn AuditStore> = Arc::new(MemoryAuditStore::new());
    let worker = ReviewEscalationReaperWorker::new(
        pool.clone(),
        Arc::new(Broadcaster::new()),
        Some(audit_store.clone()),
        GRACE_SECS,
        false,
    );

    assert_eq!(worker.tick().await.expect("tick"), 1);

    let (logs, total) = audit_store
        .list(AuditFilter {
            org_id: org.to_string(),
            actor_id: None,
            resource: None,
            resource_id: None,
            action: None,
            from: None,
            to: None,
            limit: 50,
            offset: 0,
        })
        .await
        .expect("list audit logs");

    assert_eq!(total, 1, "exactly one audit log written");
    let log = &logs[0];
    assert_eq!(log.action.as_str(), "review.escalated");
    assert_eq!(log.actor_type, "system");
    assert_eq!(log.actor_id, "system");
    assert_eq!(log.resource, "review");
    assert_eq!(log.resource_id.as_deref(), Some(review_id.as_str()));
}

/// (e, routing) The realtime `review.escalated` event is emitted and routes ONLY
/// to the escalated row's own org — proving the AC#5 per-org routing claim, not
/// just that the DB sweep is multi-org. Subscribe two org receivers BEFORE the
/// tick, then assert each receives exactly its own escalation and nothing from
/// the other tenant.
#[sqlx::test(migrations = "./migrations")]
async fn escalation_broadcasts_per_org_event(pool: PgPool) {
    let org_a = "org-esc-bcast-a";
    let org_b = "org-esc-bcast-b";
    let (pid_a, task_a) = seed_participant_task(&pool, org_a, "ba").await;
    let (pid_b, task_b) = seed_participant_task(&pool, org_b, "bb").await;

    let review_a =
        insert_review(&pool, org_a, &task_a, &pid_a, "pending", Some(Utc::now() - Duration::hours(2)), None).await;
    insert_review(&pool, org_b, &task_b, &pid_b, "in_review", Some(Utc::now() - Duration::hours(2)), None).await;

    let broadcaster = Arc::new(Broadcaster::new());
    // Subscribe before ticking so the broadcast reaches these receivers.
    let (_id_a, mut rx_a) = broadcaster.subscribe(org_a);
    let (_id_b, mut rx_b) = broadcaster.subscribe(org_b);

    let worker = ReviewEscalationReaperWorker::new(pool.clone(), broadcaster.clone(), None, GRACE_SECS, false);
    assert_eq!(worker.tick().await.expect("tick"), 2);

    // org A receives exactly its own escalation, with the correct kind + payload.
    let evt_a = rx_a.try_recv().expect("org A must receive its escalation event");
    assert_eq!(evt_a.kind, "review.escalated");
    assert_eq!(evt_a.org_id, org_a);
    assert_eq!(
        evt_a.payload["reviewId"].as_str(),
        Some(review_a.as_str()),
        "payload must carry the escalated review id"
    );
    assert!(rx_a.try_recv().is_err(), "org A must not receive org B's event (per-org routing)");

    // org B receives its own, exactly once.
    let evt_b = rx_b.try_recv().expect("org B must receive its escalation event");
    assert_eq!(evt_b.kind, "review.escalated");
    assert_eq!(evt_b.org_id, org_b);
    assert!(rx_b.try_recv().is_err(), "org B must receive exactly one event");
}
