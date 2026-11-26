//! Contract tests for the workflow execution reconciler (#1352) against a real
//! Postgres schema, with a fake Temporal behind the `WorkflowRuntime` trait.
//! Raw Temporal answers are classified by unit tests in `workflow/temporal.rs`;
//! here the fake returns the classified observation.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use sqlx::{PgPool, Row};
use tokio::sync::Mutex;

use agentforge_orchestrator::workflow::{
    ExecutionObservation, ExecutionState, MemoryWorkflowRuntime, PgWorkflowStore, Result as WfResult, SignalRequest,
    Store, Workflow, WorkflowError, WorkflowNode, WorkflowRuntime, WorkflowService, WorkflowStatus,
};
use agentforge_orchestrator::workflow_reconciler::{RECONCILE_BATCH, RuntimeConnector, WorkflowReconcilerWorker};
use futures::future::BoxFuture;

const ORG: &str = "org-reconcile";
const GRACE_SECS: u64 = 600;

/// Fake Temporal: answers per Temporal workflow id (default for the rest),
/// counts calls, and can make a legitimate status change while it is asked.
struct FakeTemporal {
    answers: Mutex<HashMap<String, Option<ExecutionObservation>>>,
    default: Option<ExecutionObservation>,
    calls: AtomicUsize,
    interfere: Option<(Arc<dyn Store>, WorkflowStatus)>,
}

impl FakeTemporal {
    /// `None` = Temporal cannot be asked.
    fn answering(default: Option<ExecutionObservation>) -> Self {
        Self { answers: Mutex::new(HashMap::new()), default, calls: AtomicUsize::new(0), interfere: None }
    }
}

#[async_trait::async_trait]
impl WorkflowRuntime for FakeTemporal {
    fn kind(&self) -> &'static str {
        "fake-temporal"
    }
    async fn start_workflow(&self, _w: &Workflow, _n: &[WorkflowNode], _l: &[Vec<String>]) -> WfResult<()> {
        panic!("the reconciler must never start a workflow")
    }
    async fn cancel_workflow(&self, _w: &Workflow) -> WfResult<()> {
        panic!("the reconciler must never cancel a workflow")
    }
    async fn signal_workflow(&self, _w: &Workflow, _s: SignalRequest, _l: &[Vec<String>]) -> WfResult<()> {
        panic!("the reconciler must never signal a workflow")
    }
    async fn observe_execution(&self, workflow: &Workflow) -> WfResult<ExecutionObservation> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some((store, status)) = &self.interfere {
            store.update_status(&workflow.id, &workflow.org_id, *status).await?;
        }
        let key = workflow.temporal_workflow_id.clone().unwrap_or_default();
        let answer = self.answers.lock().await.get(&key).cloned().unwrap_or_else(|| self.default.clone());
        answer.ok_or_else(|| WorkflowError::Unavailable("temporal unreachable".to_string()))
    }
}

async fn seed_participant(pool: &PgPool) -> String {
    sqlx::query_scalar(
        "INSERT INTO participants (type, display_name, casdoor_user_id, org_id)
         VALUES ('human', 'reconcile-user', 'casdoor-reconcile', $1)
         RETURNING id::text",
    )
    .bind(ORG)
    .fetch_one(pool)
    .await
    .expect("seed participant")
}

/// A `running` row with execution `orchestrator-{name}` / `run-1` started in
/// namespace `ns-1`, last updated an hour ago.
async fn seed_running(pool: &PgPool, participant: &str, name: &str) -> String {
    sqlx::query_scalar(
        "INSERT INTO workflows (name, status, org_id, created_by, temporal_workflow_id, temporal_run_id,
                                temporal_namespace_id, updated_at)
         VALUES ($1, 'running', $2, CAST($3 AS uuid), 'orchestrator-' || $1, 'run-1', 'ns-1',
                 NOW() - INTERVAL '1 hour')
         RETURNING id::text",
    )
    .bind(name)
    .bind(ORG)
    .bind(participant)
    .fetch_one(pool)
    .await
    .expect("seed running workflow")
}

async fn set_missing_since(pool: &PgPool, id: &str, minutes_ago: i32) {
    sqlx::query(
        "UPDATE workflows SET execution_missing_since = NOW() - make_interval(mins => $2) WHERE id = CAST($1 AS uuid)",
    )
    .bind(id)
    .bind(minutes_ago)
    .execute(pool)
    .await
    .expect("set missing since");
}

