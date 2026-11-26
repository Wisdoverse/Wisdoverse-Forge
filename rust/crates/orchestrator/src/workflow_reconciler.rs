//! Workflow execution reconciler (issue #1352).
//!
//! A workflow row says `running` while a Temporal execution stands behind it.
//! After Temporal state is lost or restored from an older backup, a row can
//! claim `running` forever with nothing behind it. This worker asks Temporal
//! about the CURRENT execution of each `running` row and acts on the answer:
//!
//! | Temporal answer                                   | Action                                              |
//! |---------------------------------------------------|-----------------------------------------------------|
//! | an execution exists (running, paused, unknown)    | clear any missing mark                              |
//! | the recorded run closed (completed/failed/...)    | set the matching status, cause `reconciled_from_temporal` |
//! | a different run closed                            | leave alone                                         |
//! | "not found", proven same Temporal, first sight    | record `execution_missing_since`                    |
//! | "not found", proven, missing for the grace period | `failed`, cause `execution_lost` (re-runnable)      |
//! | "not found", not provable (namespace id differs)  | leave alone, count as unverifiable                  |
//! | error or timeout                                  | change nothing, stop the tick, back off             |
//!
//! It never starts, cancels or signals a workflow. Every write is a
//! compare-and-set on organization, status, execution ids and the
//! `updated_at` that was read, so a concurrent legitimate transition always
//! wins. That also keeps it correct when several replicas sweep at once
//! (leader election is off by default); with election on, only the holder of
//! [`crate::leader::WORKFLOW_RECONCILER_LOCK_ID`] sweeps.
//!
//! Every row it answers for gets `execution_checked_at`, and candidates are
//! examined least-recently-checked first, so healthy long-running rows cannot
//! starve the rest. The grace period is a minimum before a lost execution is
//! marked, not a guaranteed recovery time under backlog.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::Postgres;
use sqlx::pool::PoolConnection;
use tokio::time::Instant;

use crate::workflow::{
    CAUSE_EXECUTION_LOST, CAUSE_RECONCILED_FROM_TEMPORAL, ExecutionObservation, MissingMark, Result, Store, Workflow,
    WorkflowRuntime, WorkflowStatus,
};

/// Candidates examined per tick.
pub const RECONCILE_BATCH: usize = 50;
/// Upper bound on one Temporal call.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(5);
/// Upper bound on all Temporal calls in one tick (there are no retries).
pub const TICK_BUDGET: Duration = Duration::from_secs(30);
/// Most ticks skipped after repeated error ticks.
pub const MAX_BACKOFF_TICKS: u32 = 8;

const METRIC: &str = "orchestrator_workflow_reconcile_total";

/// What one tick saw, by outcome. `examined` counts rows Temporal answered for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReconcileOutcome {
    pub examined: u64,
    /// An execution exists; the row was left running.
    pub running: u64,
    /// Ended to match a closed execution.
    pub closed: u64,
    /// Proven missing, still inside the grace period.
    pub missing: u64,
    /// Marked `failed` / `execution_lost`.
    pub lost: u64,
    /// "Not found" that cannot be proven to come from the right Temporal.
    pub unverifiable: u64,
    /// A different run closed, or the row changed while it was checked.
    pub left_alone: u64,
    /// Temporal could not be asked (the tick stopped there).
    pub errors: u64,
}

/// Ticks to skip after `consecutive_error_ticks` error ticks in a row:
/// 1, 2, 4, 8, then capped at [`MAX_BACKOFF_TICKS`].
pub fn backoff_ticks(consecutive_error_ticks: u32) -> u32 {
    match consecutive_error_ticks {
        0 => 0,
        n => 1u32.checked_shl(n - 1).unwrap_or(u32::MAX).min(MAX_BACKOFF_TICKS),
    }
}

pub struct WorkflowReconcilerWorker {
    store: Arc<dyn Store>,
    runtime: Arc<dyn WorkflowRuntime>,
    /// Pool for the leader lock; `None` when leader election is off.
    leader_pool: Option<PgPool>,
    /// Configured Temporal namespace name, for the operator-facing warning.
    namespace: String,
    grace_secs: u64,
    interval: Duration,
}

