//! Overdue review escalation reaper (issue #871, follow-up to #801).
//!
//! #801 shipped `code_reviews.due_at` and an "overdue" dashboard count, but
//! nothing *acts* when a review blows its SLA — an overdue review sits silent
//! until a human happens to look. This worker sweeps every tenant on an
//! interval and, once a review is past `due_at` by a grace window, flags it
//! (`escalated_at = NOW()`), writes a `review.escalated` audit log, and pushes a
//! realtime `review.escalated` event so dashboards/inboxes light up.
//!
//! Non-destructive contract: the reaper NEVER changes `code_reviews.state`.
//! Verdict transitions stay 100% human/MCP-driven. Escalation is recorded only
//! in the nullable `escalated_at` column, which is also the idempotency guard —
//! a review escalates exactly once, ever.
//!
//! Durability ordering: the UPDATE commits `escalated_at` first (the durable
//! source of truth — dashboards can always `SELECT ... WHERE escalated_at IS NOT
//! NULL`), THEN the realtime broadcast + audit write run best-effort from the
//! `RETURNING` rows. A crash between the two leaves the flag set with no event,
//! which is recoverable because the flag is queryable; events are a best-effort
//! overlay, never the source of truth.
//!
//! Observability: the orchestrator does not install a Prometheus recorder (its
//! `/metrics` surface is query-based dashboard JSON, not a scrape endpoint), so
//! this reaper surfaces activity via `tracing` warnings (`escalated = N`) plus a
//! per-row `review.escalated` audit log, not a Prometheus counter that would
//! silently no-op.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::audit::{AuditAction, AuditLog, Store as AuditStore};
use crate::realtime::{Broadcaster, Event};

/// SQL that escalates overdue, non-terminal, not-yet-escalated reviews.
///
/// Kept as a `pub(crate) const` so the SQL-pin unit test can assert the scope
/// predicates without a live database.
///
/// The outer UPDATE **re-checks** `state` + `escalated_at` (not just the inner
/// SELECT): a reviewer's `apply_verdict` transaction can commit between the inner
/// `SELECT ... FOR UPDATE SKIP LOCKED` and the outer write; re-checking under the
/// row lock closes that escalate-after-approve TOCTOU so a just-approved review
/// is never escalated.
///
/// `$1` = grace secs past `due_at`, bound as `f64` (`make_interval(secs => $1)`
/// needs a float). `$2` = batch limit.
pub(crate) const REAP_OVERDUE_REVIEWS_SQL: &str = "
    UPDATE code_reviews
    SET escalated_at = NOW()
    WHERE id IN (
        SELECT id FROM code_reviews
        WHERE state IN ('pending', 'in_review')
          AND escalated_at IS NULL
          AND due_at IS NOT NULL
          AND due_at < NOW() - make_interval(secs => $1)
        ORDER BY due_at
        LIMIT $2
        FOR UPDATE SKIP LOCKED
    )
      AND state IN ('pending', 'in_review')
      AND escalated_at IS NULL
    RETURNING id, org_id, task_id, due_at
";

/// Maximum reviews escalated per tick. Bounds a backlog burst so one sweep never
/// holds a long transaction or floods the broadcast/audit path; a large backlog
/// drains over successive 60-second ticks.
///
// ponytail: 200/tick caps a backlog burst; raise or add a config knob if
// escalation latency on huge backlogs ever matters.
const ESCALATION_BATCH: i64 = 200;

/// One escalated review, as returned by [`REAP_OVERDUE_REVIEWS_SQL`].
struct EscalatedReview {
    id: Uuid,
    org_id: String,
    task_id: Uuid,
    due_at: Option<DateTime<Utc>>,
}

pub struct ReviewEscalationReaperWorker {
    pool: PgPool,
    broadcaster: Arc<Broadcaster>,
    audit_store: Option<Arc<dyn AuditStore>>,
    grace_secs: u64,
    interval: Duration,
    /// CN-6: when true, each tick runs only if this replica holds the
    /// review-escalation reaper's advisory lock; otherwise it is skipped.
    leader_election_enabled: bool,
}

impl ReviewEscalationReaperWorker {
    /// Create a new reaper. The sweep interval defaults to 60 seconds.
    pub fn new(
        pool: PgPool,
        broadcaster: Arc<Broadcaster>,
        audit_store: Option<Arc<dyn AuditStore>>,
        grace_secs: u64,
        leader_election_enabled: bool,
    ) -> Self {
        Self { pool, broadcaster, audit_store, grace_secs, interval: Duration::from_secs(60), leader_election_enabled }
    }

    /// Run the reaper loop. The orchestrator has no shutdown watch channel — the
    /// process exit stops the spawned task naturally, and each tick commits its
    /// `escalated_at` flips durably before any best-effort overlay, so an abrupt
    /// stop loses no source-of-truth state.
    pub async fn run(self) {
        tracing::info!(
            grace_secs = self.grace_secs,
            interval_secs = self.interval.as_secs(),
            "review escalation reaper started"
        );
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut consecutive_failures: u32 = 0;
        // CN-6: while this replica stays leader, this slot keeps the advisory-lock
        // connection across ticks (see leader::ensure_leader).
        let mut leader_conn: Option<sqlx::pool::PoolConnection<sqlx::Postgres>> = None;
        loop {
            ticker.tick().await;
            // CN-6: under multiple replicas, only the elected leader runs the
            // sweep. Default-off → single-replica runs the tick directly.
            if self.leader_election_enabled {
                match crate::leader::ensure_leader(
                    &self.pool,
                    crate::leader::REVIEW_ESCALATION_REAPER_LOCK_ID,
                    &mut leader_conn,
                )
                .await
                {
                    crate::leader::LeaderStatus::Leader => {}
                    crate::leader::LeaderStatus::NotLeader => {
                        tracing::debug!("review escalation reaper: another replica is leader; skipping tick");
                        continue;
                    }
                    crate::leader::LeaderStatus::LockError(err) => {
                        tracing::warn!(error = ?err, "review escalation reaper: leader-lock check failed; skipping tick");
                        continue;
                    }
                }
            }
            match self.tick().await {
                Ok(0) => {
                    consecutive_failures = 0;
                }
                Ok(n) => {
                    consecutive_failures = 0;
                    tracing::warn!(
                        escalated = n,
                        grace_secs = self.grace_secs,
                        "review escalation reaper escalated overdue reviews past their SLA grace window"
                    );
                }
                Err(err) => {
                    consecutive_failures += 1;
                    if consecutive_failures >= 3 {
                        tracing::error!(
                            error = ?err,
                            consecutive_failures,
                            "review escalation reaper tick failed repeatedly — overdue reviews are not being escalated"
                        );
                    } else {
                        tracing::warn!(error = ?err, consecutive_failures, "review escalation reaper tick failed");
                    }
                }
            }
        }
    }