/// Every column the reconciler could touch, as text, for before/after checks.
async fn snapshot(pool: &PgPool, id: &str) -> String {
    sqlx::query(
        "SELECT concat_ws('|', status, status_cause, execution_missing_since, execution_checked_at, reconciled_at,
                          updated_at, temporal_run_id, execution_observation, state_version) AS snap
           FROM workflows WHERE id = CAST($1 AS uuid)",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("snapshot")
    .get("snap")
}

fn connector(fake: Arc<FakeTemporal>) -> RuntimeConnector {
    Arc::new(move || -> BoxFuture<'static, WfResult<Arc<dyn WorkflowRuntime>>> {
        let fake = fake.clone();
        Box::pin(async move { Ok(fake as Arc<dyn WorkflowRuntime>) })
    })
}

fn worker_with(pool: &PgPool, connect: RuntimeConnector, leader: bool) -> WorkflowReconcilerWorker {
    let store: Arc<dyn Store> = Arc::new(PgWorkflowStore::new(pool.clone()));
    WorkflowReconcilerWorker::new(
        store,
        connect,
        leader.then(|| pool.clone()),
        "orchestrator".to_string(),
        GRACE_SECS,
        false,
    )
}

fn worker(pool: &PgPool, fake: Arc<FakeTemporal>, leader: bool) -> WorkflowReconcilerWorker {
    worker_with(pool, connector(fake), leader)
}

async fn load(pool: &PgPool, id: &str) -> Workflow {
    PgWorkflowStore::new(pool.clone()).get_by_id(id, ORG).await.expect("load workflow")
}

#[sqlx::test(migrations = "./migrations")]
async fn missing_execution_first_sight_marks_only(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "first-sight").await;
    let fake = Arc::new(FakeTemporal::answering(Some(ExecutionObservation::Missing)));

    let outcome = worker(&pool, fake, false).tick().await.expect("tick");

    assert_eq!(outcome.missing, 1);
    assert_eq!(outcome.lost, 0);
    let after = load(&pool, &id).await;
    assert_eq!(after.status, WorkflowStatus::Running, "first sight must not end the workflow");
    assert!(after.execution_missing_since.is_some(), "first sight records when it went missing");
    assert_eq!(after.execution_observation.as_deref(), Some("missing"));
    assert!(after.status_cause.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn missing_inside_grace_period_stays_running(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "inside-grace").await;
    set_missing_since(&pool, &id, 1).await;
    let fake = Arc::new(FakeTemporal::answering(Some(ExecutionObservation::Missing)));

    let outcome = worker(&pool, fake, false).tick().await.expect("tick");

    assert_eq!((outcome.missing, outcome.lost), (1, 0));
    assert_eq!(load(&pool, &id).await.status, WorkflowStatus::Running);
}

