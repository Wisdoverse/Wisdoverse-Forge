use std::time::{Duration, Instant};

use agentforge_core::RuntimeKind;
use agentforge_core::clone_protocol::{
    CLONE_JOB_MAX_ATTEMPTS, CLONE_JOB_QUEUE, CLONE_OUTBOX_AGGREGATE_TYPE, CloneOutboxPayload,
};
use agentforge_core::orchestration_protocol::{TaskAssignment, assign_subject, assign_subject_kind};
use agentforge_db::entities::OrchestrationOutbox;
use anyhow::{Context, Result};
use async_nats::{Client, jetstream};
use sqlx::{PgPool, Postgres, Transaction};
use tokio::sync::watch;

const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(500);
const ERROR_BACKOFF: Duration = Duration::from_secs(1);

const NEXT_ASSIGNMENT_OUTBOX_SQL: &str = r#"SELECT *
      FROM orchestration_outbox
     WHERE published_at IS NULL
       AND event_type = 'assignment'
     ORDER BY created_at ASC
     FOR UPDATE SKIP LOCKED
     LIMIT 1"#;

/// Next unpublished `project_clone` outbox row (M2 transactional outbox). Distinct
/// from the assignment path by `aggregate_type`; relayed into `job_queue` rather
/// than published to JetStream.
const NEXT_CLONE_OUTBOX_SQL: &str = r#"SELECT *
      FROM orchestration_outbox
     WHERE published_at IS NULL
       AND aggregate_type = $1
     ORDER BY created_at ASC
     FOR UPDATE SKIP LOCKED
     LIMIT 1"#;

const MARK_OUTBOX_PUBLISHED_SQL: &str = r#"UPDATE orchestration_outbox
       SET published_at = NOW()
     WHERE id = $1"#;

pub struct OrchestrationOutboxPublisher {
    pool: PgPool,
    client: Client,
}

