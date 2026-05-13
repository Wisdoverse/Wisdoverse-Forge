//! NATS-backed event consumer for sidecar event ingestion.
//!
//! Consumes signed envelopes from `events.ingest.<agent_id>`, normalizes them into
//! the Rust event schema, updates agent status for lifecycle transitions, and
//! republishes a legacy-compatible broadcast envelope on `broadcast.<org_id>`.

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use async_nats::jetstream::consumer::{self, PullConsumer, pull};
use async_nats::jetstream::{self, AckKind};
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::watch;
use uuid::Uuid;

use agentforge_core::AgentStatus;

pub const EVENTS_STREAM: &str = "EVENTS";
// Reuse the legacy durable on the shared workqueue stream instead of trying
// to create a second filtered consumer, which JetStream rejects.
const EVENTS_DURABLE: &str = "event-processor";
pub const EVENTS_FILTER: &str = "events.>";
const FETCH_BATCH_SIZE: usize = 32;
const FETCH_TIMEOUT_MS: u64 = 500;
const ACK_WAIT_SECS: u64 = 30;
const MAX_DELIVER: i64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignedEventPayload {
    pub event_type: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignedEventEnvelope {
    pub payload: SignedEventPayload,
    pub timestamp: i64,
    pub agent_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTarget {
    pub agent_id: Uuid,
    pub organization_id: Uuid,
    pub cli_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedEvent {
    pub organization_id: Uuid,
    pub agent_id: Uuid,
    pub event_type: String,
    pub payload: Value,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BroadcastMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    #[serde(rename = "eventType")]
    pub event_type: String,
    #[serde(rename = "eventData")]
    pub event_data: Value,
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "orgId")]
    pub org_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnInvalidatePayload {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnInvalidateMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub payload: TurnInvalidatePayload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BroadcastEnvelope {
    Event(BroadcastMessage),
    TurnInvalidate(TurnInvalidateMessage),
}

#[derive(Debug, thiserror::Error)]
pub enum ConsumeError {
    #[error("permanent event rejection: {0}")]
    Permanent(#[source] anyhow::Error),
    #[error("transient event processing failure: {0}")]
    Transient(#[source] anyhow::Error),
}

impl ConsumeError {
    fn permanent(message: impl Into<anyhow::Error>) -> Self {
        Self::Permanent(message.into())
    }

    fn transient(message: impl Into<anyhow::Error>) -> Self {
        Self::Transient(message.into())
    }
}

#[async_trait]
pub trait EventStore: Clone + Send + Sync + 'static {
    async fn persist(&self, event: PersistedEvent) -> Result<()>;
}

/// Columns on `agents` that the event consumer refreshes in lockstep with
/// event ingest. Assembled per-event by [`derive_runtime_patch`].
///
/// Missing columns from the frontend `ManagedAgent` contract (issue #30) —
/// `cwd` / `current_tool` — were previously read-only after agent creation
/// because nothing wrote to them. This patch fixes that by having the event
/// consumer mirror the runtime state that the CLI reports via hook payloads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentRuntimePatch {
    /// When `Some`, updates `agents.status` to this value.
    pub status: Option<AgentStatus>,
    /// When `Some`, controls `agents.current_tool`:
    /// - `CurrentToolUpdate::Set(name)` writes the tool name.
    /// - `CurrentToolUpdate::Clear` nulls out the column.
    /// - `None` leaves the column untouched.
    pub current_tool: Option<CurrentToolUpdate>,
    /// When `Some`, overwrites `agents.cwd`. `None` leaves it alone.
    ///
    /// The hook protocol emits `cwd` on every event, so this is usually set
    /// whenever the payload carries a non-empty `cwd` string. An empty string
    /// is treated as "no cwd reported" and skipped.
    pub cwd: Option<String>,
}

/// Two distinct update intents for `agents.current_tool`: write a new value
/// versus null the column out. Kept separate from `Option<String>` so the
/// "clear" case is not confused with "leave untouched".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentToolUpdate {
    Set(String),
    Clear,
}

impl AgentRuntimePatch {
    /// `true` when the patch would not change any column, so the consumer can
    /// skip the round-trip to Postgres.
    pub fn is_noop(&self) -> bool {
        self.status.is_none() && self.current_tool.is_none() && self.cwd.is_none()
    }
}

#[async_trait]
pub trait AgentDirectory: Clone + Send + Sync + 'static {
    async fn resolve(&self, agent_id: Uuid) -> Result<Option<AgentTarget>>;

    /// Apply one or more `agents` column updates derived from the event
    /// payload. Returns `true` when the row existed and was updated, `false`
    /// when the row has disappeared (the caller treats the latter as
    /// permanent).
    async fn apply_runtime_patch(&self, agent_id: Uuid, patch: AgentRuntimePatch) -> Result<bool>;
}

#[async_trait]
pub trait BroadcastBus: Clone + Send + Sync + 'static {
    async fn publish(&self, subject: String, message: BroadcastEnvelope) -> Result<()>;
}

