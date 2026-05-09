//! NATS consumer that turns sidecar-emitted credential syncs into encrypted DB rows.
//!
//! The flow is: sidecar watches the container's credentials directory →
//! publishes `SignedEnvelope<CredentialSyncMessage>` to `creds.<agent_id>` →
//! this consumer verifies the envelope signature against the agent's HMAC
//! secret → resolves `agent_id → (organization_id, user_id)` via the
//! `agents` table → upserts the encrypted JSON blob into
//! `user_cli_credentials`.
//!
//! Security: unverified messages are never persisted. The rejection path
//! increments `credential_sync_unauthorized_total{reason=…}` with one
//! constant-string label per failure mode so operators can read the metric
//! without scanning logs.

use std::time::Duration;

use agentforge_core::CliToolKind;
use agentforge_core::credential_protocol::{
    CredentialSyncMessage, MAX_CREDENTIAL_FILE_BYTES, MAX_CREDENTIAL_FILES, MAX_CREDENTIAL_TOTAL_BYTES,
    creds_subject_wildcard, parse_agent_id_from_creds_subject,
};
use agentforge_core::orchestration_protocol::SignedEnvelope;
use anyhow::{Result, anyhow};
use async_nats::jetstream::consumer::{self, PullConsumer, pull};
use async_nats::jetstream::{self, AckKind};
use async_trait::async_trait;
use futures::StreamExt;
use sqlx::PgPool;
use tokio::sync::watch;
use uuid::Uuid;

/// Typed error returned by `handle_message`. The variant determines how the
/// caller ACKs the JetStream message.
#[derive(Debug)]
pub enum HandleError {
    /// Security-relevant rejection. Ack with Term — no redelivery will help.
    Unauthorized { reason: &'static str, detail: String },
    /// Transient infra failure (DB, encrypt, network). Nak so JetStream
    /// redelivers up to `max_deliver`.
    Transient(anyhow::Error),
}

impl std::fmt::Display for HandleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandleError::Unauthorized { reason, detail } => write!(f, "unauthorized ({reason}): {detail}"),
            HandleError::Transient(err) => write!(f, "transient: {err}"),
        }
    }
}

impl std::error::Error for HandleError {}

pub(crate) const TIMESTAMP_REPLAY_WINDOW_SECS: i64 = 300;
pub const CREDENTIALS_STREAM: &str = "CREDENTIALS";
pub const CREDENTIALS_DURABLE: &str = "credential-sync-handler";
/// Filter subject for the credential-sync consumer. Must match the stream's
/// subject set — use the shared helper so the two stay in sync.
pub fn credentials_filter() -> String {
    creds_subject_wildcard()
}
const FETCH_BATCH_SIZE: usize = 8;
const FETCH_TIMEOUT_MS: u64 = 500;
const ACK_WAIT_SECS: u64 = 30;
const MAX_DELIVER: i64 = 5;

/// Resolves `agent_id → (organization_id, user_id)`. Modeled as a trait so
/// tests swap in an in-memory implementation.
#[async_trait]
pub trait AgentOwnerLookup: Clone + Send + Sync + 'static {
    async fn find_owner(&self, agent_id: Uuid) -> Result<Option<AgentOwner>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentOwner {
    pub organization_id: Uuid,
    pub user_id: Uuid,
}

/// Fetches the per-agent HMAC secret. Duplicated from
/// `orchestration_result_consumer` rather than shared because each consumer
/// may grow its own lookup semantics.
#[async_trait]
pub trait HmacSecretLookup: Clone + Send + Sync + 'static {
    async fn find_secret(&self, agent_id: Uuid) -> Result<Option<String>>;
}

/// Writes an encrypted credential blob to `user_cli_credentials`.
#[async_trait]
pub trait CredentialWriter: Clone + Send + Sync + 'static {
    async fn upsert(&self, user_id: Uuid, cli_tool: &str, plaintext_json: &str) -> Result<()>;
}