impl OrchestrationOutboxPublisher {
    pub fn new(pool: PgPool, client: Client) -> Self {
        Self { pool, client }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_ok() && *shutdown.borrow() {
                        tracing::info!("Orchestration outbox publisher shutting down");
                        break;
                    }
                }
                _ = async {
                    // Drain assignment rows (JetStream) and clone rows (job_queue)
                    // each tick. A tick is "busy" if EITHER did work, so a backlog
                    // on one stream does not starve the other and the loop only
                    // idles when both are empty.
                    let assignment = self.publish_next().await;
                    let clone = self.publish_next_clone().await;
                    match (&assignment, &clone) {
                        (Ok(false), Ok(false)) => tokio::time::sleep(IDLE_POLL_INTERVAL).await,
                        (Ok(_), Ok(_)) => {}
                        _ => {
                            // Tag each failure with WHICH stream it came from and
                            // count it on its OWN axis, so a stuck clone backlog is
                            // observable independently of assignment publishing.
                            if let Err(err) = &assignment {
                                metrics::counter!("agentforge_orchestration_outbox_publish_errors_total").increment(1);
                                tracing::warn!(
                                    error = %err,
                                    stream = "orchestration_assignment",
                                    "orchestration outbox publish failed"
                                );
                            }
                            if let Err(err) = &clone {
                                metrics::counter!("agentforge_project_clone_outbox_relay_errors_total").increment(1);
                                tracing::warn!(
                                    error = %err,
                                    stream = "project_clone",
                                    aggregate_type = CLONE_OUTBOX_AGGREGATE_TYPE,
                                    "project_clone outbox relay failed"
                                );
                            }
                            tokio::time::sleep(ERROR_BACKOFF).await;
                        }
                    }
                } => {}
            }
        }
    }

    async fn publish_next(&self) -> Result<bool> {
        let mut tx = self.pool.begin().await.context("begin outbox tx")?;
        let Some(row) = sqlx::query_as::<_, OrchestrationOutbox>(NEXT_ASSIGNMENT_OUTBOX_SQL)
            .fetch_optional(&mut *tx)
            .await
            .context("select next orchestration outbox row")?
        else {
            tx.commit().await.context("commit empty outbox tx")?;
            return Ok(false);
        };

        let assignment: TaskAssignment =
            serde_json::from_value(row.payload.clone()).context("decode orchestration assignment outbox payload")?;

        // #457 phase 1c: resolve the target agent's runtime_kind for the
        // namespaced subject. The hot auto-dispatch path (participant_liveness)
        // carries it on the assignment; the cold API-dispatch path and pre-1c
        // outbox rows leave it None, so fall back to one indexed PK lookup here
        // (None only on cold/old rows, never the hot path). NOT NULL post-062;
        // a vanished agent degrades to Container — the legacy copy below still
        // covers that agent's old sidecar regardless.
        let kind = match assignment.runtime_kind {
            Some(kind) => kind,
            None => {
                let raw: Option<String> = sqlx::query_scalar("SELECT runtime_kind FROM agents WHERE id = $1")
                    .bind(assignment.agent_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .context("lookup agent runtime_kind for assignment publish")?;
                raw.and_then(|raw| RuntimeKind::parse_legacy(&raw).ok()).unwrap_or_else(|| {
                    // The agent row vanished mid-flight (or holds an unparseable
                    // kind, impossible under the 062 CHECK). Defaulting to
                    // Container is HARMLESS while we still dual-publish the legacy
                    // copy, but after the orchestration.assigned legacy-drop deploy
                    // a wrong-kind namespaced publish would silently strand a
                    // cli/api agent's assignment. Surface it loudly + count it so
                    // the legacy-drop gate can require this metric at zero.
                    tracing::warn!(
                        agent_id = %assignment.agent_id,
                        outbox_id = %row.id,
                        "assignment runtime_kind unresolved at publish; defaulting to Container \
                         — harmless during dual-publish, an assignment-loss vector after legacy-drop"
                    );
                    metrics::counter!("agentforge_orchestration_assignment_kind_fallback_total").increment(1);
                    RuntimeKind::Container
                })
            }
        };

        let legacy_subject = assign_subject(assignment.agent_id);
        let namespaced_subject = assign_subject_kind(kind, assignment.agent_id);
        let bytes = serde_json::to_vec(&assignment).context("encode orchestration assignment payload")?;
        let publish_started = Instant::now();
        let js = jetstream::new(self.client.clone());

        // Dual-publish the SAME payload (same delivery_id) to BOTH the legacy and
        // namespaced subjects during the migration drain. A given agent's
        // per-agent durable filters exactly ONE shape (legacy on pre-1c images,
        // namespaced on new images) and the platform cannot know which image the
        // agent runs, so it must emit both. AssignmentInbox dedups by
        // delivery_id BEFORE the CLI runs, so a sidecar that ever matches both
        // executes the task only once. Mark the row published only after BOTH
        // acks succeed (a partial publish leaves the row unpublished and is
        // retried — a harmless duplicate the inbox dedups). The legacy-drop
        // deploy later switches this to namespaced-only.
        for subject in [&legacy_subject, &namespaced_subject] {
            js.publish(subject.clone(), bytes.clone().into())
                .await
                .with_context(|| format!("publish assignment outbox row {} to {subject}", row.id))?
                .await
                .with_context(|| format!("await assignment JetStream ack for outbox row {} on {subject}", row.id))?;
        }
        metrics::histogram!("agentforge_orchestration_assignment_publish_seconds")
            .record(publish_started.elapsed().as_secs_f64());

        sqlx::query(MARK_OUTBOX_PUBLISHED_SQL)
            .bind(row.id)
            .execute(&mut *tx)
            .await
            .context("mark orchestration outbox row published")?;
        tx.commit().await.context("commit published outbox tx")?;

        tracing::info!(
            outbox_id = %row.id,
            aggregate_id = %row.aggregate_id,
            legacy = %legacy_subject,
            namespaced = %namespaced_subject,
            "Published orchestration assignment outbox row (dual-published)"
        );
        metrics::counter!("agentforge_orchestration_outbox_published_total").increment(1);
        Ok(true)
    }

    /// Relay one unpublished `project_clone` outbox row into `job_queue` (M2).
    /// Thin wrapper over [`relay_next_clone_outbox`] for the publisher loop.
    async fn publish_next_clone(&self) -> Result<bool> {
        relay_next_clone_outbox(&self.pool).await.map(|relayed| relayed.is_some())
    }
}