    /// Single-shot escalation pass. Returns the number of reviews escalated.
    ///
    /// The escalating UPDATE commits first (durable source of truth); then the
    /// realtime broadcast + audit write run best-effort per row (log-and-continue
    /// on error) so a failed overlay never fails the tick or re-escalates a row.
    pub async fn tick(&self) -> sqlx::Result<u64> {
        let rows = sqlx::query(REAP_OVERDUE_REVIEWS_SQL)
            .bind(self.grace_secs as f64)
            .bind(ESCALATION_BATCH)
            .fetch_all(&self.pool)
            .await?;

        let escalated: Vec<EscalatedReview> = rows
            .iter()
            .map(|row| {
                Ok(EscalatedReview {
                    id: row.try_get("id")?,
                    org_id: row.try_get("org_id")?,
                    task_id: row.try_get("task_id")?,
                    due_at: row.try_get("due_at")?,
                })
            })
            .collect::<sqlx::Result<Vec<_>>>()?;

        let now = Utc::now();
        for review in &escalated {
            // overdue_secs: how long past due_at this escalation fired. NULL due_at
            // can never be selected by the SQL (`due_at IS NOT NULL`), so the None
            // arm here is unreachable in practice but kept total rather than panic.
            let overdue_secs = review.due_at.map(|due| (now - due).num_seconds().max(0));

            self.broadcaster.broadcast(Event {
                kind: "review.escalated".into(),
                org_id: review.org_id.clone(),
                payload: json!({
                    "reviewId": review.id,
                    "taskId": review.task_id,
                    "dueAt": review.due_at,
                    "overdueSecs": overdue_secs,
                }),
            });

            if let Some(store) = &self.audit_store {
                let mut log = AuditLog {
                    id: String::new(),
                    action: AuditAction::ReviewEscalated,
                    actor_id: "system".to_string(),
                    actor_type: "system".to_string(),
                    resource: "review".to_string(),
                    resource_id: Some(review.id.to_string()),
                    org_id: review.org_id.clone(),
                    changes: None,
                    ip_address: None,
                    user_agent: None,
                    created_at: now,
                };
                if let Err(err) = store.create(&mut log).await {
                    // Best-effort: the escalation is already committed (queryable via
                    // escalated_at). Log and continue rather than fail the tick.
                    tracing::warn!(
                        error = ?err,
                        review_id = %review.id,
                        org_id = %review.org_id,
                        "review escalation reaper failed to write audit log — escalation flag is still set"
                    );
                }
            }
        }

        Ok(escalated.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::REAP_OVERDUE_REVIEWS_SQL;

    /// Pin the SQL scope predicates so a future "simplification" that widens the
    /// WHERE clause (escalating terminal/approved rows), drops the TOCTOU
    /// re-check, or — worst — writes `state` fails before review.
    #[test]
    fn reap_reviews_sql_pins_correct_predicates() {
        let sql = REAP_OVERDUE_REVIEWS_SQL;
        // Scope: only non-terminal, not-yet-escalated, genuinely overdue rows.
        assert!(sql.contains("state IN ('pending', 'in_review')"));
        assert!(sql.contains("escalated_at IS NULL"));
        assert!(sql.contains("due_at IS NOT NULL"));
        // The TOCTOU guard: the state + escalated_at predicates must appear TWICE
        // — once in the inner `FOR UPDATE SKIP LOCKED` SELECT and once as the
        // OUTER UPDATE re-check that closes the escalate-after-approve race. A
        // plain `contains` can't tell the two apart, so a "simplification" that
        // drops the outer re-check would slip past it; counting catches that.
        assert_eq!(
            sql.matches("state IN ('pending', 'in_review')").count(),
            2,
            "state predicate must appear in BOTH the inner SELECT and the outer re-check"
        );
        assert_eq!(
            sql.matches("escalated_at IS NULL").count(),
            2,
            "escalated_at IS NULL must appear in BOTH the inner SELECT and the outer re-check"
        );
        // Concurrency-safe batched sweep.
        assert!(sql.contains("FOR UPDATE SKIP LOCKED"));
        assert!(sql.contains("LIMIT $2"));
        // The only write is the escalation flag.
        assert!(sql.contains("escalated_at = NOW()"));
        // Returns the rows to broadcast + audit.
        assert!(sql.contains("RETURNING"));
        // Non-destructive: terminal verdicts are never escalated and state is
        // never written.
        assert!(!sql.contains("'approved'"));
        assert!(!sql.contains("'changes_requested'"));
        assert!(!sql.contains("'rejected'"));
        assert!(!sql.contains("SET state"));
        assert!(!sql.contains("state ="));
    }
}