#[sqlx::test(migrations = "./migrations")]
async fn missing_after_grace_period_fails_with_execution_lost_and_can_be_rerun(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "lost").await;
    set_missing_since(&pool, &id, 11).await;
    let fake = Arc::new(FakeTemporal::answering(Some(ExecutionObservation::Missing)));

    let outcome = worker(&pool, fake, false).tick().await.expect("tick");

    assert_eq!(outcome.lost, 1);
    let after = load(&pool, &id).await;
    assert_eq!(after.status, WorkflowStatus::Failed);
    assert_eq!(after.status_cause.as_deref(), Some("execution_lost"));
    assert!(after.reconciled_at.is_some());
    assert!(after.execution_missing_since.is_some(), "the operator can still see since when it was missing");

    // `failed` is re-runnable; the re-run clears the reconciliation metadata.
    sqlx::query(
        "INSERT INTO workflow_nodes (workflow_id, name, type, position) VALUES (CAST($1 AS uuid), 'n1', 'gate', 0)",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .expect("seed node");
    let store: Arc<dyn Store> = Arc::new(PgWorkflowStore::new(pool.clone()));
    let service = WorkflowService::new(store.clone(), Arc::new(MemoryWorkflowRuntime::new(store)));
    service.start_workflow(&id, ORG).await.expect("re-run after execution_lost");
    let rerun = load(&pool, &id).await;
    assert_ne!(rerun.status, WorkflowStatus::Failed);
    assert!(rerun.status_cause.is_none() && rerun.execution_missing_since.is_none() && rerun.reconciled_at.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn present_and_running_execution_is_unchanged_and_mark_cleared(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "present").await;
    set_missing_since(&pool, &id, 30).await;
    let before = load(&pool, &id).await;
    let fake = Arc::new(FakeTemporal::answering(Some(ExecutionObservation::Running)));

    let outcome = worker(&pool, fake, false).tick().await.expect("tick");

    assert_eq!(outcome.running, 1);
    let after = load(&pool, &id).await;
    assert_eq!(after.status, WorkflowStatus::Running);
    assert!(after.execution_missing_since.is_none(), "a present execution clears the missing mark");
    assert_eq!(after.execution_observation.as_deref(), Some("running"));
    assert!(after.execution_checked_at.is_some());
    assert_eq!(after.state_version, before.state_version, "bookkeeping never bumps the version");
    assert_eq!(after.updated_at, before.updated_at, "a present execution is never transitioned");
    assert!(after.status_cause.is_none() && after.reconciled_at.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn closed_recorded_run_sets_matching_terminal_status(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "closed").await;
    let closed = ExecutionObservation::Closed {
        run_id: "run-1".to_string(),
        first_run_id: "run-1".to_string(),
        status: WorkflowStatus::Completed,
    };
    let fake = Arc::new(FakeTemporal::answering(Some(closed)));

    let outcome = worker(&pool, fake, false).tick().await.expect("tick");

    assert_eq!(outcome.closed, 1);
    let after = load(&pool, &id).await;
    assert_eq!(after.status, WorkflowStatus::Completed);
    assert_eq!(after.status_cause.as_deref(), Some("reconciled_from_temporal"));
}

#[sqlx::test(migrations = "./migrations")]
async fn closed_continued_run_of_the_recorded_chain_is_reconciled(pool: PgPool) {
    // Chain A -> B (continue-as-new, retry or reset): the row records run-1 (A),
    // the current execution is run-2 (B), closed, whose first run is run-1.
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "continued-run").await;
    let closed = ExecutionObservation::Closed {
        run_id: "run-2".to_string(),
        first_run_id: "run-1".to_string(),
        status: WorkflowStatus::Completed,
    };
    let fake = Arc::new(FakeTemporal::answering(Some(closed)));

    let outcome = worker(&pool, fake, false).tick().await.expect("tick");

    assert_eq!(outcome.closed, 1);
    let after = load(&pool, &id).await;
    assert_eq!(after.status, WorkflowStatus::Completed, "a closed run of the recorded chain ends the row");
    assert_eq!(after.status_cause.as_deref(), Some("reconciled_from_temporal"));
}

#[sqlx::test(migrations = "./migrations")]
async fn closed_run_of_a_different_chain_is_left_alone(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "other-chain").await;
    let closed = ExecutionObservation::Closed {
        run_id: "run-9".to_string(),
        first_run_id: "run-9".to_string(),
        status: WorkflowStatus::Failed,
    };
    let fake = Arc::new(FakeTemporal::answering(Some(closed)));

    let outcome = worker(&pool, fake, false).tick().await.expect("tick");

    assert_eq!(outcome.left_alone, 1);
    let after = load(&pool, &id).await;
    assert_eq!(after.status, WorkflowStatus::Running);
    assert!(after.status_cause.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn temporal_unreachable_changes_nothing(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let first = seed_running(&pool, &participant, "unreachable-a").await;
    let second = seed_running(&pool, &participant, "unreachable-b").await;
    set_missing_since(&pool, &second, 60).await;
    let before = (snapshot(&pool, &first).await, snapshot(&pool, &second).await);
    let fake = Arc::new(FakeTemporal::answering(None));

    let outcome = worker(&pool, fake.clone(), false).tick().await.expect("tick");

    assert_eq!(outcome.errors, 1);
    assert_eq!(outcome.examined, 0);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1, "the tick stops at the first error");
    assert_eq!((snapshot(&pool, &first).await, snapshot(&pool, &second).await), before, "no column may change");
}

#[sqlx::test(migrations = "./migrations")]
async fn unverifiable_not_found_is_never_marked(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "unverifiable").await;
    set_missing_since(&pool, &id, 60).await;
    let fake = Arc::new(FakeTemporal::answering(Some(ExecutionObservation::Unverifiable)));

    let outcome = worker(&pool, fake, false).tick().await.expect("tick");

    assert_eq!(outcome.unverifiable, 1);
    let after = load(&pool, &id).await;
    assert_eq!(after.status, WorkflowStatus::Running, "an unprovable miss must not end the workflow");
    assert_eq!(after.execution_observation.as_deref(), Some("unverifiable"));
    assert!(after.status_cause.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_legitimate_transition_wins_the_compare_and_set(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "race").await;
    set_missing_since(&pool, &id, 60).await;
    let store: Arc<dyn Store> = Arc::new(PgWorkflowStore::new(pool.clone()));
    // The finalizer completes the workflow while the reconciler asks Temporal.
    let mut fake = FakeTemporal::answering(Some(ExecutionObservation::Missing));
    fake.interfere = Some((store, WorkflowStatus::Completed));

    let outcome = worker(&pool, Arc::new(fake), false).tick().await.expect("tick");

    assert_eq!(outcome.lost, 0);
    let after = load(&pool, &id).await;
    assert_eq!(after.status, WorkflowStatus::Completed, "the legitimate transition must win");
    assert!(after.status_cause.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn late_finalizer_of_an_earlier_run_cannot_overwrite_a_rerun(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "late-finalizer").await;
    let store = PgWorkflowStore::new(pool.clone());
    // Re-run: a new execution chain is recorded.
    store
        .set_temporal_ids(&id, ORG, "orchestrator-late-finalizer".into(), "run-2".into(), Some("ns-1".into()))
        .await
        .expect("record re-run");

    let applied = store.update_status_for_run(&id, ORG, "run-1", WorkflowStatus::Failed).await.expect("late write");
    assert!(!applied, "the old run's finalizer must be fenced out");
    assert_eq!(load(&pool, &id).await.status, WorkflowStatus::Running);

    let applied = store.update_status_for_run(&id, ORG, "run-2", WorkflowStatus::Completed).await.expect("write");
    assert!(applied);
    assert_eq!(load(&pool, &id).await.status, WorkflowStatus::Completed);
}

#[sqlx::test(migrations = "./migrations")]
async fn batch_is_bounded_and_unchecked_rows_go_first(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let total = RECONCILE_BATCH + 10;
    for index in 0..total {
        seed_running(&pool, &participant, &format!("batch-{index}")).await;
    }
    let fake = Arc::new(FakeTemporal::answering(Some(ExecutionObservation::Running)));
    let worker = worker(&pool, fake, false);

    let first = worker.tick().await.expect("first tick");
    assert_eq!(first.examined, RECONCILE_BATCH as u64, "one tick examines at most the batch");

    let unchecked: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM workflows WHERE status = 'running' AND execution_checked_at IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unchecked, 10);
    worker.tick().await.expect("second tick");
    let unchecked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workflows WHERE execution_checked_at IS NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(unchecked, 0, "rows never checked go before rows checked last tick");
}

#[sqlx::test(migrations = "./migrations")]
async fn tick_is_skipped_when_another_replica_holds_the_leader_lock(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "not-leader").await;
    set_missing_since(&pool, &id, 60).await;
    let before = snapshot(&pool, &id).await;
    // Another replica holds the reconciler's lock (class "ORCH", id 3).
    let mut other = pool.acquire().await.unwrap();
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
        .bind(0x4f52_4348_i32)
        .bind(3_i32)
        .fetch_one(&mut *other)
        .await
        .unwrap();
    assert!(locked);
    let fake = Arc::new(FakeTemporal::answering(Some(ExecutionObservation::Missing)));

    let mut slot = None;
    let result = worker(&pool, fake.clone(), true).tick_as_leader(&mut slot).await;

    assert!(result.is_none(), "a non-leader must skip the tick");
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0, "a non-leader must not ask Temporal");
    assert_eq!(snapshot(&pool, &id).await, before);
}