fn reject_unauthorized(reason: &'static str, detail: impl std::fmt::Display) -> HandleError {
    metrics::counter!("credential_sync_unauthorized_total", "reason" => reason).increment(1);
    HandleError::Unauthorized { reason, detail: detail.to_string() }
}

pub async fn handle_message<O, H, W>(
    owners: &O,
    hmac: &H,
    writer: &W,
    subject: &str,
    payload: &[u8],
) -> Result<(), HandleError>
where
    O: AgentOwnerLookup,
    H: HmacSecretLookup,
    W: CredentialWriter,
{
    let subject_agent = match parse_agent_id_from_creds_subject(subject) {
        Some(id) => id,
        None => return Err(reject_unauthorized("bad_subject", format!("subject {subject}"))),
    };

    let envelope: SignedEnvelope = match serde_json::from_slice(payload) {
        Ok(e) => e,
        Err(err) => return Err(reject_unauthorized("envelope_decode_failed", format!("{err}"))),
    };

    if envelope.agent_id != subject_agent.to_string() {
        return Err(reject_unauthorized(
            "envelope_agent_mismatch",
            format!("subject {subject_agent} vs envelope {}", envelope.agent_id),
        ));
    }

    let now_secs = chrono::Utc::now().timestamp();
    if (now_secs - envelope.timestamp).abs() > TIMESTAMP_REPLAY_WINDOW_SECS {
        return Err(reject_unauthorized(
            "timestamp_outside_window",
            format!("envelope ts {} vs now {now_secs}", envelope.timestamp),
        ));
    }

    let secret = match hmac.find_secret(subject_agent).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err(reject_unauthorized("agent_unknown", format!("no hmac_secret for {subject_agent}")));
        }
        Err(err) => {
            metrics::counter!("credential_sync_transient_errors_total", "stage" => "hmac_lookup").increment(1);
            return Err(HandleError::Transient(err));
        }
    };

    if !envelope.verify(secret.as_bytes()) {
        return Err(reject_unauthorized("signature_mismatch", format!("agent {subject_agent}")));
    }

    let msg: CredentialSyncMessage = match serde_json::from_value(envelope.payload) {
        Ok(m) => m,
        Err(err) => return Err(reject_unauthorized("file_map_invalid", format!("decode payload: {err}"))),
    };

    if msg.files.len() > MAX_CREDENTIAL_FILES {
        return Err(reject_unauthorized("payload_oversized", format!("file count {}", msg.files.len())));
    }
    let total: usize = msg.files.values().map(|s| s.len()).sum();
    if total > MAX_CREDENTIAL_TOTAL_BYTES {
        return Err(reject_unauthorized("payload_oversized", format!("total bytes {total}")));
    }
    if let Some((name, value)) = msg.files.iter().find(|(_, v)| v.len() > MAX_CREDENTIAL_FILE_BYTES) {
        return Err(reject_unauthorized("payload_oversized", format!("{name} {}", value.len())));
    }

    if msg.agent_id != subject_agent {
        return Err(reject_unauthorized(
            "payload_agent_mismatch",
            format!("payload agent {} vs subject {subject_agent}", msg.agent_id),
        ));
    }

    if CliToolKind::parse_legacy(&msg.cli_tool).is_err() {
        return Err(reject_unauthorized("cli_tool_unknown", msg.cli_tool.to_string()));
    }

    let owner = match owners.find_owner(subject_agent).await {
        Ok(Some(o)) => o,
        Ok(None) => {
            return Err(reject_unauthorized("agent_row_missing", format!("no agent row for {subject_agent}")));
        }
        Err(err) => {
            metrics::counter!("credential_sync_transient_errors_total", "stage" => "owner_lookup").increment(1);
            return Err(HandleError::Transient(err));
        }
    };

    if msg.organization_id != owner.organization_id {
        return Err(reject_unauthorized(
            "payload_org_mismatch",
            format!("payload org {} vs db org {}", msg.organization_id, owner.organization_id),
        ));
    }

    let plaintext = serde_json::to_string(&msg.files).map_err(|e| {
        metrics::counter!("credential_sync_transient_errors_total", "stage" => "payload_serialize").increment(1);
        HandleError::Transient(anyhow!(e))
    })?;
    let start = std::time::Instant::now();
    writer.upsert(owner.user_id, &msg.cli_tool, &plaintext).await.map_err(|e| {
        metrics::counter!("credential_sync_transient_errors_total", "stage" => "writer_upsert").increment(1);
        HandleError::Transient(e)
    })?;
    let elapsed = start.elapsed().as_secs_f64();
    metrics::histogram!(
        "credential_sync_persist_duration_seconds",
        "cli_tool" => msg.cli_tool.clone(),
    )
    .record(elapsed);
    metrics::counter!(
        "credential_sync_received_total",
        "cli_tool" => msg.cli_tool,
    )
    .increment(1);
    Ok(())
}

