//! JetStream consumer that turns sidecar-emitted task results into DB
//! complete/fail.
//!
//! The flow is: sidecar executes the wrapped CLI → publishes a signed envelope
//! to `orchestration.result.<agent_id>` → JetStream persists it → this durable
//! consumer verifies the envelope signature against the per-agent HMAC secret
//! stored at spawn time → resolves `agent_id → organization_id` via the
//! `participants` table → applies the result to `orchestration_tasks` and
//! releases the participant.
//!
//! Issue #39: signature verification is enforced here. Any envelope that
//! fails verification (bad key, tampered payload, missing agent row, or
//! timestamp outside the ±5 min replay window) is dropped and bumps
//! `orchestration_result_unauthorized_total{reason=…}` so operators can
//! spot forgery attempts on their dashboards without reading per-message
//! logs.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use async_nats::Client;
use async_nats::jetstream::consumer::{self, PullConsumer, pull};
use async_nats::jetstream::{self, AckKind};
use async_trait::async_trait;
use futures::StreamExt;
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::watch;
use uuid::Uuid;

use agentforge_core::orchestration_protocol::{
    RESULT_SUBJECT_PREFIX, SignedEnvelope, TaskOutcome, TaskResult, parse_agent_id_from_subject,
    result_subject_wildcard,
};
use agentforge_db::entities::OrchestrationTask;
use agentforge_db::inbox_notifications::{TaskOwnerNotificationKind, upsert_task_owner_lifecycle_notification_in_tx};

use crate::orchestration_realtime::publish_task_update;

/// Accept envelopes whose `timestamp` is within ±5 minutes of the consumer's
/// wall clock. Narrower than the 15-min JWT window because these messages are
/// produced by a sidecar we just spawned — the clock skew tolerance only has
/// to absorb NTP drift + one scheduling lag, not cross-region tokens.
pub(crate) const TIMESTAMP_REPLAY_WINDOW_SECS: i64 = 300;
pub const ORCHESTRATION_RESULTS_STREAM: &str = "ORCHESTRATION_RESULTS";
pub const ORCHESTRATION_RESULTS_DURABLE: &str = "orchestration-result-handler";
const FETCH_BATCH_SIZE: usize = 8;
const FETCH_TIMEOUT_MS: u64 = 500;
const ACK_WAIT_SECS: u64 = 30;
const MAX_DELIVER: i64 = 5;

/// Filter subject for the orchestration-result consumer. Must stay aligned
/// with the result stream subject set.
pub fn results_filter() -> String {
    result_subject_wildcard()
}

/// Build a wildcard filter for a custom result subject prefix. Used by
/// integration tests to isolate their JetStream stream from the shared
/// production `orchestration.result.*` work queue.
pub fn results_filter_for(subject_prefix: &str) -> String {
    format!("{subject_prefix}.*")
}

#[derive(Debug, Clone)]
pub struct OrchestrationResultConsumerConfig {
    pub stream_name: String,
    pub durable_name: String,
    pub filter_subject: String,
    pub subject_prefix: String,
}

impl Default for OrchestrationResultConsumerConfig {
    fn default() -> Self {
        Self::production()
    }
}

impl OrchestrationResultConsumerConfig {
    pub fn production() -> Self {
        Self {
            stream_name: ORCHESTRATION_RESULTS_STREAM.to_string(),
            durable_name: ORCHESTRATION_RESULTS_DURABLE.to_string(),
            filter_subject: results_filter(),
            subject_prefix: RESULT_SUBJECT_PREFIX.to_string(),
        }
    }
}

/// Typed error returned by `handle_message`. The variant determines how the
/// caller ACKs the JetStream message.
#[derive(Debug)]
pub enum HandleError {
    /// Security-relevant rejection. Ack with Term — redelivery won't help.
    Unauthorized { reason: &'static str, detail: String },
    /// Transient infra failure (DB, network). Nak so JetStream can redeliver.
    Transient(anyhow::Error),
}

impl std::fmt::Display for HandleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized { reason, detail } => write!(f, "unauthorized ({reason}): {detail}"),
            Self::Transient(err) => write!(f, "transient: {err}"),
        }
    }
}

impl std::error::Error for HandleError {}