#[derive(Debug, Clone)]
struct DecodedEvent {
    persisted: PersistedEvent,
    broadcast: BroadcastMessage,
    turn_invalidate: Option<TurnInvalidateMessage>,
    broadcast_subject: String,
    runtime_patch: AgentRuntimePatch,
    persistable: bool,
}

#[derive(Clone)]
pub struct EventConsumer<S, A, B> {
    store: S,
    agents: A,
    broadcast: B,
}

impl<S, A, B> EventConsumer<S, A, B>
where
    S: EventStore,
    A: AgentDirectory,
    B: BroadcastBus,
{
    pub fn new(store: S, agents: A, broadcast: B) -> Self {
        Self { store, agents, broadcast }
    }

    pub async fn handle(&self, subject: &str, envelope: SignedEventEnvelope) -> std::result::Result<(), ConsumeError> {
        let agent_id = parse_subject_agent_id(subject)?;
        if envelope.agent_id != agent_id.to_string() {
            return Err(ConsumeError::permanent(anyhow!(
                "subject agent_id {} does not match envelope agent_id {}",
                agent_id,
                envelope.agent_id
            )));
        }

        let target = self
            .agents
            .resolve(agent_id)
            .await
            .map_err(ConsumeError::transient)?
            .ok_or_else(|| ConsumeError::permanent(anyhow!("agent {agent_id} not found")))?;

        let decoded = decode_event(target, envelope)?;

        if !decoded.runtime_patch.is_noop() {
            let updated = self
                .agents
                .apply_runtime_patch(decoded.persisted.agent_id, decoded.runtime_patch.clone())
                .await
                .map_err(ConsumeError::transient)?;
            if !updated {
                return Err(ConsumeError::permanent(anyhow!(
                    "agent {} disappeared during runtime patch",
                    decoded.persisted.agent_id
                )));
            }
        }

        if decoded.persistable {
            self.store.persist(decoded.persisted).await.map_err(ConsumeError::transient)?;
        }

        self.broadcast
            .publish(decoded.broadcast_subject.clone(), BroadcastEnvelope::Event(decoded.broadcast))
            .await
            .map_err(ConsumeError::transient)?;

        if let Some(turn_invalidate) = decoded.turn_invalidate {
            self.broadcast
                .publish(decoded.broadcast_subject, BroadcastEnvelope::TurnInvalidate(turn_invalidate))
                .await
                .map_err(ConsumeError::transient)?;
        }

        Ok(())
    }
}

fn parse_subject_agent_id(subject: &str) -> std::result::Result<Uuid, ConsumeError> {
    let mut parts = subject.split('.');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("events"), Some("ingest"), Some(agent_id), None) => Uuid::parse_str(agent_id)
            .map_err(|err| ConsumeError::permanent(anyhow!("invalid events.ingest subject {subject}: {err}"))),
        _ => Err(ConsumeError::permanent(anyhow!("unsupported event subject {subject}"))),
    }
}

fn decode_event(target: AgentTarget, envelope: SignedEventEnvelope) -> std::result::Result<DecodedEvent, ConsumeError> {
    let session_id = extract_session_id(&envelope.payload.data)
        .or(target.cli_session_id.clone())
        .or_else(|| Some(target.agent_id.to_string()));

    let event_data = normalize_event_data(
        envelope.payload.event_type.clone(),
        envelope.payload.data,
        session_id.clone(),
        target.organization_id,
        envelope.timestamp,
    )?;

    let broadcast_agent_id = session_id.clone().unwrap_or_else(|| target.agent_id.to_string());
    let event_type = envelope.payload.event_type;
    let organization_id = target.organization_id;
    let persistable = is_persistable(&event_type);
    let event_timestamp_ms = event_data.get("timestamp").and_then(json_i64).unwrap_or(envelope.timestamp * 1000);

    let runtime_patch = derive_runtime_patch(&event_type, &event_data);

    Ok(DecodedEvent {
        broadcast_subject: format!("broadcast.{organization_id}"),
        broadcast: BroadcastMessage {
            message_type: "event".to_string(),
            event_type: event_type.clone(),
            event_data: event_data.clone(),
            agent_id: broadcast_agent_id,
            org_id: organization_id.to_string(),
        },
        turn_invalidate: persistable.then(|| TurnInvalidateMessage {
            message_type: "turn_invalidate".to_string(),
            payload: TurnInvalidatePayload { agent_id: target.agent_id.to_string(), timestamp: event_timestamp_ms },
        }),
        persisted: PersistedEvent {
            organization_id,
            agent_id: target.agent_id,
            event_type: event_type.clone(),
            payload: event_data,
            session_id,
        },
        runtime_patch,
        persistable,
    })
}