pub struct CredentialStreamWorker<O, H, W> {
    consumer: PullConsumer,
    owners: O,
    hmac: H,
    writer: W,
}

impl<O, H, W> CredentialStreamWorker<O, H, W>
where
    O: AgentOwnerLookup,
    H: HmacSecretLookup,
    W: CredentialWriter,
{
    pub async fn connect(client: async_nats::Client, owners: O, hmac: H, writer: W) -> Result<Self> {
        let js = jetstream::new(client);
        let stream = js.get_stream(CREDENTIALS_STREAM).await?;
        let consumer = stream
            .get_or_create_consumer(
                CREDENTIALS_DURABLE,
                pull::Config {
                    durable_name: Some(CREDENTIALS_DURABLE.to_string()),
                    ack_policy: consumer::AckPolicy::Explicit,
                    ack_wait: Duration::from_secs(ACK_WAIT_SECS),
                    max_deliver: MAX_DELIVER,
                    filter_subject: credentials_filter(),
                    ..Default::default()
                },
            )
            .await?;
        Ok(Self { consumer, owners, hmac, writer })
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_ok() && *shutdown.borrow() { break; }
                }
                batch = self.consumer.fetch().max_messages(FETCH_BATCH_SIZE).expires(Duration::from_millis(FETCH_TIMEOUT_MS)).messages() => {
                    match batch {
                        Ok(mut messages) => {
                            while let Some(msg) = messages.next().await {
                                let Ok(msg) = msg else { break; };
                                let subject = msg.subject.to_string();
                                let payload = msg.payload.to_vec();
                                match handle_message(&self.owners, &self.hmac, &self.writer, &subject, &payload).await {
                                    Ok(()) => {
                                        if let Err(err) = msg.ack().await {
                                            metrics::counter!("credential_sync_ack_errors_total", "kind" => "success").increment(1);
                                            tracing::warn!(error = %err, %subject, "credential sync ack failed; will redeliver");
                                        }
                                    }
                                    Err(HandleError::Unauthorized { reason, detail }) => {
                                        tracing::warn!(%reason, %detail, %subject, "credential sync rejected");
                                        let _ = msg.ack_with(AckKind::Term).await;
                                    }
                                    Err(HandleError::Transient(err)) => {
                                        tracing::warn!(error = %err, %subject, "credential sync transient error; will redeliver");
                                        // Nak with None backoff → redeliver after ack_wait (30s). max_deliver=5 bounds retries.
                                        let _ = msg.ack_with(AckKind::Nak(None)).await;
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, "credential sync batch fetch failed");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }
    }
}

/// Production adapter: `agents` table → `(organization_id, user_id)`.
#[derive(Clone)]
pub struct SqlxAgentOwnerLookup {
    pool: PgPool,
}

impl SqlxAgentOwnerLookup {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentOwnerLookup for SqlxAgentOwnerLookup {
    async fn find_owner(&self, agent_id: Uuid) -> Result<Option<AgentOwner>> {
        let row: Option<(Uuid, Uuid)> =
            sqlx::query_as(r#"SELECT organization_id, user_id FROM agents WHERE id = $1 LIMIT 1"#)
                .bind(agent_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(organization_id, user_id)| AgentOwner { organization_id, user_id }))
    }
}

/// Production adapter: reuses `agents.hmac_secret` (migration 025).
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

/// Describe + prime credential-sync metrics at zero so Prometheus scrape
/// returns the metric even before any event fires. `describe_*` sets help
/// text only; an explicit `increment(0)` / `record(0.0)` primes the sample
/// so dashboards render from t=0 instead of "metric not found".
pub fn register_metrics() {
    metrics::describe_counter!(
        "credential_sync_published_total",
        "Credential sync envelopes published by a sidecar watcher"
    );
    metrics::describe_counter!(
        "credential_sync_received_total",
        "Credential sync envelopes persisted by the backend consumer"
    );
    metrics::describe_counter!(
        "credential_sync_unauthorized_total",
        "Credential sync envelopes dropped by the backend; label = reason"
    );
    metrics::describe_counter!(
        "credential_sync_publish_errors_total",
        "Producer-side credential sync publish errors emitted by the sidecar; label = reason"
    );
    metrics::describe_counter!(
        "credential_sync_transient_errors_total",
        "Consumer-side transient failures (DB, encrypt) that will be redelivered"
    );
    metrics::describe_counter!(
        "credential_sync_ack_errors_total",
        "JetStream ack send failures (fire-and-forget ack.await returned Err)"
    );
    metrics::describe_histogram!(
        "credential_sync_persist_duration_seconds",
        "Time spent encrypting and upserting a credential blob"
    );
    // Prime so the metric exists on /metrics before first event.
    metrics::counter!("credential_sync_published_total", "cli_tool" => "unknown").increment(0);
    metrics::counter!("credential_sync_received_total", "cli_tool" => "unknown").increment(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, HashMap};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    const TEST_HMAC: &str = "hmac-creds-test";

    #[derive(Clone, Default)]
    struct FakeOwners {
        by_agent: Arc<HashMap<Uuid, AgentOwner>>,
    }
    #[async_trait]
    impl AgentOwnerLookup for FakeOwners {
        async fn find_owner(&self, agent_id: Uuid) -> anyhow::Result<Option<AgentOwner>> {
            Ok(self.by_agent.get(&agent_id).cloned())
        }
    }

    /// FakeOwners variant that always returns an error (simulates DB failure).
    #[derive(Clone, Default)]
    struct FakeOwnersErr;
    #[async_trait]
    impl AgentOwnerLookup for FakeOwnersErr {
        async fn find_owner(&self, _agent_id: Uuid) -> anyhow::Result<Option<AgentOwner>> {
            Err(anyhow::anyhow!("db connection lost"))
        }
    }

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
        async fn find_secret(&self, agent_id: Uuid) -> anyhow::Result<Option<String>> {
            Ok(self.by_agent.get(&agent_id).cloned())
        }
    }

    #[derive(Clone, Default)]
    struct FakeWriter {
        stored: Arc<Mutex<Vec<(Uuid, String, String)>>>,
    }
    #[async_trait]
    impl CredentialWriter for FakeWriter {
        async fn upsert(&self, user_id: Uuid, cli_tool: &str, plaintext_json: &str) -> anyhow::Result<()> {
            self.stored.lock().await.push((user_id, cli_tool.to_string(), plaintext_json.to_string()));
            Ok(())
        }
    }

    /// FakeWriter variant that always returns an error (simulates encrypt/DB failure).
    #[derive(Clone, Default)]
    struct FakeWriterErr;
    #[async_trait]
    impl CredentialWriter for FakeWriterErr {
        async fn upsert(&self, _user_id: Uuid, _cli_tool: &str, _plaintext_json: &str) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("encryption service unavailable"))
        }
    }

    fn sample_message(agent_id: Uuid, org_id: Uuid, cli: &str) -> CredentialSyncMessage {
        let mut files = BTreeMap::new();
        files.insert("auth.json".into(), r#"{"ok":true}"#.into());
        CredentialSyncMessage { agent_id, organization_id: org_id, cli_tool: cli.into(), files }
    }

    /// Build a signed envelope. NOTE: use `SignedEnvelope::sign(secret, agent_id, ts, &value)`;
    /// there is no `sign_value` helper despite what the plan text says.
    fn envelope_for(secret: &str, agent_id: Uuid, msg: &CredentialSyncMessage, ts_override: Option<i64>) -> Vec<u8> {
        let ts = ts_override.unwrap_or_else(|| chrono::Utc::now().timestamp());
        let value = serde_json::to_value(msg).unwrap();
        let env = SignedEnvelope::sign(secret.as_bytes(), &agent_id.to_string(), ts, &value).unwrap();
        serde_json::to_vec(&env).unwrap()
    }

    fn subject(agent_id: Uuid) -> String {
        format!("creds.{agent_id}")
    }

    #[tokio::test]
    async fn rejects_bad_subject() {
        let writer = FakeWriter::default();
        let owners = FakeOwners::default();
        let hmac = FakeHmac::default();
        let err = handle_message(&owners, &hmac, &writer, "credentials.xyz", &[]).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "bad_subject", .. }), "{err}");
        assert!(writer.stored.lock().await.is_empty());
    }

    #[tokio::test]
    async fn rejects_envelope_agent_mismatch_with_subject() {
        let subject_agent = Uuid::new_v4();
        let other_agent = Uuid::new_v4();
        let owners = FakeOwners {
            by_agent: Arc::new(HashMap::from([(
                subject_agent,
                AgentOwner { organization_id: Uuid::new_v4(), user_id: Uuid::new_v4() },
            )])),
        };
        let hmac = FakeHmac::with(subject_agent, TEST_HMAC);
        let writer = FakeWriter::default();
        let msg = sample_message(other_agent, Uuid::new_v4(), "claude");
        let payload = envelope_for(TEST_HMAC, other_agent, &msg, None);
        let err = handle_message(&owners, &hmac, &writer, &subject(subject_agent), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "envelope_agent_mismatch", .. }), "{err}");
        assert!(writer.stored.lock().await.is_empty());
    }

    #[tokio::test]
    async fn rejects_envelope_outside_replay_window() {
        let agent_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let owners = FakeOwners {
            by_agent: Arc::new(HashMap::from([(
                agent_id,
                AgentOwner { organization_id: org_id, user_id: Uuid::new_v4() },
            )])),
        };
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriter::default();
        let msg = sample_message(agent_id, org_id, "claude");
        let stale = chrono::Utc::now().timestamp() - (TIMESTAMP_REPLAY_WINDOW_SECS + 60);
        let payload = envelope_for(TEST_HMAC, agent_id, &msg, Some(stale));
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "timestamp_outside_window", .. }), "{err}");
        assert!(writer.stored.lock().await.is_empty());
    }

    #[tokio::test]
    async fn rejects_signature_mismatch() {
        let agent_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let owners = FakeOwners {
            by_agent: Arc::new(HashMap::from([(
                agent_id,
                AgentOwner { organization_id: org_id, user_id: Uuid::new_v4() },
            )])),
        };
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriter::default();
        let msg = sample_message(agent_id, org_id, "claude");
        let payload = envelope_for("different-key", agent_id, &msg, None);
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "signature_mismatch", .. }), "{err}");
        assert!(writer.stored.lock().await.is_empty());
    }

    #[tokio::test]
    async fn rejects_unknown_cli_tool() {
        let agent_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let owners = FakeOwners {
            by_agent: Arc::new(HashMap::from([(agent_id, AgentOwner { organization_id: org_id, user_id })])),
        };
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriter::default();
        let msg = sample_message(agent_id, org_id, "nvim");
        let payload = envelope_for(TEST_HMAC, agent_id, &msg, None);
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "cli_tool_unknown", .. }), "{err}");
        assert!(writer.stored.lock().await.is_empty());
    }

    #[tokio::test]
    async fn rejects_payload_over_total_cap() {
        let agent_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let owners = FakeOwners {
            by_agent: Arc::new(HashMap::from([(agent_id, AgentOwner { organization_id: org_id, user_id })])),
        };
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriter::default();
        let mut msg = sample_message(agent_id, org_id, "claude");
        msg.files.insert("big.json".into(), "x".repeat(MAX_CREDENTIAL_TOTAL_BYTES + 1));
        let payload = envelope_for(TEST_HMAC, agent_id, &msg, None);
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "payload_oversized", .. }), "{err}");
        assert!(writer.stored.lock().await.is_empty());
    }

    #[tokio::test]
    async fn persists_valid_message() {
        let agent_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let owners = FakeOwners {
            by_agent: Arc::new(HashMap::from([(agent_id, AgentOwner { organization_id: org_id, user_id })])),
        };
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriter::default();
        let msg = sample_message(agent_id, org_id, "claude");
        let payload = envelope_for(TEST_HMAC, agent_id, &msg, None);
        handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap();
        let stored = writer.stored.lock().await;
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].0, user_id);
        assert_eq!(stored[0].1, "claude");
        assert!(stored[0].2.contains("auth.json"));
    }

    #[tokio::test]
    async fn rejects_envelope_org_mismatch() {
        let agent_id = Uuid::new_v4();
        let db_org = Uuid::new_v4();
        let forged_org = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let owners = FakeOwners {
            by_agent: Arc::new(HashMap::from([(agent_id, AgentOwner { organization_id: db_org, user_id })])),
        };
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriter::default();
        let msg = sample_message(agent_id, forged_org, "claude");
        let payload = envelope_for(TEST_HMAC, agent_id, &msg, None);
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "payload_org_mismatch", .. }), "{err}");
        assert!(writer.stored.lock().await.is_empty());
    }

    #[tokio::test]
    async fn rejects_envelope_vs_payload_agent_mismatch() {
        // Envelope says agent_id = X and is signed by X's secret. Payload's
        // inner msg.agent_id = Y. Must reject with `payload_agent_mismatch`.
        let agent_id = Uuid::new_v4();
        let forged_payload_agent = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let owners = FakeOwners {
            by_agent: Arc::new(HashMap::from([(agent_id, AgentOwner { organization_id: org_id, user_id })])),
        };
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriter::default();
        let mut msg = sample_message(agent_id, org_id, "claude");
        msg.agent_id = forged_payload_agent; // payload lies about which agent
        let payload = envelope_for(TEST_HMAC, agent_id, &msg, None);
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "payload_agent_mismatch", .. }), "{err}");
        assert!(writer.stored.lock().await.is_empty());
    }

    #[tokio::test]
    async fn rejects_agent_row_missing() {
        // Valid envelope but no agent row in the DB — hit after all signature
        // + payload checks pass.
        let agent_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let owners = FakeOwners::default(); // intentionally empty — agent has secret but no row
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriter::default();
        let msg = sample_message(agent_id, org_id, "claude");
        let payload = envelope_for(TEST_HMAC, agent_id, &msg, None);
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "agent_row_missing", .. }), "{err}");
        assert!(writer.stored.lock().await.is_empty());
    }

    // --- New Fix-1 transient tests ---

    #[tokio::test]
    async fn transient_writer_error_naks_not_terms() {
        // FakeWriter returns Err — assert HandleError::Transient, not Unauthorized.
        let agent_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let owners = FakeOwners {
            by_agent: Arc::new(HashMap::from([(agent_id, AgentOwner { organization_id: org_id, user_id })])),
        };
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriterErr;
        let msg = sample_message(agent_id, org_id, "claude");
        let payload = envelope_for(TEST_HMAC, agent_id, &msg, None);
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Transient(_)), "expected Transient, got {err}");
    }

    #[tokio::test]
    async fn transient_owner_lookup_error_naks_not_terms() {
        // FakeOwnersErr returns Err — assert HandleError::Transient, not Unauthorized.
        let agent_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let owners = FakeOwnersErr;
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriter::default();
        let msg = sample_message(agent_id, org_id, "claude");
        let payload = envelope_for(TEST_HMAC, agent_id, &msg, None);
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Transient(_)), "expected Transient, got {err}");
        assert!(writer.stored.lock().await.is_empty());
    }

    // --- Fix 7: additional reject-reason tests ---

    #[tokio::test]
    async fn rejects_malformed_envelope_bytes() {
        let agent_id = Uuid::new_v4();
        let owners = FakeOwners::default();
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriter::default();
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &[0xff, 0xfe, 0xfd]).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "envelope_decode_failed", .. }), "{err}");
        assert!(writer.stored.lock().await.is_empty());
    }

    #[tokio::test]
    async fn rejects_agent_unknown_no_hmac_secret() {
        let agent_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let owners = FakeOwners {
            by_agent: Arc::new(HashMap::from([(
                agent_id,
                AgentOwner { organization_id: org_id, user_id: Uuid::new_v4() },
            )])),
        };
        let hmac = FakeHmac::default(); // EMPTY — agent has no secret registered
        let writer = FakeWriter::default();
        let msg = sample_message(agent_id, org_id, "claude");
        let payload = envelope_for(TEST_HMAC, agent_id, &msg, None);
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "agent_unknown", .. }), "{err}");
        assert!(writer.stored.lock().await.is_empty());
    }

    #[tokio::test]
    async fn rejects_file_map_invalid_payload_shape() {
        // Sign a non-CredentialSyncMessage payload so from_value fails.
        let agent_id = Uuid::new_v4();
        let owners = FakeOwners {
            by_agent: Arc::new(HashMap::from([(
                agent_id,
                AgentOwner { organization_id: Uuid::new_v4(), user_id: Uuid::new_v4() },
            )])),
        };
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriter::default();
        let ts = chrono::Utc::now().timestamp();
        let bogus = serde_json::json!({"not": "a credential sync"});
        let env = SignedEnvelope::sign(TEST_HMAC.as_bytes(), &agent_id.to_string(), ts, &bogus).unwrap();
        let payload = serde_json::to_vec(&env).unwrap();
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "file_map_invalid", .. }), "{err}");
    }

    #[tokio::test]
    async fn rejects_single_file_over_per_file_cap() {
        let agent_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let owners = FakeOwners {
            by_agent: Arc::new(HashMap::from([(agent_id, AgentOwner { organization_id: org_id, user_id })])),
        };
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriter::default();
        let mut msg = sample_message(agent_id, org_id, "claude");
        // Single file at MAX_FILE + 1 bytes; total stays under TOTAL cap if we clear auth.json.
        msg.files.clear();
        msg.files.insert("big.json".into(), "x".repeat(MAX_CREDENTIAL_FILE_BYTES + 1));
        let payload = envelope_for(TEST_HMAC, agent_id, &msg, None);
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "payload_oversized", .. }), "{err}");
    }

    #[tokio::test]
    async fn rejects_too_many_files_in_payload() {
        let agent_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let owners = FakeOwners {
            by_agent: Arc::new(HashMap::from([(agent_id, AgentOwner { organization_id: org_id, user_id })])),
        };
        let hmac = FakeHmac::with(agent_id, TEST_HMAC);
        let writer = FakeWriter::default();
        let mut msg = sample_message(agent_id, org_id, "claude");
        msg.files.clear();
        for i in 0..(MAX_CREDENTIAL_FILES + 1) {
            msg.files.insert(format!("f{i}.json"), "x".into());
        }
        let payload = envelope_for(TEST_HMAC, agent_id, &msg, None);
        let err = handle_message(&owners, &hmac, &writer, &subject(agent_id), &payload).await.unwrap_err();
        assert!(matches!(err, HandleError::Unauthorized { reason: "payload_oversized", .. }), "{err}");
    }
}