/// Resolves `agent_id → organization_id` via the `participants` table. Modeled
/// as a trait so the integration test can swap in a deterministic in-memory
/// implementation without a Postgres dependency.
#[async_trait]
pub trait ParticipantLookup: Clone + Send + Sync + 'static {
    async fn find_org(&self, agent_id: Uuid) -> Result<Option<Uuid>>;
}

/// Applies a task outcome (completion or failure) and releases the
/// participant so the auto-dispatcher can hand them the next task.
#[async_trait]
pub trait TaskWriter: Clone + Send + Sync + 'static {
    async fn apply(&self, organization_id: Uuid, result: TaskResult) -> Result<()>;
}

/// Fetches the per-agent HMAC secret persisted at spawn time. Modeled as a
/// trait so tests can inject fixed keys without a DB. Not tenant-scoped —
/// the consumer runs in a worker context without `TenantScope`, and the
/// `agent_id` comes from the NATS subject which is already the identity we
/// verify against. Returning `Ok(None)` for an unknown agent is the
/// expected path for forged subjects and for agents spawned before
/// migration 025; the consumer treats it as a verification failure.
#[async_trait]
pub trait HmacSecretLookup: Clone + Send + Sync + 'static {
    async fn find_secret(&self, agent_id: Uuid) -> Result<Option<String>>;
}

pub struct OrchestrationResultWorker<L, W, H> {
    consumer: PullConsumer,
    lookup: L,
    writer: W,
    hmac: H,
    config: OrchestrationResultConsumerConfig,
}

