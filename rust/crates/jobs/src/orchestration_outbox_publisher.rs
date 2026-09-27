use std::time::{Duration, Instant};

use agentforge_core::RuntimeKind;
use agentforge_core::clone_protocol::{
    CLONE_JOB_MAX_ATTEMPTS, CLONE_JOB_QUEUE, CLONE_OUTBOX_AGGREGATE_TYPE, CloneOutboxPayload,
};
use agentforge_core::orchestration_protocol::{
    SignedEnvelope, TaskAssignment, assign_subject, assign_subject_kind, container_generation_fingerprint,
};
use agentforge_db::entities::OrchestrationOutbox;
use anyhow::{Context, Result, anyhow};
use async_nats::{Client, jetstream};
use chrono::Utc;
use sqlx::{PgPool, Postgres, Transaction};
use tokio::sync::watch;

const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(500);
const ERROR_BACKOFF: Duration = Duration::from_secs(1);

const NEXT_ASSIGNMENT_OUTBOX_SQL: &str = r#"SELECT *
      FROM orchestration_outbox
     WHERE published_at IS NULL
       AND event_type = 'assignment'
     ORDER BY created_at ASC
     LIMIT 1"#;

const LOCK_ASSIGNMENT_OUTBOX_SQL: &str = r#"SELECT *
      FROM orchestration_outbox
     WHERE id = $1
       AND published_at IS NULL
       AND event_type = 'assignment'
     FOR UPDATE SKIP LOCKED"#;

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

/// Serialize an assignment for publication. When `signing_secret` is `Some`,
/// wrap it in a [`SignedEnvelope`] (HMAC-SHA256 over `agent_id:timestamp:payload`)
/// signed with the target agent's per-agent secret, so the sidecar can verify
/// integrity before executing a payload that runs with
/// `--dangerously-skip-permissions` (F064). When `None`, emit the legacy raw
/// assignment. This raw form is retained only for non-container compatibility;
/// container assignments are required to select a signing secret before this
/// helper is called.
fn encode_assignment(assignment: &TaskAssignment, signing_secret: Option<&str>) -> Result<Vec<u8>> {
    match signing_secret {
        Some(secret) => {
            let envelope = SignedEnvelope::sign(
                secret.as_bytes(),
                &assignment.agent_id.to_string(),
                Utc::now().timestamp(),
                assignment,
            )
            .context("sign orchestration assignment envelope")?;
            serde_json::to_vec(&envelope).context("encode signed orchestration assignment")
        }
        None => serde_json::to_vec(assignment).context("encode orchestration assignment payload"),
    }
}