/// Relay the next unpublished `project_clone` outbox row into `job_queue` (M2),
/// returning the relayed outbox row id (or `None` when the queue is empty).
///
/// Unlike the assignment path this never touches NATS: the clone worker (M5)
/// dequeues from `job_queue` directly. The enqueue + mark-published happen in ONE
/// transaction holding the `FOR UPDATE SKIP LOCKED` lock on the outbox row, so
/// the relay is atomic: a crash after enqueue but before commit rolls the row
/// back to unpublished and is retried.
///
/// IDEMPOTENCY SCOPE (do not overstate): the job's `ON CONFLICT (unique_key) DO
/// NOTHING` only protects against a duplicate enqueue WHILE the `job_queue` row
/// still exists. The `unique_key = project_clone:<project_id>:<attempt>` is
/// TRANSIENT — once the M5 worker `complete()`s the job (which DELETEs the row),
/// the unique_key is gone, so a subsequent relay of the SAME attempt (e.g. if
/// `published_at` were ever re-cleared) would enqueue a fresh duplicate. The
/// DURABLE, exactly-once-per-attempt guarantee is the WORKER's responsibility in
/// M5: it claims the attempt against the `uq_project_clone_attempt(project_id,
/// attempt)` unique index, which survives `complete()` and so dedups a
/// re-relayed job at claim time. This relay only guarantees "no duplicate while
/// the queued job is in flight," not "exactly once forever."
///
/// Exposed (not private to the publisher) so the relay can be exercised directly
/// without constructing a NATS client.
pub async fn relay_next_clone_outbox(pool: &PgPool) -> Result<Option<uuid::Uuid>> {
    let mut tx = pool.begin().await.context("begin clone outbox tx")?;
    let Some(row) = sqlx::query_as::<_, OrchestrationOutbox>(NEXT_CLONE_OUTBOX_SQL)
        .bind(CLONE_OUTBOX_AGGREGATE_TYPE)
        .fetch_optional(&mut *tx)
        .await
        .context("select next project_clone outbox row")?
    else {
        tx.commit().await.context("commit empty clone outbox tx")?;
        return Ok(None);
    };

    // A DECODE failure is TERMINAL, not transient: a structurally-undecodable
    // payload can never relay, and leaving the row unpublished would make the
    // next `FOR UPDATE SKIP LOCKED LIMIT 1` re-select this SAME row forever — a
    // head-of-line stall that blocks every newer clone request behind one poison
    // row. So on a decode error we DEAD-LETTER the row (mark it published so the
    // sweep skips it) in this same locked tx, count it on its own axis, log
    // LOUDLY at error level, and return Ok(Some) so the loop keeps draining newer
    // rows. (A transient DB error during the mark below still propagates and
    // leaves the row for retry — only the decode case is treated as poison.)
    let payload: CloneOutboxPayload = match serde_json::from_value(row.payload.clone()) {
        Ok(payload) => payload,
        Err(decode_err) => {
            sqlx::query(MARK_OUTBOX_PUBLISHED_SQL)
                .bind(row.id)
                .execute(&mut *tx)
                .await
                .context("dead-letter undecodable project_clone outbox row")?;
            tx.commit().await.context("commit dead-lettered clone outbox tx")?;

            tracing::error!(
                outbox_id = %row.id,
                aggregate_id = %row.aggregate_id,
                error = %decode_err,
                "POISON project_clone outbox row: payload could not be decoded — dead-lettered \
                 (marked published) so it cannot stall the relay head. The clone for this row was \
                 NOT enqueued; investigate the producer that wrote an undecodable payload."
            );
            metrics::counter!("agentforge_project_clone_outbox_poison_total").increment(1);
            return Ok(Some(row.id));
        }
    };
    let unique_key = payload.job_unique_key();

    // Enqueue inside the same tx (not the pool-based `queue::enqueue`) so the
    // insert and the mark-published commit atomically. The job carries the
    // identifier-only payload; the worker re-reads the authoritative attempt row.
    // `ON CONFLICT (unique_key) DO NOTHING` relies on the partial unique index
    // `idx_job_queue_unique_key` (migration 068).
    sqlx::query(
        r#"INSERT INTO job_queue (queue, payload, priority, run_at, unique_key, max_attempts)
           VALUES ($1, $2, 0, NOW(), $3, $4)
           ON CONFLICT (unique_key) WHERE unique_key IS NOT NULL DO NOTHING"#,
    )
    .bind(CLONE_JOB_QUEUE)
    .bind(&row.payload)
    .bind(&unique_key)
    .bind(CLONE_JOB_MAX_ATTEMPTS)
    .execute(&mut *tx)
    .await
    .with_context(|| format!("enqueue project_clone job for outbox row {}", row.id))?;

    sqlx::query(MARK_OUTBOX_PUBLISHED_SQL)
        .bind(row.id)
        .execute(&mut *tx)
        .await
        .context("mark project_clone outbox row published")?;
    tx.commit().await.context("commit published clone outbox tx")?;

    tracing::info!(
        outbox_id = %row.id,
        aggregate_id = %row.aggregate_id,
        unique_key = %unique_key,
        "Relayed project_clone outbox row into job_queue"
    );
    metrics::counter!("agentforge_project_clone_outbox_relayed_total").increment(1);
    Ok(Some(row.id))
}