impl<L, W, H> OrchestrationResultWorker<L, W, H>
where
    L: ParticipantLookup,
    W: TaskWriter,
    H: HmacSecretLookup,
{
    pub async fn connect(client: Client, lookup: L, writer: W, hmac: H) -> Result<Self> {
        Self::connect_with_config(client, lookup, writer, hmac, OrchestrationResultConsumerConfig::production()).await
    }

    pub async fn connect_with_config(
        client: Client,
        lookup: L,
        writer: W,
        hmac: H,
        config: OrchestrationResultConsumerConfig,
    ) -> Result<Self> {
        let jetstream = jetstream::new(client);
        let stream = jetstream
            .get_stream(&config.stream_name)
            .await
            .with_context(|| format!("failed to open JetStream stream {}", config.stream_name))?;
        let consumer: PullConsumer = stream
            .get_or_create_consumer(
                &config.durable_name,
                pull::Config {
                    durable_name: Some(config.durable_name.clone()),
                    ack_policy: consumer::AckPolicy::Explicit,
                    ack_wait: Duration::from_secs(ACK_WAIT_SECS),
                    max_deliver: MAX_DELIVER,
                    filter_subject: config.filter_subject.clone(),
                    ..Default::default()
                },
            )
            .await
            .context("failed to create orchestration result consumer")?;

        Ok(Self { consumer, lookup, writer, hmac, config })
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_ok() && *shutdown.borrow() {
                        tracing::info!(durable = %self.config.durable_name, "Orchestration result worker shutting down");
                        break;
                    }
                }
                batch = self.consumer.fetch().max_messages(FETCH_BATCH_SIZE).expires(Duration::from_millis(FETCH_TIMEOUT_MS)).messages() => {
                    match batch {
                        Ok(mut messages) => {
                            while let Some(message) = messages.next().await {
                                let Ok(message) = message else { break; };
                                let subject = message.subject.to_string();
                                let payload = message.payload.to_vec();
                                match self.handle(&subject, &payload).await {
                                    Ok(()) => {
                                        if let Err(err) = message.ack().await {
                                            metrics::counter!(
                                                "agentforge_orchestration_result_ack_errors_total",
                                                "kind" => "success"
                                            )
                                            .increment(1);
                                            metrics::counter!("orchestration_result_ack_errors_total", "kind" => "success").increment(1);
                                            tracing::warn!(error = %err, %subject, "orchestration result ack failed; will redeliver");
                                        }
                                    }
                                    Err(HandleError::Unauthorized { reason, detail }) => {
                                        tracing::warn!(%reason, %detail, %subject, "orchestration result rejected");
                                        let _ = message.ack_with(AckKind::Term).await;
                                    }
                                    Err(HandleError::Transient(err)) => {
                                        tracing::warn!(error = %err, %subject, "orchestration result transient error; will redeliver");
                                        let _ = message.ack_with(AckKind::Nak(None)).await;
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, "orchestration result batch fetch failed");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }
    }

    async fn handle(&self, subject: &str, payload: &[u8]) -> Result<(), HandleError> {
        handle_message_with_subject_prefix(
            &self.lookup,
            &self.writer,
            &self.hmac,
            &self.config.subject_prefix,
            subject,
            payload,
        )
        .await
    }
}

/// Bump `orchestration_result_unauthorized_total{reason}` and return an Err
/// carrying the same reason — one helper so the metric label and the log
/// message can't drift apart.
fn reject_unauthorized<T>(reason: &'static str, detail: impl std::fmt::Display) -> Result<T, HandleError> {
    metrics::counter!("agentforge_orchestration_result_unauthorized_total", "reason" => reason).increment(1);
    metrics::counter!("orchestration_result_unauthorized_total", "reason" => reason).increment(1);
    Err(HandleError::Unauthorized { reason, detail: detail.to_string() })
}

/// Pure handler logic extracted so tests can exercise it without a live NATS
/// client. All validation happens here: subject parsing, payload decode,
/// envelope signature verification (issue #39), agent cross-check,
/// participant lookup, result apply.
pub async fn handle_message<L, W, H>(
    lookup: &L,
    writer: &W,
    hmac: &H,
    subject: &str,
    payload: &[u8],
) -> Result<(), HandleError>
where
    L: ParticipantLookup,
    W: TaskWriter,
    H: HmacSecretLookup,
{
    handle_message_with_subject_prefix(lookup, writer, hmac, RESULT_SUBJECT_PREFIX, subject, payload).await
}

pub async fn handle_message_with_subject_prefix<L, W, H>(
    lookup: &L,
    writer: &W,
    hmac: &H,
    subject_prefix: &str,
    subject: &str,
    payload: &[u8],
) -> Result<(), HandleError>
where
    L: ParticipantLookup,
    W: TaskWriter,
    H: HmacSecretLookup,
{
    let subject_agent = parse_agent_id_from_subject(subject, subject_prefix)
        .ok_or_else(|| HandleError::Unauthorized { reason: "bad_subject", detail: format!("subject {subject}") })?;

    let envelope: SignedEnvelope = serde_json::from_slice(payload)
        .map_err(|err| HandleError::Unauthorized { reason: "envelope_decode_failed", detail: err.to_string() })?;

    // Envelope-stated agent_id MUST match the subject's — verifying the
    // signature of the wrong agent's secret would otherwise pass for a
    // forger who controls any single agent's HMAC key.
    if envelope.agent_id != subject_agent.to_string() {
        return reject_unauthorized(
            "envelope_agent_mismatch",
            format!("subject {subject_agent} vs envelope {}", envelope.agent_id),
        );
    }

    // Replay guard: reject timestamps that drift more than ±5 min from the
    // consumer's clock. Constant window; skew tolerance, not expiry, so we
    // use the same bound on both sides of `now`.
    let now_secs = chrono::Utc::now().timestamp();
    if (now_secs - envelope.timestamp).abs() > TIMESTAMP_REPLAY_WINDOW_SECS {
        return reject_unauthorized(
            "timestamp_outside_window",
            format!("envelope ts {} vs now {now_secs}", envelope.timestamp),
        );
    }

    // Secret lookup is the auth step. `Ok(None)` = row never existed / was
    // cleared on stop / migration 025 hadn't run yet; in every case the
    // right move is to refuse the message rather than fall through.
    let secret = match hmac.find_secret(subject_agent).await {
        Ok(Some(s)) => s,
        Ok(None) => return reject_unauthorized("agent_unknown", format!("no hmac_secret for agent {subject_agent}")),
        Err(err) => {
            metrics::counter!("agentforge_orchestration_result_transient_errors_total", "stage" => "hmac_lookup")
                .increment(1);
            metrics::counter!("orchestration_result_transient_errors_total", "stage" => "hmac_lookup").increment(1);
            return Err(HandleError::Transient(err));
        }
    };

    if !envelope.verify(secret.as_bytes()) {
        return reject_unauthorized("signature_mismatch", format!("agent {subject_agent}"));
    }

    // Parse the payload only after verification — prevents an attacker from
    // trivially exercising `TaskResult` deserialisation on unauthenticated
    // input, and keeps the error taxonomy clean (`bad_payload` only fires
    // for legitimately-signed-but-malformed bodies).
    let result: TaskResult = serde_json::from_value(envelope.payload)
        .map_err(|err| HandleError::Unauthorized { reason: "bad_payload", detail: err.to_string() })?;

    if result.agent_id != subject_agent {
        return reject_unauthorized(
            "payload_agent_mismatch",
            format!("subject {subject_agent} vs payload {}", result.agent_id),
        );
    }
    if result.delivery_id.is_none() {
        return reject_unauthorized("missing_delivery_id", format!("task {}", result.task_id));
    }
    if result.attempt.is_none() {
        return reject_unauthorized("missing_attempt", format!("task {}", result.task_id));
    }

    let organization_id = match lookup.find_org(result.agent_id).await {
        Ok(Some(org_id)) => org_id,
        Ok(None) => {
            return reject_unauthorized(
                "participant_missing",
                format!("no participant registered for agent {}", result.agent_id),
            );
        }
        Err(err) => {
            metrics::counter!("agentforge_orchestration_result_transient_errors_total", "stage" => "participant_lookup")
                .increment(1);
            metrics::counter!("orchestration_result_transient_errors_total", "stage" => "participant_lookup")
                .increment(1);
            return Err(HandleError::Transient(err));
        }
    };

    let apply_started = Instant::now();
    match writer.apply(organization_id, result).await {
        Ok(()) => {
            metrics::histogram!("agentforge_orchestration_result_apply_seconds", "outcome" => "success")
                .record(apply_started.elapsed().as_secs_f64());
            Ok(())
        }
        Err(err) => {
            metrics::histogram!("agentforge_orchestration_result_apply_seconds", "outcome" => "transient_error")
                .record(apply_started.elapsed().as_secs_f64());
            metrics::counter!("agentforge_orchestration_result_transient_errors_total", "stage" => "writer_apply")
                .increment(1);
            metrics::counter!("orchestration_result_transient_errors_total", "stage" => "writer_apply").increment(1);
            Err(HandleError::Transient(err))
        }
    }
}

/// Production `ParticipantLookup` backed by the `participants` table.
#[derive(Clone)]
pub struct SqlxParticipantLookup {
    pool: PgPool,
}

pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_orchestration_result_unauthorized_total",
        "Orchestration result envelopes rejected as unauthorized; label = reason"
    );
    metrics::describe_counter!(
        "agentforge_orchestration_result_transient_errors_total",
        "Transient orchestration result consumer failures that should be redelivered; label = stage"
    );
    metrics::describe_counter!(
        "agentforge_orchestration_result_ack_errors_total",
        "JetStream ack send failures for orchestration results; label = kind"
    );
    metrics::describe_counter!(
        "agentforge_orchestration_inbox_duplicate_total",
        "Duplicate orchestration result deliveries deduped by delivery_id"
    );
    metrics::describe_histogram!(
        "agentforge_orchestration_result_apply_seconds",
        "Time spent applying one verified orchestration result to the database"
    );

    metrics::counter!("agentforge_orchestration_result_unauthorized_total", "reason" => "none").increment(0);
    metrics::counter!("agentforge_orchestration_result_transient_errors_total", "stage" => "none").increment(0);
    metrics::counter!("agentforge_orchestration_result_ack_errors_total", "kind" => "none").increment(0);
    metrics::counter!("agentforge_orchestration_inbox_duplicate_total").increment(0);
    metrics::histogram!("agentforge_orchestration_result_apply_seconds", "outcome" => "success").record(0.0);
}

impl SqlxParticipantLookup {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ParticipantLookup for SqlxParticipantLookup {
    async fn find_org(&self, agent_id: Uuid) -> Result<Option<Uuid>> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            r#"SELECT organization_id
               FROM participants
               WHERE agent_id = $1
               LIMIT 1"#,
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.0))
    }
}