/// Container assignments are never allowed onto the wire unsigned: their
/// generation fingerprint is non-secret and only becomes an execution fence
/// when the containing assignment is authenticated. Non-container assignments
/// retain the rollout flag's legacy behavior.
fn select_assignment_signing_secret(
    runtime_kind: RuntimeKind,
    sign_non_container: bool,
    stored_secret: Option<&str>,
) -> Result<Option<&str>> {
    let stored_secret = stored_secret.filter(|secret| !secret.trim().is_empty());
    if runtime_kind == RuntimeKind::Container {
        return stored_secret
            .map(Some)
            .ok_or_else(|| anyhow!("container assignment requires a per-container HMAC signing secret"));
    }
    Ok(if sign_non_container { stored_secret } else { None })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssignmentRetirementReason {
    MalformedPayload,
    MalformedDeliveryMetadata,
    TaskMissing,
    TaskNotWorking,
    TaskLeaseExpired,
    AssignmentNotCurrent,
    AgentMissing,
    RuntimeKindMismatch,
    MissingSigningSecret,
    MissingGenerationFingerprint,
    StaleGenerationFingerprint,
}

impl AssignmentRetirementReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::MalformedPayload => "malformed_payload",
            Self::MalformedDeliveryMetadata => "malformed_delivery_metadata",
            Self::TaskMissing => "task_missing",
            Self::TaskNotWorking => "task_not_working",
            Self::TaskLeaseExpired => "task_lease_expired",
            Self::AssignmentNotCurrent => "assignment_not_current",
            Self::AgentMissing => "agent_missing",
            Self::RuntimeKindMismatch => "runtime_kind_mismatch",
            Self::MissingSigningSecret => "missing_signing_secret",
            Self::MissingGenerationFingerprint => "missing_generation_fingerprint",
            Self::StaleGenerationFingerprint => "stale_generation_fingerprint",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssignmentPublishDecision<'a> {
    Publish { signing_secret: Option<&'a str> },
    Retire { reason: AssignmentRetirementReason },
}

/// Decide whether an assignment is safe to place on the wire. This runs before
/// envelope encoding so even a legacy sidecar that ignores the fingerprint can
/// never receive an assignment authenticated by a replacement container's key.
fn assignment_publish_decision<'a>(
    runtime_kind: RuntimeKind,
    sign_non_container: bool,
    assignment: &TaskAssignment,
    stored_secret: Option<&'a str>,
) -> Result<AssignmentPublishDecision<'a>> {
    if runtime_kind == RuntimeKind::Container && stored_secret.filter(|secret| !secret.trim().is_empty()).is_none() {
        return Ok(AssignmentPublishDecision::Retire { reason: AssignmentRetirementReason::MissingSigningSecret });
    }
    let signing_secret = select_assignment_signing_secret(runtime_kind, sign_non_container, stored_secret)?;
    if runtime_kind != RuntimeKind::Container {
        return Ok(AssignmentPublishDecision::Publish { signing_secret });
    }

    let current_fingerprint = container_generation_fingerprint(
        signing_secret.expect("container signing selection returns a secret").as_bytes(),
    );
    let Some(enqueued_fingerprint) =
        assignment.container_generation_fingerprint.as_deref().filter(|fingerprint| !fingerprint.trim().is_empty())
    else {
        return Ok(AssignmentPublishDecision::Retire {
            reason: AssignmentRetirementReason::MissingGenerationFingerprint,
        });
    };
    if enqueued_fingerprint != current_fingerprint {
        return Ok(AssignmentPublishDecision::Retire {
            reason: AssignmentRetirementReason::StaleGenerationFingerprint,
        });
    }

    Ok(AssignmentPublishDecision::Publish { signing_secret })
}

#[derive(Debug, Clone)]
struct AssignmentTaskState {
    status: String,
    last_assignment_id: Option<uuid::Uuid>,
    assigned_agent_id: Option<uuid::Uuid>,
    lease_current: bool,
}

fn assignment_state_retirement_reason(
    row: &OrchestrationOutbox,
    assignment: &TaskAssignment,
    task: Option<AssignmentTaskState>,
) -> Option<AssignmentRetirementReason> {
    let Some(delivery_id) = assignment.delivery_id else {
        return Some(AssignmentRetirementReason::MalformedDeliveryMetadata);
    };
    if assignment.attempt.is_none()
        || assignment.lease_expires_at.is_none()
        || delivery_id != row.id
        || assignment.task_id != row.aggregate_id
    {
        return Some(AssignmentRetirementReason::MalformedDeliveryMetadata);
    }
    let Some(task) = task else {
        return Some(AssignmentRetirementReason::TaskMissing);
    };
    if task.status != "working" {
        return Some(AssignmentRetirementReason::TaskNotWorking);
    }
    if !task.lease_current {
        return Some(AssignmentRetirementReason::TaskLeaseExpired);
    }
    if task.last_assignment_id != Some(delivery_id) || task.assigned_agent_id != Some(assignment.agent_id) {
        return Some(AssignmentRetirementReason::AssignmentNotCurrent);
    }
    None
}