#[sqlx::test(migrations = "./migrations")]
async fn status_read_reports_execution_state_and_only_sets_the_missing_mark(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "status-read").await;
    let store: Arc<dyn Store> = Arc::new(PgWorkflowStore::new(pool.clone()));

    // Temporal cannot be asked: the read still succeeds and changes nothing.
    let before = snapshot(&pool, &id).await;
    let service = WorkflowService::new(store.clone(), Arc::new(FakeTemporal::answering(None)));
    let (workflow, _nodes, state) = service.get_status(&id, ORG).await.expect("status read");
    assert_eq!(state, Some(ExecutionState::Unknown));
    assert_eq!(workflow.status, WorkflowStatus::Running);
    assert_eq!(snapshot(&pool, &id).await, before);

    // Proven missing: reported, and the first miss is recorded (status unchanged).
    let service = WorkflowService::new(store, Arc::new(FakeTemporal::answering(Some(ExecutionObservation::Missing))));
    let (workflow, _nodes, state) = service.get_status(&id, ORG).await.expect("status read");
    assert_eq!(state, Some(ExecutionState::Missing));
    assert_eq!(workflow.status, WorkflowStatus::Running);
    assert!(workflow.execution_missing_since.is_some());
    // Reads never move the reconciler's fairness timestamp, so polling cannot
    // keep a lost workflow behind the other candidates.
    assert!(workflow.execution_checked_at.is_none(), "a status read must not write execution_checked_at");

    // Polling again writes nothing at all.
    let polled = snapshot(&pool, &id).await;
    for _ in 0..3 {
        service.get_status(&id, ORG).await.expect("status read");
    }
    assert_eq!(snapshot(&pool, &id).await, polled, "repeated reads must not write");
}