/// Production `HmacSecretLookup` backed by `agents.hmac_secret`. The row is
/// nullable — we treat "missing row" and "row with NULL secret" the same,
/// returning `Ok(None)` so the caller can emit a uniform
/// `reason=agent_unknown` rejection.
#[derive(Clone)]
pub struct SqlxHmacSecretLookup {
    pool: PgPool,
}

impl SqlxHmacSecretLookup {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HmacSecretLookup for SqlxHmacSecretLookup {
    async fn find_secret(&self, agent_id: Uuid) -> Result<Option<String>> {
        let row: Option<(Option<String>,)> = sqlx::query_as(r#"SELECT hmac_secret FROM agents WHERE id = $1 LIMIT 1"#)
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.and_then(|r| r.0))
    }
}

/// Production `TaskWriter` that mirrors the HTTP `complete_task` / `fail_task`
/// service logic directly against the DB. Reimplementing instead of calling
/// back into the HTTP handlers keeps this worker dependency-free and avoids a
/// loopback authenticated HTTP call from inside the same process.
#[derive(Clone)]
pub struct SqlxTaskWriter {
    pool: PgPool,
    realtime_client: Option<Client>,
}

impl SqlxTaskWriter {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, realtime_client: None }
    }

    pub fn with_realtime(mut self, client: Client) -> Self {
        self.realtime_client = Some(client);
        self
    }
}