fn json_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_u64().and_then(|n| i64::try_from(n).ok()))
}

fn normalize_event_data(
    event_type: String,
    data: Value,
    session_id: Option<String>,
    organization_id: Uuid,
    envelope_timestamp_secs: i64,
) -> std::result::Result<Value, ConsumeError> {
    let mut object = match data {
        Value::Object(map) => map,
        _ => {
            return Err(ConsumeError::permanent(anyhow!("event payload data must be a JSON object")));
        }
    };

    object.entry("type".to_string()).or_insert_with(|| Value::String(event_type));
    object.entry("orgId".to_string()).or_insert_with(|| Value::String(organization_id.to_string()));
    if let Some(session_id) = session_id {
        object.entry("sessionId".to_string()).or_insert_with(|| Value::String(session_id));
    }
    object.entry("timestamp".to_string()).or_insert_with(|| Value::from(envelope_timestamp_secs * 1000));
    object.entry("id".to_string()).or_insert_with(|| Value::String(Uuid::now_v7().to_string()));

    Ok(Value::Object(object))
}

fn extract_session_id(data: &Value) -> Option<String> {
    data.as_object().and_then(|map| {
        map.get("sessionId").or_else(|| map.get("session_id")).and_then(Value::as_str).map(str::to_owned)
    })
}

fn is_persistable(event_type: &str) -> bool {
    !matches!(event_type, "token_update")
}

fn derive_status(event_type: &str) -> Option<AgentStatus> {
    match event_type {
        "pre_tool_use" | "user_prompt_submit" => Some(AgentStatus::Working),
        "stop" | "session_end" => Some(AgentStatus::Idle),
        _ => None,
    }
}

/// Hard upper bound on any `agents` column written from a sidecar-supplied
/// string (currently `current_tool` and `cwd`). Realistic tool names are
/// <64 chars and realistic paths are <4KiB; this ceiling is generous for
/// legitimate input and tight enough to prevent a rogue sidecar from
/// writing megabyte-sized strings into the row and degrading every
/// downstream read (admin listing, ManagedAgent hydration, WebSocket frame).
///
/// Oversize values are dropped (not truncated) — a truncated path would be
/// silently wrong (`/home/user/pro` instead of the real directory) and more
/// confusing than "no write". The caller's existing column value is left
/// untouched.
const MAX_RUNTIME_STRING_LEN: usize = 4096;

/// Read a non-empty string field from a normalized event payload. Empty
/// strings are treated as "field not reported" because the hook serializer
/// writes `""` for missing optional strings. Values exceeding
/// [`MAX_RUNTIME_STRING_LEN`] are also rejected — see the constant's doc.
fn payload_string<'a>(data: &'a Value, key: &str) -> Option<&'a str> {
    data.get(key).and_then(Value::as_str).filter(|s| !s.is_empty() && s.len() <= MAX_RUNTIME_STRING_LEN)
}

/// Build the full `agents` column patch for an event. Centralizes the mapping
/// between hook event types and runtime-column writes so the consumer only
/// issues one UPDATE per event regardless of how many columns move.
///
/// Issue #30: `cwd` and `current_tool` previously stayed NULL forever because
/// nothing wrote to them. `pre_tool_use` now sets the tool; `post_tool_use`
/// clears it (the tool has finished); `stop` / `session_end` also clear.
/// Every event with a non-empty `cwd` refreshes the column so the frontend
/// "Working Dir" display survives page reload.
fn derive_runtime_patch(event_type: &str, data: &Value) -> AgentRuntimePatch {
    let status = derive_status(event_type);

    let current_tool = match event_type {
        "pre_tool_use" => payload_string(data, "tool").map(|t| CurrentToolUpdate::Set(t.to_owned())),
        "post_tool_use" | "stop" | "session_end" => Some(CurrentToolUpdate::Clear),
        _ => None,
    };

    let cwd = payload_string(data, "cwd").map(str::to_owned);

    AgentRuntimePatch { status, current_tool, cwd }
}