pub async fn insert_assignment_outbox_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    organization_id: uuid::Uuid,
    task_id: uuid::Uuid,
    assignment: &TaskAssignment,
) -> Result<()> {
    let payload = serde_json::to_value(assignment).context("serialize assignment for outbox row")?;
    let delivery_id = assignment.delivery_id.context("assignment missing delivery_id")?;

    sqlx::query(
        r#"INSERT INTO orchestration_outbox
           (id, organization_id, aggregate_type, aggregate_id, event_type, payload)
           VALUES ($1, $2, 'orchestration_task', $3, 'assignment', $4)"#,
    )
    .bind(delivery_id)
    .bind(organization_id)
    .bind(task_id)
    .bind(payload)
    .execute(&mut **tx)
    .await
    .context("insert orchestration assignment outbox row")?;

    Ok(())
}

pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_orchestration_outbox_published_total",
        "Orchestration assignment outbox rows published to JetStream after publish ack"
    );
    metrics::describe_counter!(
        "agentforge_orchestration_outbox_publish_errors_total",
        "Orchestration assignment outbox publish attempts that failed before DB publish mark"
    );
    metrics::describe_histogram!(
        "agentforge_orchestration_assignment_publish_seconds",
        "Time to publish one orchestration assignment outbox row to JetStream and receive ack"
    );
    metrics::describe_counter!(
        "agentforge_orchestration_assignment_kind_fallback_total",
        "#457 phase 1c: assignment publishes whose runtime_kind could not be resolved and \
         defaulted to Container. The orchestration.assigned legacy-drop deploy is gated on this \
         holding at zero (a non-zero value risks silently stranding a cli/api assignment post-drop)."
    );
    metrics::describe_counter!(
        "agentforge_project_clone_outbox_relayed_total",
        "project_clone transactional-outbox rows relayed into job_queue after the enqueue + \
         mark-published commit (M2). A duplicate publish is a no-op and is NOT counted here."
    );
    metrics::describe_counter!(
        "agentforge_project_clone_outbox_relay_errors_total",
        "project_clone outbox relay attempts that failed (a transient DB/enqueue error that leaves \
         the row unpublished for retry). Tracked on its OWN axis — separate from assignment-publish \
         errors — so a stuck clone backlog is observable independently."
    );
    metrics::describe_counter!(
        "agentforge_project_clone_outbox_poison_total",
        "project_clone outbox rows that were dead-lettered (marked published WITHOUT enqueueing a \
         job) because their payload could not be decoded. A non-zero value means a producer wrote \
         an undecodable payload and that clone was dropped — investigate. Distinct from a transient \
         relay error: a poison row can never succeed, so it is skipped to keep the relay head moving."
    );

    metrics::counter!("agentforge_orchestration_outbox_published_total").increment(0);
    metrics::counter!("agentforge_orchestration_outbox_publish_errors_total").increment(0);
    metrics::histogram!("agentforge_orchestration_assignment_publish_seconds").record(0.0);
    metrics::counter!("agentforge_orchestration_assignment_kind_fallback_total").increment(0);
    metrics::counter!("agentforge_project_clone_outbox_relayed_total").increment(0);
    metrics::counter!("agentforge_project_clone_outbox_relay_errors_total").increment(0);
    metrics::counter!("agentforge_project_clone_outbox_poison_total").increment(0);
}