async fn retire_assignment_outbox_in_tx(tx: &mut Transaction<'_, Postgres>, outbox_id: uuid::Uuid) -> Result<()> {
    sqlx::query(MARK_OUTBOX_PUBLISHED_SQL)
        .bind(outbox_id)
        .execute(&mut **tx)
        .await
        .context("terminally retire stale orchestration assignment outbox row")?;
    Ok(())
}

async fn retire_locked_assignment(
    mut tx: Transaction<'_, Postgres>,
    row: &OrchestrationOutbox,
    assignment: Option<&TaskAssignment>,
    reason: AssignmentRetirementReason,
) -> Result<bool> {
    retire_assignment_outbox_in_tx(&mut tx, row.id).await?;
    tx.commit().await.context("commit retired orchestration assignment outbox row")?;
    tracing::error!(
        outbox_id = %row.id,
        task_id = %row.aggregate_id,
        agent_id = ?assignment.map(|value| value.agent_id),
        reason = reason.as_str(),
        "Retired unsafe orchestration assignment before publish; the task lease recovery path remains authoritative"
    );
    metrics::counter!(
        "agentforge_orchestration_assignment_retired_total",
        "reason" => reason.as_str()
    )
    .increment(1);
    Ok(true)
}

pub struct OrchestrationOutboxPublisher {
    pool: PgPool,
    client: Client,
    /// When true, also sign non-container assignments when the target has an
    /// HMAC secret. Container assignments are always signed regardless of this
    /// compatibility flag.
    sign_assignments: bool,
}

impl OrchestrationOutboxPublisher {
    pub fn new(pool: PgPool, client: Client, sign_assignments: bool) -> Self {
        Self { pool, client, sign_assignments }
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
        // Peek without a row lock so the safety lock order can be lifecycle ->
        // task -> outbox. Container quarantine uses that same order. Acquiring
        // the Agent advisory after locking the outbox would invert it and can
        // deadlock replacement/quarantine against the publisher.
        let Some(candidate) = sqlx::query_as::<_, OrchestrationOutbox>(NEXT_ASSIGNMENT_OUTBOX_SQL)
            .fetch_optional(&self.pool)
            .await
            .context("peek next orchestration outbox row")?
        else {
            return Ok(false);
        };

        // A poison payload has no trustworthy Agent id, so it cannot enter the
        // lifecycle lock order. Lock only this exact row and terminally retire
        // it; otherwise one pre-change/corrupt head blocks the whole queue.
        let candidate_assignment: TaskAssignment = match serde_json::from_value(candidate.payload.clone()) {
            Ok(assignment) => assignment,
            Err(decode_err) => {
                let mut tx = self.pool.begin().await.context("begin poison outbox retirement tx")?;
                let Some(row) = sqlx::query_as::<_, OrchestrationOutbox>(LOCK_ASSIGNMENT_OUTBOX_SQL)
                    .bind(candidate.id)
                    .fetch_optional(&mut *tx)
                    .await
                    .context("lock poison orchestration outbox row")?
                else {
                    tx.commit().await.context("commit skipped poison outbox tx")?;
                    return Ok(false);
                };
                if serde_json::from_value::<TaskAssignment>(row.payload.clone()).is_ok() {
                    tx.commit().await.context("commit changed poison outbox tx")?;
                    return Ok(false);
                }
                tracing::error!(outbox_id = %row.id, error = %decode_err, "poison assignment outbox payload");
                return retire_locked_assignment(tx, &row, None, AssignmentRetirementReason::MalformedPayload).await;
            }
        };

        let mut tx = self.pool.begin().await.context("begin outbox tx")?;
        // This acquisition intentionally precedes the outbox row lock. It
        // freezes the target container generation until both JetStream acks
        // complete, without introducing the quarantine lock inversion.
        agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, candidate_assignment.agent_id)
            .await
            .context("lock assignment Agent lifecycle before outbox")?;