#[async_trait]
impl TaskWriter for SqlxTaskWriter {
    async fn apply(&self, organization_id: Uuid, result: TaskResult) -> Result<()> {
        let delivery_id = result.delivery_id.context("orchestration result missing delivery_id")?;
        let attempt = result.attempt.context("orchestration result missing attempt")?;
        let task_id = result.task_id;
        let agent_id = result.agent_id;
        let (status, body) = match result.outcome {
            TaskOutcome::Completed { stdout } => ("completed", json!({ "stdout": stdout })),
            TaskOutcome::Failed { stderr, exit_code } => {
                ("failed", json!({ "message": stderr, "exit_code": exit_code }))
            }
        };
        let mut tx = self.pool.begin().await?;

        let inserted = sqlx::query(
            r#"INSERT INTO orchestration_inbox
               (delivery_id, organization_id, task_id, message_type)
               VALUES ($1, $2, $3, 'result')
               ON CONFLICT (delivery_id) DO NOTHING"#,
        )
        .bind(delivery_id)
        .bind(organization_id)
        .bind(task_id)
        .execute(&mut *tx)
        .await?;

        if inserted.rows_affected() == 0 {
            metrics::counter!("agentforge_orchestration_inbox_duplicate_total").increment(1);
            tx.commit().await?;
            return Ok(());
        }

        // A single UPDATE enforces three invariants at once: tenant scope,
        // working-state guard (transitions from any other status silently no-op),
        // and the result/error column pick. We also stamp completed_at and
        // progress the same way `set_result` does on the HTTP path so the UI
        // renders identically whether the task was completed via the kanban or
        // the worker bridge.
        let updated = sqlx::query_as::<_, OrchestrationTask>(
            r#"UPDATE orchestration_tasks
               SET status = $3,
                   result = CASE WHEN $3 = 'completed' THEN $4 ELSE result END,
                   error  = CASE WHEN $3 = 'failed'    THEN $4 ELSE error  END,
                   progress = CASE WHEN $3 = 'completed' THEN 100 ELSE progress END,
                   lease_expires_at = NULL,
                   retryable = FALSE,
                   completed_at = NOW(),
                   updated_at = NOW()
               WHERE id = $1
                 AND organization_id = $2
                 AND status = 'working'
                 AND last_assignment_id = $5
                 AND attempt = $6
               RETURNING *"#,
        )
        .bind(task_id)
        .bind(organization_id)
        .bind(status)
        .bind(&body)
        .bind(delivery_id)
        .bind(attempt)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(updated_task) = updated else {
            tracing::warn!(
                task_id = %task_id,
                delivery_id = ?delivery_id,
                attempt,
                %organization_id,
                %status,
                "No working task matched the result; sidecar likely reported for a task that was already canceled or completed",
            );
            tx.commit().await?;
            return Ok(());
        };

        sqlx::query(
            r#"UPDATE task_runs
                  SET status = $3,
                      finished_at = COALESCE(finished_at, NOW()),
                      updated_at = NOW()
                WHERE organization_id = $1
                  AND orchestration_task_id = $2
                  AND idempotency_key = $4
                  AND finished_at IS NULL"#,
        )
        .bind(organization_id)
        .bind(task_id)
        .bind(status)
        .bind(delivery_id.to_string())
        .execute(&mut *tx)
        .await?;

        let assigned_agent_name: Option<String> = sqlx::query_scalar(
            r#"SELECT name
               FROM participants
               WHERE organization_id = $1 AND agent_id = $2
               LIMIT 1"#,
        )
        .bind(organization_id)
        .bind(agent_id)
        .fetch_optional(&mut *tx)
        .await?;

        sqlx::query(
            r#"UPDATE participants
               SET status = 'available'
               WHERE organization_id = $1 AND agent_id = $2"#,
        )
        .bind(organization_id)
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

        if status == "failed" {
            upsert_task_owner_lifecycle_notification_in_tx(
                &mut tx,
                &updated_task,
                assigned_agent_name.as_deref(),
                TaskOwnerNotificationKind::Failed,
            )
            .await?;
        }

        tx.commit().await?;
        self.broadcast_task_result(&updated_task, assigned_agent_name.as_deref(), status).await;
        Ok(())
    }
}