#[derive(Clone)]
pub struct SqlxEventStore {
    pool: PgPool,
}

impl SqlxEventStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventStore for SqlxEventStore {
    async fn persist(&self, event: PersistedEvent) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO events (organization_id, agent_id, event_type, payload, session_id)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(event.organization_id)
        .bind(event.agent_id)
        .bind(event.event_type)
        .bind(event.payload)
        .bind(event.session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct SqlxAgentDirectory {
    pool: PgPool,
}

impl SqlxAgentDirectory {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct AgentLookupRow {
    id: Uuid,
    organization_id: Uuid,
    cli_session_id: Option<String>,
}

#[async_trait]
impl AgentDirectory for SqlxAgentDirectory {
    async fn resolve(&self, agent_id: Uuid) -> Result<Option<AgentTarget>> {
        let row = sqlx::query_as::<_, AgentLookupRow>(
            r#"SELECT id, organization_id, cli_session_id
               FROM agents
               WHERE id = $1"#,
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| AgentTarget {
            agent_id: row.id,
            organization_id: row.organization_id,
            cli_session_id: row.cli_session_id,
        }))
    }

    async fn apply_runtime_patch(&self, agent_id: Uuid, patch: AgentRuntimePatch) -> Result<bool> {
        // Build one dynamic UPDATE per event so we never issue a no-op write.
        // The consumer has already checked `patch.is_noop()` before this call;
        // guard against accidental regressions anyway.
        if patch.is_noop() {
            return Ok(true);
        }

        let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE agents SET updated_at = NOW()");

        if let Some(status) = patch.status {
            builder.push(", status = ");
            builder.push_bind(status);
        }
        match patch.current_tool {
            Some(CurrentToolUpdate::Set(tool)) => {
                builder.push(", current_tool = ");
                builder.push_bind(tool);
            }
            Some(CurrentToolUpdate::Clear) => {
                // NULL is a literal here — no bind value, avoids pushing a
                // typed `Option<String>` that sqlx could choose to infer
                // incorrectly.
                builder.push(", current_tool = NULL");
            }
            None => {}
        }
        if let Some(cwd) = patch.cwd {
            builder.push(", cwd = ");
            builder.push_bind(cwd);
        }

        builder.push(" WHERE id = ");
        builder.push_bind(agent_id);

        let result = builder.build().execute(&self.pool).await?;
        Ok(result.rows_affected() == 1)
    }
}

#[derive(Clone)]
pub struct NatsBroadcastBus {
    client: async_nats::Client,
}

impl NatsBroadcastBus {
    pub fn new(client: async_nats::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl BroadcastBus for NatsBroadcastBus {
    async fn publish(&self, subject: String, message: BroadcastEnvelope) -> Result<()> {
        let bytes = serde_json::to_vec(&message)?;
        self.client.publish(subject, bytes.into()).await?;
        Ok(())
    }
}

pub struct EventStreamWorker {
    consumer: PullConsumer,
    logic: EventConsumer<SqlxEventStore, SqlxAgentDirectory, NatsBroadcastBus>,
}

impl EventStreamWorker {
    pub async fn connect(pool: PgPool, client: async_nats::Client) -> Result<Self> {
        let jetstream = jetstream::new(client.clone());
        let stream = jetstream
            .get_stream(EVENTS_STREAM)
            .await
            .with_context(|| format!("failed to open JetStream stream {EVENTS_STREAM}"))?;
        let consumer: PullConsumer = stream
            .get_or_create_consumer(
                EVENTS_DURABLE,
                pull::Config {
                    durable_name: Some(EVENTS_DURABLE.to_string()),
                    ack_policy: consumer::AckPolicy::Explicit,
                    ack_wait: Duration::from_secs(ACK_WAIT_SECS),
                    max_deliver: MAX_DELIVER,
                    filter_subject: EVENTS_FILTER.to_string(),
                    ..Default::default()
                },
            )
            .await
            .context("failed to create event ingest consumer")?;

        Ok(Self {
            consumer,
            logic: EventConsumer::new(
                SqlxEventStore::new(pool.clone()),
                SqlxAgentDirectory::new(pool),
                NatsBroadcastBus::new(client),
            ),
        })
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_ok() && *shutdown.borrow() {
                        tracing::info!(durable = EVENTS_DURABLE, "event stream worker shutting down");
                        break;
                    }
                }
                batch = self.consumer.fetch().max_messages(FETCH_BATCH_SIZE).expires(Duration::from_millis(FETCH_TIMEOUT_MS)).messages() => {
                    match batch {
                        Ok(mut messages) => {
                            while let Some(message) = messages.next().await {
                                match message {
                                    Ok(message) => {
                                        self.process_message(message).await;
                                    }
                                    Err(err) => {
                                        tracing::warn!(error = %err, "event stream worker fetch item failed");
                                        break;
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            tracing::warn!(error = %err, "event stream worker batch fetch failed");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }
    }

    async fn process_message(&self, message: async_nats::jetstream::Message) {
        let subject = message.subject.to_string();
        let envelope = match serde_json::from_slice::<SignedEventEnvelope>(&message.payload) {
            Ok(envelope) => envelope,
            Err(err) => {
                tracing::warn!(error = %err, %subject, "dropping malformed event envelope");
                if let Err(ack_err) = message.ack().await {
                    tracing::warn!(error = %ack_err, %subject, "failed to ack malformed event envelope");
                }
                return;
            }
        };

        match self.logic.handle(&subject, envelope).await {
            Ok(()) => {
                if let Err(err) = message.ack().await {
                    tracing::warn!(error = %err, %subject, "failed to ack processed event");
                }
            }
            Err(ConsumeError::Permanent(err)) => {
                tracing::warn!(error = %err, %subject, "dropping permanently invalid event");
                if let Err(ack_err) = message.ack().await {
                    tracing::warn!(error = %ack_err, %subject, "failed to ack rejected event");
                }
            }
            Err(ConsumeError::Transient(err)) => {
                tracing::warn!(error = %err, %subject, "transient event processing failure; requesting redelivery");
                if let Err(nak_err) = message.ack_with(AckKind::Nak(None)).await {
                    tracing::warn!(error = %nak_err, %subject, "failed to NAK event after transient failure");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    #[test]
    fn subject_parser_accepts_ingest_subject() {
        let agent_id = Uuid::now_v7();
        assert_eq!(parse_subject_agent_id(&format!("events.ingest.{agent_id}")).unwrap(), agent_id);
    }

    #[test]
    fn subject_parser_rejects_other_subjects() {
        let err = parse_subject_agent_id("events.broadcast.org").unwrap_err();
        assert!(matches!(err, ConsumeError::Permanent(_)));
    }

    #[test]
    fn normalize_adds_required_fields() {
        let session_id = Some("cli-123".to_string());
        let org_id = Uuid::nil();
        let data = normalize_event_data("pre_tool_use".to_string(), Value::Object(Map::new()), session_id, org_id, 123)
            .unwrap();
        assert_eq!(data["type"], "pre_tool_use");
        assert_eq!(data["sessionId"], "cli-123");
        assert_eq!(data["orgId"], org_id.to_string());
        assert_eq!(data["timestamp"], 123000);
        assert!(data["id"].is_string());
    }

    // --- Issue #30: derive_runtime_patch mapping between event type and
    // agents-row column writes. Pins the behavior per event type in one
    // place so future contributors see the full table at a glance.

    fn payload(event_type: &str, extras: serde_json::Value) -> Value {
        let mut base = serde_json::json!({ "type": event_type, "cwd": "/w/p" });
        if let Value::Object(map) = &extras {
            for (k, v) in map {
                base[k] = v.clone();
            }
        }
        base
    }

    #[test]
    fn derive_patch_pre_tool_use_sets_tool_status_and_cwd() {
        let data = payload("pre_tool_use", serde_json::json!({ "tool": "Edit" }));
        let patch = derive_runtime_patch("pre_tool_use", &data);
        assert_eq!(patch.status, Some(AgentStatus::Working));
        assert_eq!(patch.current_tool, Some(CurrentToolUpdate::Set("Edit".to_owned())));
        assert_eq!(patch.cwd.as_deref(), Some("/w/p"));
    }

    #[test]
    fn derive_patch_pre_tool_use_missing_tool_leaves_column_untouched() {
        // A malformed pre_tool_use with no tool name must not clobber the
        // existing tool to NULL — prefer preserving the last known value.
        let data = payload("pre_tool_use", serde_json::json!({}));
        let patch = derive_runtime_patch("pre_tool_use", &data);
        assert_eq!(patch.current_tool, None, "missing tool → no write, not clear");
        assert_eq!(patch.status, Some(AgentStatus::Working));
    }

    #[test]
    fn derive_patch_post_tool_use_clears_and_does_not_flip_status() {
        let data = payload("post_tool_use", serde_json::json!({ "tool": "Read" }));
        let patch = derive_runtime_patch("post_tool_use", &data);
        assert_eq!(patch.current_tool, Some(CurrentToolUpdate::Clear));
        assert_eq!(patch.status, None);
    }

    #[test]
    fn derive_patch_stop_clears_tool_and_goes_idle() {
        let data = payload("stop", serde_json::json!({}));
        let patch = derive_runtime_patch("stop", &data);
        assert_eq!(patch.status, Some(AgentStatus::Idle));
        assert_eq!(patch.current_tool, Some(CurrentToolUpdate::Clear));
    }

    #[test]
    fn derive_patch_session_end_matches_stop() {
        let data = payload("session_end", serde_json::json!({}));
        let patch = derive_runtime_patch("session_end", &data);
        assert_eq!(patch.status, Some(AgentStatus::Idle));
        assert_eq!(patch.current_tool, Some(CurrentToolUpdate::Clear));
    }

    #[test]
    fn derive_patch_user_prompt_submit_only_flips_status() {
        let data = payload("user_prompt_submit", serde_json::json!({}));
        let patch = derive_runtime_patch("user_prompt_submit", &data);
        assert_eq!(patch.status, Some(AgentStatus::Working));
        assert_eq!(patch.current_tool, None);
    }

    #[test]
    fn derive_patch_unknown_event_with_cwd_only_writes_cwd() {
        // Any event carrying a non-empty cwd refreshes the column, so the
        // UI "Working Dir" survives page reload even without an active tool.
        let data = payload("notification", serde_json::json!({}));
        let patch = derive_runtime_patch("notification", &data);
        assert_eq!(patch.status, None);
        assert_eq!(patch.current_tool, None);
        assert_eq!(patch.cwd.as_deref(), Some("/w/p"));
    }

    #[test]
    fn derive_patch_empty_cwd_is_treated_as_absent() {
        // Hook serializer writes `""` for missing optional strings. That
        // must not overwrite a real stored cwd. Verified alongside the
        // wire-level test `event_without_cwd_does_not_overwrite_stored_cwd`
        // in `event_consumer_contract.rs`.
        let data = serde_json::json!({ "type": "notification", "cwd": "" });
        let patch = derive_runtime_patch("notification", &data);
        assert!(patch.is_noop(), "empty cwd + unknown event = noop patch");
    }

    #[test]
    fn runtime_patch_is_noop_when_all_fields_are_none() {
        let patch = AgentRuntimePatch::default();
        assert!(patch.is_noop());
        let partial = AgentRuntimePatch { status: None, current_tool: Some(CurrentToolUpdate::Clear), cwd: None };
        assert!(!partial.is_noop());
    }

    #[test]
    fn derive_patch_drops_oversize_tool_and_cwd_to_prevent_row_bloat() {
        // A compromised or buggy sidecar within an org can forge any payload
        // field. The `agents` row is read by every admin listing, ManagedAgent
        // hydration, and WebSocket frame, so an unbounded string here
        // degrades the whole tenant. Drop (do not truncate) oversize values
        // — leaving the column at its previous value is less misleading
        // than writing a sliced prefix.
        let huge = "A".repeat(MAX_RUNTIME_STRING_LEN + 1);
        let data = serde_json::json!({
            "type": "pre_tool_use",
            "tool": huge,
            "cwd": huge,
        });
        let patch = derive_runtime_patch("pre_tool_use", &data);

        assert_eq!(patch.current_tool, None, "oversize tool must drop, not truncate, and not clear");
        assert_eq!(patch.cwd, None, "oversize cwd must drop, not truncate");
        // Status is derived purely from event_type, so it still transitions.
        assert_eq!(patch.status, Some(AgentStatus::Working));
    }

    #[test]
    fn derive_patch_accepts_values_at_the_size_limit() {
        let exact = "a".repeat(MAX_RUNTIME_STRING_LEN);
        let data = serde_json::json!({
            "type": "pre_tool_use",
            "tool": exact,
            "cwd": exact,
        });
        let patch = derive_runtime_patch("pre_tool_use", &data);

        assert!(matches!(patch.current_tool, Some(CurrentToolUpdate::Set(_))));
        assert!(patch.cwd.is_some());
    }
}