        // Hold a share lock while publishing so cancel/expiry cannot change a
        // validated working delivery between this check and wire publication.
        let task_row: Option<(String, Option<uuid::Uuid>, Option<uuid::Uuid>, bool)> = sqlx::query_as(
            r#"SELECT status::text,
                      last_assignment_id,
                      assigned_agent_id,
                      lease_expires_at IS NOT NULL AND lease_expires_at > NOW()
                 FROM orchestration_tasks
                WHERE id = $1
                  AND organization_id = $2
                FOR SHARE"#,
        )
        .bind(candidate.aggregate_id)
        .bind(candidate.organization_id)
        .fetch_optional(&mut *tx)
        .await
        .context("lock authoritative task before assignment publish")?;

        let Some(row) = sqlx::query_as::<_, OrchestrationOutbox>(LOCK_ASSIGNMENT_OUTBOX_SQL)
            .bind(candidate.id)
            .fetch_optional(&mut *tx)
            .await
            .context("lock exact orchestration outbox row")?
        else {
            tx.commit().await.context("commit skipped outbox tx")?;
            return Ok(false);
        };
        let assignment: TaskAssignment = match serde_json::from_value(row.payload.clone()) {
            Ok(assignment) => assignment,
            Err(decode_err) => {
                tracing::error!(outbox_id = %row.id, error = %decode_err, "poison assignment outbox payload");
                return retire_locked_assignment(tx, &row, None, AssignmentRetirementReason::MalformedPayload).await;
            }
        };
        if assignment.agent_id != candidate_assignment.agent_id {
            return retire_locked_assignment(tx, &row, Some(&assignment), AssignmentRetirementReason::MalformedPayload)
                .await;
        }
        let task_state = task_row.map(|(status, last_assignment_id, assigned_agent_id, lease_current)| {
            AssignmentTaskState { status, last_assignment_id, assigned_agent_id, lease_current }
        });
        if let Some(reason) = assignment_state_retirement_reason(&row, &assignment, task_state) {
            return retire_locked_assignment(tx, &row, Some(&assignment), reason).await;
        }

        let agent_row: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT runtime_kind, hmac_secret FROM agents WHERE id = $1 AND organization_id = $2")
                .bind(assignment.agent_id)
                .bind(row.organization_id)
                .fetch_optional(&mut *tx)
                .await
                .context("lookup current Agent generation for assignment publish")?;
        let Some((raw_kind, stored_secret)) = agent_row else {
            return retire_locked_assignment(tx, &row, Some(&assignment), AssignmentRetirementReason::AgentMissing)
                .await;
        };
        let kind = match RuntimeKind::parse_legacy(&raw_kind) {
            Ok(kind) => kind,
            Err(_) => {
                return retire_locked_assignment(
                    tx,
                    &row,
                    Some(&assignment),
                    AssignmentRetirementReason::RuntimeKindMismatch,
                )
                .await;
            }
        };
        if assignment.runtime_kind.is_some_and(|enqueued| enqueued != kind) {
            return retire_locked_assignment(
                tx,
                &row,
                Some(&assignment),
                AssignmentRetirementReason::RuntimeKindMismatch,
            )
            .await;
        }

        let legacy_subject = assign_subject(assignment.agent_id);
        let namespaced_subject = assign_subject_kind(kind, assignment.agent_id);