impl SqlxTaskWriter {
    async fn broadcast_task_result(
        &self,
        task: &OrchestrationTask,
        assigned_agent_name: Option<&str>,
        status: &'static str,
    ) {
        let Some(client) = self.realtime_client.as_ref() else {
            return;
        };
        let action = match status {
            "completed" => "task.completed",
            "failed" => "task.failed",
            _ => "task.updated",
        };
        if let Err(err) = publish_task_update(client, task, assigned_agent_name, action).await {
            tracing::warn!(error = %err, task_id = %task.id, %action, "Failed to broadcast orchestration result task update");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentforge_core::orchestration_protocol::{SignedEnvelope, assign_subject, result_subject};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    const TEST_HMAC: &str = "hmac-unit-test-key";

    #[derive(Clone, Default)]
    struct FakeLookup {
        by_agent: Arc<HashMap<Uuid, Uuid>>,
    }

    #[async_trait]
    impl ParticipantLookup for FakeLookup {
        async fn find_org(&self, agent_id: Uuid) -> Result<Option<Uuid>> {
            Ok(self.by_agent.get(&agent_id).copied())
        }
    }

    #[derive(Clone, Default)]
    struct FakeWriter {
        applied: Arc<Mutex<Vec<(Uuid, TaskResult)>>>,
    }

    #[async_trait]
    impl TaskWriter for FakeWriter {
        async fn apply(&self, organization_id: Uuid, result: TaskResult) -> Result<()> {
            self.applied.lock().await.push((organization_id, result));
            Ok(())
        }
    }

    /// Static HMAC map: every agent in the map verifies against `TEST_HMAC`;
    /// any agent missing from the map is `Ok(None)` (unknown — treated as
    /// unauthorised by the handler).
    #[derive(Clone, Default)]
    struct FakeHmac {
        by_agent: Arc<HashMap<Uuid, String>>,
    }

    impl FakeHmac {
        fn with(agent_id: Uuid, secret: &str) -> Self {
            Self { by_agent: Arc::new(HashMap::from([(agent_id, secret.to_string())])) }
        }
    }

    #[async_trait]
    impl HmacSecretLookup for FakeHmac {
        async fn find_secret(&self, agent_id: Uuid) -> Result<Option<String>> {
            Ok(self.by_agent.get(&agent_id).cloned())
        }
    }

    /// Build an envelope signed with `secret` and a fresh timestamp so the
    /// replay guard doesn't trip.
    fn envelope_with(secret: &str, result: &TaskResult) -> Vec<u8> {
        let env = SignedEnvelope::sign(
            secret.as_bytes(),
            &result.agent_id.to_string(),
            chrono::Utc::now().timestamp(),
            result,
        )
        .expect("sign envelope");
        serde_json::to_vec(&env).unwrap()
    }

    fn envelope(result: &TaskResult) -> Vec<u8> {
        envelope_with(TEST_HMAC, result)
    }

    fn result_for(agent_id: Uuid, outcome: TaskOutcome) -> TaskResult {
        TaskResult { delivery_id: Some(Uuid::now_v7()), attempt: Some(1), task_id: Uuid::now_v7(), agent_id, outcome }
    }

    #[tokio::test]
    async fn handle_rejects_unknown_agent() {
        let writer = FakeWriter::default();
        let lookup = FakeLookup::default();
        let agent_id = Uuid::now_v7();
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let result = result_for(agent_id, TaskOutcome::Completed { stdout: "ok".into() });
        let payload = envelope(&result);
        let err =
            handle_message(&lookup, &writer, &hmac, &result_subject(result.agent_id), &payload).await.unwrap_err();
        assert!(err.to_string().contains("no participant registered"), "err = {err}");
        assert!(writer.applied.lock().await.is_empty());
    }

    #[tokio::test]
    async fn handle_routes_completed_result_to_writer() {
        let agent_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();
        let lookup = FakeLookup { by_agent: Arc::new(HashMap::from([(agent_id, org_id)])) };
        let writer = FakeWriter::default();
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let result = result_for(agent_id, TaskOutcome::Completed { stdout: "all done".into() });
        handle_message(&lookup, &writer, &hmac, &result_subject(agent_id), &envelope(&result)).await.unwrap();
        let applied = writer.applied.lock().await;
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].0, org_id);
        assert_eq!(applied[0].1, result);
    }