#[sqlx::test(migrations = "./migrations")]
async fn temporal_down_at_start_gives_error_ticks_then_normal_ticks(pool: PgPool) {
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "lazy-connect").await;
    set_missing_since(&pool, &id, 60).await;
    let before = snapshot(&pool, &id).await;
    let attempts = Arc::new(AtomicUsize::new(0));
    let fake = Arc::new(FakeTemporal::answering(Some(ExecutionObservation::Running)));
    let connect: RuntimeConnector = {
        let attempts = attempts.clone();
        Arc::new(move || -> BoxFuture<'static, WfResult<Arc<dyn WorkflowRuntime>>> {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
            let fake = fake.clone();
            Box::pin(async move {
                if attempt < 2 {
                    Err(WorkflowError::Unavailable("connection refused".to_string()))
                } else {
                    Ok(fake as Arc<dyn WorkflowRuntime>)
                }
            })
        })
    };
    let worker = worker_with(&pool, connect, false);

    for _ in 0..2 {
        let outcome = worker.tick().await.expect("tick");
        assert_eq!((outcome.errors, outcome.examined), (1, 0), "Temporal down is an error tick");
        assert_eq!(snapshot(&pool, &id).await, before, "nothing changes while Temporal is down");
    }
    let outcome = worker.tick().await.expect("tick");
    assert_eq!((outcome.errors, outcome.running), (0, 1), "once connected, ticks are normal");
    let outcome = worker.tick().await.expect("tick");
    assert_eq!(outcome.errors, 0);
    assert_eq!(attempts.load(Ordering::SeqCst), 3, "a connected runtime is reused");
    assert!(load(&pool, &id).await.execution_missing_since.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn fenced_status_write_cannot_overwrite_a_rerun(pool: PgPool) {
    // P1-1 at the store level: a delayed successful cancel of run-1 arrives
    // after a re-run recorded run-2.
    let participant = seed_participant(&pool).await;
    let id = seed_running(&pool, &participant, "late-cancel").await;
    let store = PgWorkflowStore::new(pool.clone());
    let read_before_rerun = load(&pool, &id).await;
    store
        .set_temporal_ids(&id, ORG, "orchestrator-late-cancel".into(), "run-2".into(), Some("ns-1".into()))
        .await
        .expect("record re-run");
    let rerun = snapshot(&pool, &id).await;

    let applied =
        store.set_status_if_unchanged(&read_before_rerun, WorkflowStatus::Cancelled).await.expect("fenced write");

    assert!(!applied);
    assert_eq!(snapshot(&pool, &id).await, rerun, "the row keeps run-2's state");
    let current = load(&pool, &id).await;
    assert!(store.set_status_if_unchanged(&current, WorkflowStatus::Cancelled).await.unwrap());
    assert_eq!(load(&pool, &id).await.status, WorkflowStatus::Cancelled);
}