impl WorkflowReconcilerWorker {
    /// Create a reconciler. The sweep interval is 60 seconds.
    pub fn new(
        store: Arc<dyn Store>,
        runtime: Arc<dyn WorkflowRuntime>,
        leader_pool: Option<PgPool>,
        namespace: String,
        grace_secs: u64,
    ) -> Self {
        Self { store, runtime, leader_pool, namespace, grace_secs, interval: Duration::from_secs(60) }
    }

    /// Run the loop. Each tick commits row by row, so an abrupt stop loses no
    /// state; the next tick simply asks again.
    pub async fn run(self) {
        metrics::describe_counter!(
            METRIC,
            "Workflow rows checked against Temporal by the execution reconciler, labelled by outcome."
        );
        tracing::info!(
            grace_secs = self.grace_secs,
            interval_secs = self.interval.as_secs(),
            namespace = %self.namespace,
            "workflow execution reconciler started"
        );
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut leader_conn: Option<PoolConnection<Postgres>> = None;
        let mut error_ticks: u32 = 0;
        let mut skip: u32 = 0;
        loop {
            ticker.tick().await;
            if skip > 0 {
                skip -= 1;
                continue;
            }
            let Some(result) = self.tick_as_leader(&mut leader_conn).await else {
                continue;
            };
            match result {
                Ok(outcome) if outcome.errors == 0 => error_ticks = 0,
                Ok(outcome) => {
                    error_ticks += 1;
                    skip = backoff_ticks(error_ticks);
                    tracing::warn!(
                        examined = outcome.examined,
                        error_ticks,
                        skip_ticks = skip,
                        namespace = %self.namespace,
                        "workflow reconciler could not ask Temporal; nothing was changed for the remaining rows"
                    );
                }
                Err(err) => {
                    error_ticks += 1;
                    skip = backoff_ticks(error_ticks);
                    if error_ticks >= 3 {
                        tracing::error!(error = %err, error_ticks, "workflow reconciler tick failed repeatedly");
                    } else {
                        tracing::warn!(error = %err, error_ticks, "workflow reconciler tick failed");
                    }
                }
            }
        }
    }

    /// One tick gated by leader election. `None` = skipped because another
    /// replica is leader or the lock could not be checked.
    pub async fn tick_as_leader(
        &self,
        slot: &mut Option<PoolConnection<Postgres>>,
    ) -> Option<Result<ReconcileOutcome>> {
        if let Some(pool) = &self.leader_pool {
            match crate::leader::ensure_leader(pool, crate::leader::WORKFLOW_RECONCILER_LOCK_ID, slot).await {
                crate::leader::LeaderStatus::Leader => {}
                crate::leader::LeaderStatus::NotLeader => {
                    tracing::debug!("workflow reconciler: another replica is leader; skipping tick");
                    return None;
                }
                crate::leader::LeaderStatus::LockError(err) => {
                    tracing::warn!(error = ?err, "workflow reconciler: leader-lock check failed; skipping tick");
                    return None;
                }
            }
        }
        Some(self.tick().await)
    }