        // Container generations fail closed: their non-secret generation
        // fingerprint must be authenticated by the SAME secret from which it
        // was derived. Host CLI/API assignments retain the rollout flag's
        // historical signed-when-possible / raw fallback behavior.
        if kind != RuntimeKind::Container
            && self.sign_assignments
            && stored_secret.as_deref().filter(|secret| !secret.trim().is_empty()).is_none()
        {
            tracing::warn!(
                agent_id = %assignment.agent_id,
                outbox_id = %row.id,
                "non-container assignment signing enabled but agent has no hmac_secret; preserving unsigned compatibility"
            );
        }
        let signing_secret =
            match assignment_publish_decision(kind, self.sign_assignments, &assignment, stored_secret.as_deref())
                .with_context(|| {
                    format!(
                        "refusing to publish container assignment outbox row {} for agent {} without signing",
                        row.id, assignment.agent_id
                    )
                })? {
                AssignmentPublishDecision::Publish { signing_secret } => signing_secret,
                AssignmentPublishDecision::Retire { reason } => {
                    return retire_locked_assignment(tx, &row, Some(&assignment), reason).await;
                }
            };
        let bytes = encode_assignment(&assignment, signing_secret)?;
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
    // Honor the retry backoff: a delayed payload schedules the job for its
    // `run_after`, so the worker's `run_at <= now()` dequeue filter holds it back
    // until then (no fast-fail retry storm). A first-delivery payload (run_after
    // None) enqueues for immediate pickup. `GREATEST(.., NOW())` clamps a stale
    // past deadline to now so a clock skew can never schedule the job in the past
    // ambiguously.
    let run_at = payload.run_after.unwrap_or_else(Utc::now);

    // Enqueue inside the same tx (not the pool-based `queue::enqueue`) so the
    // insert and the mark-published commit atomically. The job carries the
    // identifier-only payload; the worker re-reads the authoritative attempt row.
    // `ON CONFLICT (unique_key) DO NOTHING` relies on the partial unique index
    // `idx_job_queue_unique_key` (migration 068).
    sqlx::query(
        r#"INSERT INTO job_queue (queue, payload, priority, run_at, unique_key, max_attempts)
           VALUES ($1, $2, 0, GREATEST($3, NOW()), $4, $5)
           ON CONFLICT (unique_key) WHERE unique_key IS NOT NULL DO NOTHING"#,
    )
    .bind(CLONE_JOB_QUEUE)
    .bind(&row.payload)
    .bind(run_at)
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
        "agentforge_orchestration_assignment_retired_total",
        "Assignment outbox rows terminally retired before publish because the payload, live task delivery, Agent, \
         signing key, or container generation was no longer authoritative."
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
    for reason in [
        AssignmentRetirementReason::MalformedPayload,
        AssignmentRetirementReason::MalformedDeliveryMetadata,
        AssignmentRetirementReason::TaskMissing,
        AssignmentRetirementReason::TaskNotWorking,
        AssignmentRetirementReason::TaskLeaseExpired,
        AssignmentRetirementReason::AssignmentNotCurrent,
        AssignmentRetirementReason::AgentMissing,
        AssignmentRetirementReason::RuntimeKindMismatch,
        AssignmentRetirementReason::MissingSigningSecret,
        AssignmentRetirementReason::MissingGenerationFingerprint,
        AssignmentRetirementReason::StaleGenerationFingerprint,
    ] {
        metrics::counter!(
            "agentforge_orchestration_assignment_retired_total",
            "reason" => reason.as_str()
        )
        .increment(0);
    }
    metrics::counter!("agentforge_project_clone_outbox_relayed_total").increment(0);
    metrics::counter!("agentforge_project_clone_outbox_relay_errors_total").increment(0);
    metrics::counter!("agentforge_project_clone_outbox_poison_total").increment(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn sample_assignment() -> TaskAssignment {
        TaskAssignment {
            delivery_id: Some(Uuid::now_v7()),
            attempt: Some(1),
            lease_expires_at: Some(Utc::now()),
            task_id: Uuid::now_v7(),
            agent_id: Uuid::now_v7(),
            title: "Sweep".into(),
            task: "Do a thing".into(),
            message: String::new(),
            priority: "normal".into(),
            context_envelope: None,
            runtime_kind: Some(RuntimeKind::Cli),
            container_generation_fingerprint: None,
            image_paths: Vec::new(),
            trace_context: None,
        }
    }

    #[test]
    fn encode_assignment_unsigned_is_raw() {
        let assignment = sample_assignment();
        let bytes = encode_assignment(&assignment, None).unwrap();

        // Round-trips as a raw TaskAssignment and is NOT a SignedEnvelope.
        let back: TaskAssignment = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.task_id, assignment.task_id);
        assert!(serde_json::from_slice::<SignedEnvelope>(&bytes).is_err(), "unsigned encoding must not be an envelope");
    }