    #[tokio::test]
    async fn handle_rejects_result_missing_delivery_metadata() {
        let agent_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();
        let lookup = FakeLookup { by_agent: Arc::new(HashMap::from([(agent_id, org_id)])) };
        let writer = FakeWriter::default();
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);

        let mut missing_delivery = result_for(agent_id, TaskOutcome::Completed { stdout: "ok".into() });
        missing_delivery.delivery_id = None;
        let err = handle_message(&lookup, &writer, &hmac, &result_subject(agent_id), &envelope(&missing_delivery))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("missing_delivery_id"), "err = {err}");

        let mut missing_attempt = result_for(agent_id, TaskOutcome::Completed { stdout: "ok".into() });
        missing_attempt.attempt = None;
        let err = handle_message(&lookup, &writer, &hmac, &result_subject(agent_id), &envelope(&missing_attempt))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("missing_attempt"), "err = {err}");

        assert!(writer.applied.lock().await.is_empty());
    }

    #[tokio::test]
    async fn handle_rejects_subject_agent_mismatch() {
        let subject_agent = Uuid::now_v7();
        let payload_agent = Uuid::now_v7();
        let lookup = FakeLookup { by_agent: Arc::new(HashMap::from([(subject_agent, Uuid::now_v7())])) };
        let writer = FakeWriter::default();
        let hmac = FakeHmac::with(subject_agent, TEST_HMAC);
        let result = result_for(payload_agent, TaskOutcome::Completed { stdout: String::new() });
        // Envelope signed against the payload agent's id — the `agent_id`
        // field mismatch against the subject trips the unauthorized path.
        let err = handle_message(&lookup, &writer, &hmac, &result_subject(subject_agent), &envelope(&result))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("envelope_agent_mismatch"), "err = {err}");
        assert!(writer.applied.lock().await.is_empty());
    }

    #[tokio::test]
    async fn handle_rejects_non_result_subject() {
        let writer = FakeWriter::default();
        let lookup = FakeLookup::default();
        let agent_id = Uuid::now_v7();
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let result = result_for(agent_id, TaskOutcome::Failed { stderr: "x".into(), exit_code: Some(1) });
        // Addressing the handler on the assignment subject (not result) must be refused.
        let err =
            handle_message(&lookup, &writer, &hmac, &assign_subject(agent_id), &envelope(&result)).await.unwrap_err();
        assert!(err.to_string().contains("bad_subject"), "err = {err}");
    }

    #[tokio::test]
    async fn handle_routes_failed_result_to_writer() {
        let agent_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();
        let lookup = FakeLookup { by_agent: Arc::new(HashMap::from([(agent_id, org_id)])) };
        let writer = FakeWriter::default();
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let result = result_for(agent_id, TaskOutcome::Failed { stderr: "boom".into(), exit_code: Some(42) });
        handle_message(&lookup, &writer, &hmac, &result_subject(agent_id), &envelope(&result)).await.unwrap();
        let applied = writer.applied.lock().await;
        assert_eq!(applied.len(), 1);
        match &applied[0].1.outcome {
            TaskOutcome::Failed { stderr, exit_code } => {
                assert_eq!(stderr, "boom");
                assert_eq!(*exit_code, Some(42));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    // -------------------------------------------------------------------
    // Issue #39 acceptance: verification + replay
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn handle_rejects_envelope_with_wrong_hmac_key() {
        // Agent is known, but the sidecar signed with a stale key. Verify
        // must fail and the writer must not be touched — this is the
        // primary defence against a forger who snagged an old secret but
        // can't read the current row.
        let agent_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();
        let lookup = FakeLookup { by_agent: Arc::new(HashMap::from([(agent_id, org_id)])) };
        let writer = FakeWriter::default();
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let result = result_for(agent_id, TaskOutcome::Completed { stdout: "ok".into() });
        let forged = envelope_with("different-key", &result);
        let err = handle_message(&lookup, &writer, &hmac, &result_subject(agent_id), &forged).await.unwrap_err();
        assert!(err.to_string().contains("signature_mismatch"), "err = {err}");
        assert!(writer.applied.lock().await.is_empty());
    }

    #[tokio::test]
    async fn handle_rejects_envelope_when_agent_has_no_stored_secret() {
        // Pre-migration row, or an agent that was stopped between sidecar
        // publish and backend receive. The consumer treats "no secret" the
        // same as "bad secret" — neither path lets the writer run.
        let agent_id = Uuid::now_v7();
        let lookup = FakeLookup { by_agent: Arc::new(HashMap::from([(agent_id, Uuid::now_v7())])) };
        let writer = FakeWriter::default();
        let hmac = FakeHmac::default(); // no entry for any agent
        let result = result_for(agent_id, TaskOutcome::Completed { stdout: "ok".into() });
        let err =
            handle_message(&lookup, &writer, &hmac, &result_subject(agent_id), &envelope(&result)).await.unwrap_err();
        assert!(err.to_string().contains("agent_unknown"), "err = {err}");
        assert!(writer.applied.lock().await.is_empty());
    }

    #[tokio::test]
    async fn handle_rejects_envelope_outside_replay_window() {
        // Envelope signed with the right key but stamped an hour in the
        // past. Replay guard rejects; signature alone is not enough.
        let agent_id = Uuid::now_v7();
        let lookup = FakeLookup { by_agent: Arc::new(HashMap::from([(agent_id, Uuid::now_v7())])) };
        let writer = FakeWriter::default();
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let result = result_for(agent_id, TaskOutcome::Completed { stdout: "ok".into() });
        let stale_ts = chrono::Utc::now().timestamp() - (TIMESTAMP_REPLAY_WINDOW_SECS + 60);
        let env = SignedEnvelope::sign(TEST_HMAC.as_bytes(), &agent_id.to_string(), stale_ts, &result).unwrap();
        let payload = serde_json::to_vec(&env).unwrap();
        let err = handle_message(&lookup, &writer, &hmac, &result_subject(agent_id), &payload).await.unwrap_err();
        assert!(err.to_string().contains("timestamp_outside_window"), "err = {err}");
        assert!(writer.applied.lock().await.is_empty());
    }

    #[tokio::test]
    async fn handle_accepts_envelope_at_window_edge() {
        // Exactly `WINDOW` seconds old still passes — the bound is
        // inclusive so a normal slow path doesn't get cut off.
        let agent_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();
        let lookup = FakeLookup { by_agent: Arc::new(HashMap::from([(agent_id, org_id)])) };
        let writer = FakeWriter::default();
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let result = result_for(agent_id, TaskOutcome::Completed { stdout: "ok".into() });
        let edge_ts = chrono::Utc::now().timestamp() - TIMESTAMP_REPLAY_WINDOW_SECS;
        let env = SignedEnvelope::sign(TEST_HMAC.as_bytes(), &agent_id.to_string(), edge_ts, &result).unwrap();
        let payload = serde_json::to_vec(&env).unwrap();
        handle_message(&lookup, &writer, &hmac, &result_subject(agent_id), &payload).await.unwrap();
        assert_eq!(writer.applied.lock().await.len(), 1);
    }
}