    /// One bounded pass over at most [`RECONCILE_BATCH`] candidates.
    pub async fn tick(&self) -> Result<ReconcileOutcome> {
        let deadline = Instant::now() + TICK_BUDGET;
        let candidates = self.store.reconcile_candidates(RECONCILE_BATCH).await?;
        let mut outcome = ReconcileOutcome::default();
        for workflow in &candidates {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let observation = match tokio::time::timeout(
                CALL_TIMEOUT.min(remaining),
                self.runtime.observe_execution(workflow),
            )
            .await
            {
                Ok(Ok(observation)) => observation,
                Ok(Err(err)) => {
                    outcome.errors += 1;
                    tracing::warn!(error = %err, workflow_id = %workflow.id, "workflow reconciler: Temporal lookup failed");
                    break;
                }
                Err(_) => {
                    outcome.errors += 1;
                    tracing::warn!(workflow_id = %workflow.id, "workflow reconciler: Temporal lookup timed out");
                    break;
                }
            };
            outcome.examined += 1;
            self.apply(workflow, observation, &mut outcome).await?;
        }

        if outcome.examined > 0 && outcome.missing + outcome.lost + outcome.unverifiable == outcome.examined {
            // A wrong namespace and a real loss look the same from here.
            tracing::warn!(
                examined = outcome.examined,
                missing = outcome.missing,
                lost = outcome.lost,
                unverifiable = outcome.unverifiable,
                namespace = %self.namespace,
                "every workflow examined has no execution in Temporal; check the orchestrator points at the right Temporal namespace"
            );
        }
        for (label, count) in [
            ("running", outcome.running),
            ("closed", outcome.closed),
            ("missing", outcome.missing),
            ("lost", outcome.lost),
            ("unverifiable", outcome.unverifiable),
            ("left_alone", outcome.left_alone),
            ("error", outcome.errors),
        ] {
            if count > 0 {
                metrics::counter!(METRIC, "outcome" => label).increment(count);
            }
        }
        Ok(outcome)
    }

    async fn apply(
        &self,
        workflow: &Workflow,
        observation: ExecutionObservation,
        outcome: &mut ReconcileOutcome,
    ) -> Result<()> {
        match observation {
            ExecutionObservation::Running => {
                self.store.record_execution_check(workflow, MissingMark::Clear).await?;
                outcome.running += 1;
            }
            ExecutionObservation::Closed { run_id, status }
                if workflow.temporal_run_id.as_deref() == Some(run_id.as_str()) =>
            {
                if self.store.reconcile_status(workflow, status, CAUSE_RECONCILED_FROM_TEMPORAL, None).await? {
                    log_reconciled(workflow, status, CAUSE_RECONCILED_FROM_TEMPORAL);
                    outcome.closed += 1;
                } else {
                    outcome.left_alone += 1;
                }
            }
            // The current run is not the recorded one (reset, retry or a
            // successor): its outcome is not proof about this row.
            ExecutionObservation::Closed { .. } => {
                self.store.record_execution_check(workflow, MissingMark::Keep).await?;
                outcome.left_alone += 1;
            }
            // First sight only marks: the grace period starts here.
            ExecutionObservation::Missing if workflow.execution_missing_since.is_none() => {
                if self.store.record_execution_check(workflow, MissingMark::Set).await? {
                    outcome.missing += 1;
                } else {
                    outcome.left_alone += 1;
                }
            }
            ExecutionObservation::Missing => {
                if self
                    .store
                    .reconcile_status(workflow, WorkflowStatus::Failed, CAUSE_EXECUTION_LOST, Some(self.grace_secs))
                    .await?
                {
                    log_reconciled(workflow, WorkflowStatus::Failed, CAUSE_EXECUTION_LOST);
                    outcome.lost += 1;
                } else {
                    self.store.record_execution_check(workflow, MissingMark::Keep).await?;
                    outcome.missing += 1;
                }
            }
            // Only the check time is recorded, so the row rotates to the back
            // of the queue; its status and missing mark stay untouched.
            ExecutionObservation::Unverifiable => {
                self.store.record_execution_check(workflow, MissingMark::Keep).await?;
                outcome.unverifiable += 1;
            }
        }
        Ok(())
    }
}

fn log_reconciled(workflow: &Workflow, status: WorkflowStatus, cause: &str) {
    tracing::info!(
        workflow_id = %workflow.id,
        org_id = %workflow.org_id,
        previous_status = workflow.status.as_str(),
        status = status.as_str(),
        cause,
        "workflow reconciled from Temporal"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_and_is_capped() {
        let ticks: Vec<u32> = (0..7).map(backoff_ticks).collect();
        assert_eq!(ticks, vec![0, 1, 2, 4, 8, 8, 8]);
        assert_eq!(backoff_ticks(u32::MAX), MAX_BACKOFF_TICKS);
    }
}