    #[test]
    fn encode_assignment_signed_verifies_with_secret_only() {
        // Generated, not a literal, so the secret-scan can't false-positive on it.
        let signing_key = Uuid::new_v4().to_string();
        let assignment = sample_assignment();
        let bytes = encode_assignment(&assignment, Some(&signing_key)).unwrap();

        let envelope: SignedEnvelope = serde_json::from_slice(&bytes).unwrap();
        assert!(envelope.verify(signing_key.as_bytes()), "must verify with the signing key");
        assert!(!envelope.verify(b"a-different-key"), "must not verify with a different key");
        assert_eq!(envelope.agent_id, assignment.agent_id.to_string());

        let back: TaskAssignment = serde_json::from_value(envelope.payload).unwrap();
        assert_eq!(back.task_id, assignment.task_id);
    }

    #[test]
    fn container_signing_is_mandatory_while_non_container_remains_compatible() {
        let signing_key = Uuid::new_v4().to_string();

        assert_eq!(
            select_assignment_signing_secret(RuntimeKind::Container, false, Some(&signing_key)).unwrap(),
            Some(signing_key.as_str()),
            "container assignments must be signed even when the compatibility flag is off"
        );
        assert!(select_assignment_signing_secret(RuntimeKind::Container, false, None).is_err());
        assert!(select_assignment_signing_secret(RuntimeKind::Container, true, Some("   ")).is_err());

        assert_eq!(
            select_assignment_signing_secret(RuntimeKind::Cli, false, Some(&signing_key)).unwrap(),
            None,
            "the compatibility flag still controls non-container signing"
        );
        assert_eq!(select_assignment_signing_secret(RuntimeKind::Api, true, None).unwrap(), None);
        assert_eq!(
            select_assignment_signing_secret(RuntimeKind::Cli, true, Some(&signing_key)).unwrap(),
            Some(signing_key.as_str())
        );
    }

    #[test]
    fn stale_or_legacy_container_rows_are_retired_before_legacy_sidecars_can_receive_them() {
        let mut assignment = sample_assignment();
        let old_secret = Uuid::new_v4().to_string();
        let replacement_secret = Uuid::new_v4().to_string();

        assert!(matches!(
            assignment_publish_decision(RuntimeKind::Cli, false, &assignment, Some(&replacement_secret)).unwrap(),
            AssignmentPublishDecision::Publish { signing_secret: None }
        ));

        assignment.runtime_kind = Some(RuntimeKind::Container);
        assert_eq!(
            assignment_publish_decision(RuntimeKind::Container, false, &assignment, Some(&replacement_secret)).unwrap(),
            AssignmentPublishDecision::Retire { reason: AssignmentRetirementReason::MissingGenerationFingerprint }
        );

        assignment.container_generation_fingerprint = Some(container_generation_fingerprint(old_secret.as_bytes()));
        assert_eq!(
            assignment_publish_decision(RuntimeKind::Container, false, &assignment, Some(&replacement_secret)).unwrap(),
            AssignmentPublishDecision::Retire { reason: AssignmentRetirementReason::StaleGenerationFingerprint },
            "a replacement key must never authenticate an old-generation assignment for a legacy sidecar"
        );

        assignment.container_generation_fingerprint =
            Some(container_generation_fingerprint(replacement_secret.as_bytes()));
        assert_eq!(
            assignment_publish_decision(RuntimeKind::Container, false, &assignment, Some(&replacement_secret)).unwrap(),
            AssignmentPublishDecision::Publish { signing_secret: Some(replacement_secret.as_str()) }
        );
        assert_eq!(
            assignment_publish_decision(RuntimeKind::Container, false, &assignment, None).unwrap(),
            AssignmentPublishDecision::Retire { reason: AssignmentRetirementReason::MissingSigningSecret }
        );
    }

    #[test]
    fn live_task_and_delivery_metadata_are_revalidated_before_publish() {
        let assignment = sample_assignment();
        let row = OrchestrationOutbox {
            id: assignment.delivery_id.unwrap(),
            organization_id: agentforge_core::OrgId::from(Uuid::new_v4()),
            aggregate_type: "orchestration_task".into(),
            aggregate_id: assignment.task_id,
            event_type: "assignment".into(),
            payload: serde_json::Value::Null,
            published_at: None,
            created_at: Utc::now(),
        };
        let current = || AssignmentTaskState {
            status: "working".into(),
            last_assignment_id: assignment.delivery_id,
            assigned_agent_id: Some(assignment.agent_id),
            lease_current: true,
        };
        assert_eq!(assignment_state_retirement_reason(&row, &assignment, Some(current())), None);

        let mut canceled = current();
        canceled.status = "canceled".into();
        assert_eq!(
            assignment_state_retirement_reason(&row, &assignment, Some(canceled)),
            Some(AssignmentRetirementReason::TaskNotWorking)
        );
        let mut expired = current();
        expired.lease_current = false;
        assert_eq!(
            assignment_state_retirement_reason(&row, &assignment, Some(expired)),
            Some(AssignmentRetirementReason::TaskLeaseExpired)
        );
        let mut superseded = current();
        superseded.last_assignment_id = Some(Uuid::new_v4());
        assert_eq!(
            assignment_state_retirement_reason(&row, &assignment, Some(superseded)),
            Some(AssignmentRetirementReason::AssignmentNotCurrent)
        );

        let mut malformed = assignment.clone();
        malformed.delivery_id = None;
        assert_eq!(
            assignment_state_retirement_reason(&row, &malformed, Some(current())),
            Some(AssignmentRetirementReason::MalformedDeliveryMetadata)
        );
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn retiring_oldest_generation_row_unblocks_the_next_assignment(pool: PgPool) {
        let organization_id = Uuid::new_v4();
        let stale_id = Uuid::new_v4();
        let next_id = Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Outbox', $2)")
            .bind(organization_id)
            .bind(format!("outbox-{organization_id}"))
            .execute(&pool)
            .await
            .expect("seed organization");
        for (id, age) in [(stale_id, "2 seconds"), (next_id, "1 second")] {
            sqlx::query(
                r#"INSERT INTO orchestration_outbox
                   (id, organization_id, aggregate_type, aggregate_id, event_type, payload, created_at)
                   VALUES ($1, $2, 'orchestration_task', $1, 'assignment', '{}'::jsonb,
                           NOW() - $3::interval)"#,
            )
            .bind(id)
            .bind(organization_id)
            .bind(age)
            .execute(&pool)
            .await
            .expect("seed assignment outbox row");
        }

        let mut tx = pool.begin().await.expect("begin retirement transaction");
        let oldest = sqlx::query_as::<_, OrchestrationOutbox>(NEXT_ASSIGNMENT_OUTBOX_SQL)
            .fetch_one(&mut *tx)
            .await
            .expect("select oldest assignment");
        assert_eq!(oldest.id, stale_id);
        retire_assignment_outbox_in_tx(&mut tx, oldest.id).await.expect("retire stale assignment");
        tx.commit().await.expect("commit retirement");

        let next = sqlx::query_as::<_, OrchestrationOutbox>(NEXT_ASSIGNMENT_OUTBOX_SQL)
            .fetch_one(&pool)
            .await
            .expect("select next assignment");
        assert_eq!(next.id, next_id, "retired poison head must not block newer assignments");
        let retired: bool =
            sqlx::query_scalar("SELECT published_at IS NOT NULL FROM orchestration_outbox WHERE id = $1")
                .bind(stale_id)
                .fetch_one(&pool)
                .await
                .expect("read retired row");
        assert!(retired);
    }
}
