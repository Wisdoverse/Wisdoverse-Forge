//! NATS-backed event consumer for sidecar event ingestion.
//!
//! Consumes signed envelopes from `events.ingest.<agent_id>`, verifies the
//! per-agent HMAC signature and timestamp window, normalizes them into the
//! Rust event schema, updates agent status for lifecycle transitions, and
//! republishes a legacy-compatible broadcast envelope on `broadcast.<org_id>`.
//!
//! ## Replay defense (issue #458)
//!
//! Each event envelope is signed by the sidecar's [`EventPublisher`] over the
//! canonical form `agent_id:timestamp:payload` using the per-agent HMAC secret
//! stored on `agents.hmac_secret` at spawn time — the exact same scheme and
//! secret used by [`agentforge_core::orchestration_protocol::SignedEnvelope`].
//! The consumer enforces two of the three replay-defense layers used by the
//! orchestration-result and credential consumers:
//!
//! 1. **HMAC verify** — recomputes the signature against the stored secret and
//!    constant-time-compares. A forged or tampered envelope is dropped.
//! 2. **Timestamp window** — rejects envelopes whose `timestamp` drifts beyond
//!    ±[`TIMESTAMP_REPLAY_WINDOW_SECS`] of the consumer clock, bounding the
//!    window in which a captured envelope can be replayed.
//!
//! There is no unique message-id dedup row, but lifecycle mirrors are not
//! last-write-wins: every persisted event carries the non-secret HMAC-generation
//! fingerprint, and runtime patches revalidate that generation under the Agent
//! lifecycle advisory lock. The events ledger then applies only the newest
//! lifecycle timestamp within that generation (Idle wins ties). Consequently a
//! transient broadcast failure cannot replay an older Working after Stop, and
//! an event verified just before container-key rotation cannot mutate the new
//! generation or poison its ordering ledger.

use std::sync::Arc;
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
use agentforge_core::event_protocol::parse_events_ingest_subject;
use agentforge_core::orchestration_protocol::{SignedEnvelope, container_generation_fingerprint};
use agentforge_core::ws_protocol::{ServerMessage, TurnInvalidatePayload};

use crate::dead_events::{DeadEvent, DeadEventRecorder, SqlxDeadEventRecorder, payload_excerpt};

pub const EVENTS_STREAM: &str = "EVENTS";
// Reuse the legacy durable on the shared workqueue stream instead of trying
// to create a second filtered consumer, which JetStream rejects.
const EVENTS_DURABLE: &str = "event-processor";
pub const EVENTS_FILTER: &str = "events.>";
const FETCH_BATCH_SIZE: usize = 32;
const FETCH_TIMEOUT_MS: u64 = 500;
const ACK_WAIT_SECS: u64 = 30;
const MAX_DELIVER: i64 = 5;

/// The shared signed-envelope replay window, re-exported at this module path so
/// the out-of-crate `tests/event_consumer_contract.rs` can keep importing
/// `event_consumer::TIMESTAMP_REPLAY_WINDOW_SECS`. The single source of truth
/// (and the deterministic boundary test) lives in [`crate::replay_window`].
pub use crate::replay_window::TIMESTAMP_REPLAY_WINDOW_SECS;
use crate::replay_window::within_replay_window;

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

impl SignedEventEnvelope {
    /// Verify the envelope's HMAC signature against the per-agent secret.
    ///
    /// The sidecar's `EventPublisher` signs over the canonical form
    /// `agent_id:timestamp:payload` where `payload` is the JSON value
    /// `{"event_type":…,"data":…}`. That is byte-for-byte the same canonical
    /// form as [`SignedEnvelope`], so we reconstruct a `SignedEnvelope` from
    /// our fields and delegate to its constant-time `verify` rather than
    /// re-implementing the HMAC dance (and pulling `hmac`/`sha2`/`hex` into
    /// this crate). `self.payload` re-serializes to the identical
    /// `{"event_type":…,"data":…}` object the sidecar signed.
    fn verify(&self, hmac_key: &[u8]) -> bool {
        let Ok(payload_value) = serde_json::to_value(&self.payload) else {
            return false;
        };
        let envelope = SignedEnvelope {
            payload: payload_value,
            timestamp: self.timestamp,
            agent_id: self.agent_id.clone(),
            signature: self.signature.clone(),
        };
        envelope.verify(hmac_key)
    }
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

// The browser broadcast frames (`event` / `turn_invalidate`) are built as the
// shared `agentforge_core::ws_protocol::ServerMessage` enum (MS-3 PR-D). The
// local `BroadcastMessage`/`TurnInvalidateMessage`/`BroadcastEnvelope` trio it
// replaced encoded the same wire bytes but duplicated the contract; the enum's
// round-trip test against the golden fixtures now pins it in one place.

#[derive(Debug, thiserror::Error)]
pub enum ConsumeError {
    /// A permanent rejection (Term drop). `reason` is the structured drop reason
    /// — `signature_mismatch`, `agent_unknown`, etc. for auth drops, or the
    /// generic `"permanent"` for the non-auth permanent rejections (malformed
    /// payload data, agent disappeared mid-flight). It is carried separately from
    /// the `source` error string so the dead-letter writer can query/group by it;
    /// without it the structured reason is otherwise swallowed into the anyhow
    /// string before the consumer's Term site.
    #[error("permanent event rejection ({reason}): {source}")]
    Permanent {
        reason: &'static str,
        #[source]
        source: anyhow::Error,
    },
    #[error("transient event processing failure: {0}")]
    Transient(#[source] anyhow::Error),
}

impl ConsumeError {
    /// A permanent rejection whose reason is not separately classified. Kept for
    /// any genuinely-unclassifiable site; the known non-auth permanent drops use
    /// [`Self::permanent_with_reason`] so their `dead_events.reason` is queryable,
    /// matching the orchestration path. Auth drops go through [`reject_unauthorized`].
    #[allow(dead_code)]
    fn permanent(message: impl Into<anyhow::Error>) -> Self {
        Self::Permanent { reason: "permanent", source: message.into() }
    }

    /// A permanent rejection carrying a specific, queryable structured `reason`
    /// (e.g. `bad_subject`, `payload_not_object`). Mirrors how
    /// [`reject_unauthorized`] tags the auth drops, so every dead-letter row has
    /// a reason an operator can filter on.
    fn permanent_with_reason(reason: &'static str, message: impl Into<anyhow::Error>) -> Self {
        Self::Permanent { reason, source: message.into() }
    }

    fn transient(message: impl Into<anyhow::Error>) -> Self {
        Self::Transient(message.into())
    }
}

#[async_trait]
pub trait EventStore: Clone + Send + Sync + 'static {
    async fn persist(&self, event: PersistedEvent) -> Result<()>;
}

/// Fetches the per-agent HMAC secret persisted at spawn time. Mirrors the
/// trait of the same name in `orchestration_result_consumer` and
/// `credential_consumer` — each consumer keeps its own copy so a future
/// per-path change to lookup semantics doesn't ripple across all three.
/// `Ok(None)` for an unknown agent (no row, NULL secret, or pre-migration
/// agent) is the expected forged-subject path and is treated as a
/// verification failure by the caller.
#[async_trait]
pub trait HmacSecretLookup: Clone + Send + Sync + 'static {
    async fn find_secret(&self, agent_id: Uuid) -> Result<Option<String>>;
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
    /// Signed hook event time used to make lifecycle transitions monotonic.
    /// The production SQL directory compares this against the durable events
    /// ledger, so a redelivered older `working` event cannot overwrite a newer
    /// `stop` after a transient broadcast failure.
    pub lifecycle_event_timestamp_ms: Option<i64>,
    /// Exact external Container CLI hook session that owns interactive work.
    /// The existing `agents.cli_session_id` column is the durable owner epoch;
    /// heartbeats and Stop events may mutate its lease only on an exact match.
    pub interactive_owner_session: Option<String>,
    /// Non-secret SHA-256 identity of the HMAC generation that authenticated
    /// this event. Lifecycle SQL rechecks it under the Agent advisory lock and
    /// filters the event-order ledger by it, closing verify -> container-roll
    /// TOCTOU races.
    pub expected_generation_fingerprint: Option<String>,
}

/// Two distinct update intents for `agents.current_tool`: write a new value
/// versus null the column out. Kept separate from `Option<String>` so the
/// "clear" case is not confused with "leave untouched".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentToolUpdate {
    Set(String),
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPatchOutcome {
    Applied,
    Superseded,
    StaleGeneration,
    AgentMissing,
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
    async fn apply_runtime_patch(&self, agent_id: Uuid, patch: AgentRuntimePatch) -> Result<AgentPatchOutcome>;
}

#[async_trait]
pub trait BroadcastBus: Clone + Send + Sync + 'static {
    async fn publish(&self, subject: String, message: ServerMessage) -> Result<()>;
}

#[derive(Debug, Clone)]
struct DecodedEvent {
    persisted: PersistedEvent,
    broadcast: ServerMessage,
    turn_invalidate: Option<ServerMessage>,
    broadcast_subject: String,
    runtime_patch: AgentRuntimePatch,
    persistable: bool,
}

/// Bump `event_ingest_unauthorized_total{reason}` and return a `Permanent`
/// rejection carrying the same reason — one helper so the metric label and the
/// log message can't drift apart. Permanent because redelivery of an
/// unauthenticated or stale envelope can never succeed.
fn reject_unauthorized(reason: &'static str, detail: impl std::fmt::Display) -> ConsumeError {
    metrics::counter!("event_ingest_unauthorized_total", "reason" => reason).increment(1);
    // Carry the structured `reason` on the variant (not just folded into the
    // error string) so the dead-letter row for an events.ingest drop is queryable
    // by reason, exactly like the orchestration.result path.
    ConsumeError::Permanent { reason, source: anyhow!("{reason}: {detail}") }
}

#[derive(Clone)]
pub struct EventConsumer<S, A, B, H> {
    store: S,
    agents: A,
    broadcast: B,
    hmac: H,
}

impl<S, A, B, H> EventConsumer<S, A, B, H>
where
    S: EventStore,
    A: AgentDirectory,
    B: BroadcastBus,
    H: HmacSecretLookup,
{
    pub fn new(store: S, agents: A, broadcast: B, hmac: H) -> Self {
        Self { store, agents, broadcast, hmac }
    }

    pub async fn handle(&self, subject: &str, envelope: SignedEventEnvelope) -> std::result::Result<(), ConsumeError> {
        let agent_id = parse_subject_agent_id(subject)?;
        if envelope.agent_id != agent_id.to_string() {
            return Err(reject_unauthorized(
                "envelope_agent_mismatch",
                format!("subject {agent_id} vs envelope {}", envelope.agent_id),
            ));
        }

        // Replay guard: reject timestamps that drift more than ±5 min from the
        // consumer's clock before doing any work. Same constant window as the
        // orchestration-result and credential consumers.
        let now_secs = chrono::Utc::now().timestamp();
        if !within_replay_window(now_secs, envelope.timestamp) {
            return Err(reject_unauthorized(
                "timestamp_outside_window",
                format!("envelope ts {} vs now {now_secs}", envelope.timestamp),
            ));
        }

        // HMAC verify is the auth step. A missing secret (unknown agent, NULL
        // column, pre-migration row) is treated the same as a bad signature —
        // refuse rather than fall through to side effects.
        let secret = match self.hmac.find_secret(agent_id).await {
            Ok(Some(secret)) => secret,
            Ok(None) => {
                return Err(reject_unauthorized("agent_unknown", format!("no hmac_secret for agent {agent_id}")));
            }
            Err(err) => {
                metrics::counter!("event_ingest_transient_errors_total", "stage" => "hmac_lookup").increment(1);
                return Err(ConsumeError::transient(err));
            }
        };
        if !envelope.verify(secret.as_bytes()) {
            return Err(reject_unauthorized("signature_mismatch", format!("agent {agent_id}")));
        }
        let generation_fingerprint = container_generation_fingerprint(secret.as_bytes());

        let target = self.agents.resolve(agent_id).await.map_err(ConsumeError::transient)?.ok_or_else(|| {
            ConsumeError::permanent_with_reason("agent_not_found", anyhow!("agent {agent_id} not found"))
        })?;

        let decoded = decode_event(target, envelope, generation_fingerprint)?;

        if decoded.persistable {
            self.store.persist(decoded.persisted.clone()).await.map_err(ConsumeError::transient)?;
        }

        // Persist lifecycle events before updating the Agent mirror. The
        // events table is the monotonic ordering ledger: on redelivery, SQL can
        // see a later stop/session_end and refuse to replay an older working
        // transition. Persistence is intentionally before the broadcast too,
        // because broadcast failures are the normal redelivery schedule this
        // ordering must survive.
        if !decoded.runtime_patch.is_noop() {
            let outcome = self
                .agents
                .apply_runtime_patch(decoded.persisted.agent_id, decoded.runtime_patch.clone())
                .await
                .map_err(ConsumeError::transient)?;
            match outcome {
                AgentPatchOutcome::Applied => {}
                AgentPatchOutcome::Superseded => return Ok(()),
                AgentPatchOutcome::StaleGeneration => {
                    return Err(ConsumeError::permanent_with_reason(
                        "stale_container_generation",
                        anyhow!("agent {} generation changed before runtime patch", decoded.persisted.agent_id),
                    ));
                }
                AgentPatchOutcome::AgentMissing => {
                    return Err(ConsumeError::permanent_with_reason(
                        "agent_disappeared",
                        anyhow!("agent {} disappeared during runtime patch", decoded.persisted.agent_id),
                    ));
                }
            }
        }

        self.broadcast
            .publish(decoded.broadcast_subject.clone(), decoded.broadcast)
            .await
            .map_err(ConsumeError::transient)?;

        if let Some(turn_invalidate) = decoded.turn_invalidate {
            self.broadcast
                .publish(decoded.broadcast_subject, turn_invalidate)
                .await
                .map_err(ConsumeError::transient)?;
        }

        Ok(())
    }
}

/// Resolve the publishing agent's UUID from a received ingest subject.
///
/// Accepts both the #457 kind-namespaced shape (`events.ingest.<kind>.<uuid>`)
/// and the legacy shape (`events.ingest.<uuid>`) via the shared core parser,
/// so the cross-check against the envelope's `agent_id` is stable across the
/// migration. Anything else is a permanent reject (forged/unsupported subject).
fn parse_subject_agent_id(subject: &str) -> std::result::Result<Uuid, ConsumeError> {
    parse_events_ingest_subject(subject).map(|parsed| parsed.agent_id).ok_or_else(|| {
        ConsumeError::permanent_with_reason("bad_subject", anyhow!("unsupported event subject {subject}"))
    })
}

fn decode_event(
    target: AgentTarget,
    envelope: SignedEventEnvelope,
    generation_fingerprint: String,
) -> std::result::Result<DecodedEvent, ConsumeError> {
    let reported_session_id = extract_session_id(&envelope.payload.data);
    if derive_status(&envelope.payload.event_type).is_some() && reported_session_id.is_none() {
        return Err(ConsumeError::permanent_with_reason(
            "lifecycle_session_missing",
            anyhow!("lifecycle hook {} omitted sessionId", envelope.payload.event_type),
        ));
    }
    let session_id =
        reported_session_id.clone().or(target.cli_session_id.clone()).or_else(|| Some(target.agent_id.to_string()));

    let event_type = envelope.payload.event_type;
    let mut event_data = normalize_event_data(
        event_type.clone(),
        envelope.payload.data,
        session_id.clone(),
        target.organization_id,
        envelope.timestamp,
    )?;
    let event_timestamp_ms = event_data.get("timestamp").and_then(json_i64).unwrap_or(envelope.timestamp * 1000);
    if let Value::Object(object) = &mut event_data {
        object.insert("containerGenerationFingerprint".to_string(), Value::String(generation_fingerprint.clone()));
        // The ordering ledger must contain exactly the timestamp used by the
        // lifecycle CAS. Legacy hooks may send a string timestamp (or none at
        // all); preserving it would make the SQL fall back to `created_at`, so
        // the just-persisted event could incorrectly supersede itself.
        if derive_status(&event_type).is_some() {
            object.insert("timestamp".to_string(), Value::from(event_timestamp_ms));
        }
    }

    let broadcast_agent_id = session_id.clone().unwrap_or_else(|| target.agent_id.to_string());
    let organization_id = target.organization_id;
    let persistable = is_persistable(&event_type);

    let runtime_patch = derive_runtime_patch(
        &event_type,
        &event_data,
        event_timestamp_ms,
        Some(generation_fingerprint),
        reported_session_id,
    );

    Ok(DecodedEvent {
        broadcast_subject: format!("broadcast.{organization_id}"),
        broadcast: ServerMessage::Event {
            event_type: event_type.clone(),
            event_data: event_data.clone(),
            agent_id: broadcast_agent_id,
            org_id: organization_id.to_string(),
        },
        turn_invalidate: persistable.then(|| ServerMessage::TurnInvalidate {
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
            return Err(ConsumeError::permanent_with_reason(
                "payload_not_object",
                anyhow!("event payload data must be a JSON object"),
            ));
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
fn derive_runtime_patch(
    event_type: &str,
    data: &Value,
    canonical_event_timestamp_ms: i64,
    expected_generation_fingerprint: Option<String>,
    interactive_owner_session: Option<String>,
) -> AgentRuntimePatch {
    let status = derive_status(event_type);
    let lifecycle_event_timestamp_ms = status.map(|_| canonical_event_timestamp_ms);

    let current_tool = match event_type {
        "pre_tool_use" => payload_string(data, "tool").map(|t| CurrentToolUpdate::Set(t.to_owned())),
        "post_tool_use" | "stop" | "session_end" => Some(CurrentToolUpdate::Clear),
        _ => None,
    };

    let cwd = payload_string(data, "cwd").map(str::to_owned);

    AgentRuntimePatch {
        status,
        current_tool,
        cwd,
        lifecycle_event_timestamp_ms,
        interactive_owner_session: status.and(interactive_owner_session),
        expected_generation_fingerprint,
    }
}

/// Describe + prime the event-ingest replay-defense metrics so a Prometheus
/// scrape returns them before the first rejection fires.
pub fn register_metrics() {
    metrics::describe_counter!(
        "event_ingest_unauthorized_total",
        "Event ingest envelopes dropped by the backend consumer; label = reason"
    );
    metrics::describe_counter!(
        "event_ingest_transient_errors_total",
        "Consumer-side transient failures (DB, hmac lookup) that will be redelivered; label = stage"
    );
    metrics::counter!("event_ingest_unauthorized_total", "reason" => "none").increment(0);
    metrics::counter!("event_ingest_transient_errors_total", "stage" => "none").increment(0);
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

/// Production `HmacSecretLookup` backed by `agents.hmac_secret` (migration
/// 025) — the same secret handed to the sidecar's `EventPublisher` at spawn.
/// The column is nullable; "missing row" and "row with NULL secret" both map
/// to `Ok(None)` so the caller emits a uniform `reason=agent_unknown`.
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

    async fn apply_runtime_patch(&self, agent_id: Uuid, patch: AgentRuntimePatch) -> Result<AgentPatchOutcome> {
        // Build one dynamic UPDATE per event so we never issue a no-op write.
        // The consumer has already checked `patch.is_noop()` before this call;
        // guard against accidental regressions anyway.
        if patch.is_noop() {
            return Ok(AgentPatchOutcome::Applied);
        }

        let expected_generation_fingerprint = patch.expected_generation_fingerprint.clone();
        let mut tx = self.pool.begin().await?;
        agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, agent_id).await?;
        let current_secret: Option<Option<String>> = sqlx::query_scalar("SELECT hmac_secret FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(current_secret) = current_secret else {
            tx.commit().await?;
            return Ok(AgentPatchOutcome::AgentMissing);
        };
        if let Some(expected_fingerprint) = expected_generation_fingerprint.as_deref()
            && current_secret.as_deref().map(|secret| container_generation_fingerprint(secret.as_bytes())).as_deref()
                != Some(expected_fingerprint)
        {
            tx.commit().await?;
            return Ok(AgentPatchOutcome::StaleGeneration);
        }

        let (has_interactive_epoch, current_interactive_owner, has_orchestration_owner): (bool, Option<String>, bool) =
            sqlx::query_as(
                r#"SELECT interactive_lease_expires_at > NOW(),
                      interactive_owner_session_id,
                      EXISTS (
                          SELECT 1 FROM participants participant
                           WHERE participant.organization_id = agents.organization_id
                             AND participant.agent_id = agents.id
                             AND participant.status = 'busy'
                      ) OR EXISTS (
                          SELECT 1 FROM orchestration_tasks task
                           WHERE task.organization_id = agents.organization_id
                             AND task.assigned_agent_id = agents.id
                             AND task.status = 'working'
                      )
                 FROM agents
                WHERE id = $1"#,
            )
            .bind(agent_id)
            .fetch_one(&mut *tx)
            .await?;
        let owner_session_matches = patch.interactive_owner_session.as_deref() == current_interactive_owner.as_deref();
        let owner_transition_conflicts = match patch.status {
            Some(AgentStatus::Working) => {
                !has_orchestration_owner
                    && (patch.interactive_owner_session.is_none()
                        || (has_interactive_epoch && current_interactive_owner.is_some() && !owner_session_matches))
            }
            Some(AgentStatus::Idle) => {
                has_orchestration_owner
                    || !has_interactive_epoch
                    || patch.interactive_owner_session.is_none()
                    || !owner_session_matches
            }
            _ => false,
        };
        if owner_transition_conflicts {
            tx.commit().await?;
            return Ok(AgentPatchOutcome::Superseded);
        }
        let may_write_interactive_owner = !has_orchestration_owner;
        let lifecycle_order = patch.status.zip(patch.lifecycle_event_timestamp_ms).map(|(status, timestamp)| {
            let rank: i16 = if status == AgentStatus::Idle { 1 } else { 0 };
            (timestamp, rank)
        });
        let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE agents SET updated_at = NOW()");

        if let Some(status) = patch.status {
            builder.push(", status = ");
            builder.push_bind(status);
            match status {
                AgentStatus::Working => {
                    // Authenticated CLI hooks establish a durable session epoch.
                    // Current-generation sidecar heartbeats renew this bounded
                    // crash backstop only for the exact epoch; a stale monitor
                    // or heartbeat can never revive or clear a newer owner.
                    if may_write_interactive_owner {
                        builder.push(", interactive_owner_session_id = ");
                        builder.push_bind(patch.interactive_owner_session.clone().expect("validated hook owner"));
                        builder.push(", interactive_lease_expires_at = NOW() + INTERVAL '2 minutes'");
                    }
                }
                AgentStatus::Idle => {
                    if may_write_interactive_owner {
                        builder.push(", interactive_lease_expires_at = NULL");
                    }
                }
                AgentStatus::Offline => {}
            }
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
        if let Some((timestamp_ms, rank)) = lifecycle_order {
            // The current event was persisted before this UPDATE. Reject this
            // mirror write when the durable ledger already contains a newer
            // lifecycle transition. Idle wins an exact timestamp tie, making
            // Working -> Stop -> redelivered Working monotonic and idempotent.
            builder.push(
                r#" AND NOT EXISTS (
                        SELECT 1
                          FROM events newer
                         WHERE newer.agent_id = agents.id
                           AND newer.event_type IN ('pre_tool_use', 'user_prompt_submit', 'stop', 'session_end')"#,
            );
            if let Some(expected_fingerprint) = expected_generation_fingerprint.as_deref() {
                builder.push(" AND newer.payload->>'containerGenerationFingerprint' = ");
                builder.push_bind(expected_fingerprint.to_string());
            }
            builder.push(
                r#" AND (
                               CASE
                                   WHEN jsonb_typeof(newer.payload->'timestamp') = 'number'
                                   THEN (newer.payload->>'timestamp')::numeric::bigint
                                   ELSE (EXTRACT(EPOCH FROM newer.created_at) * 1000)::bigint
                               END > "#,
            );
            builder.push_bind(timestamp_ms);
            builder.push(
                r#" OR (
                                   CASE
                                       WHEN jsonb_typeof(newer.payload->'timestamp') = 'number'
                                       THEN (newer.payload->>'timestamp')::numeric::bigint
                                       ELSE (EXTRACT(EPOCH FROM newer.created_at) * 1000)::bigint
                                   END = "#,
            );
            builder.push_bind(timestamp_ms);
            builder.push(r#" AND CASE WHEN newer.event_type IN ('stop', 'session_end') THEN 1 ELSE 0 END > "#);
            builder.push_bind(rank);
            builder.push(")))");
        }

        let result = builder.build().execute(&mut *tx).await?;
        let outcome =
            if result.rows_affected() == 0 { AgentPatchOutcome::Superseded } else { AgentPatchOutcome::Applied };
        tx.commit().await?;
        Ok(outcome)
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
    async fn publish(&self, subject: String, message: ServerMessage) -> Result<()> {
        let bytes = serde_json::to_vec(&message)?;
        self.client.publish(subject, bytes.into()).await?;
        Ok(())
    }
}

pub struct EventStreamWorker {
    consumer: PullConsumer,
    logic: EventConsumer<SqlxEventStore, SqlxAgentDirectory, NatsBroadcastBus, SqlxHmacSecretLookup>,
    /// Optional dead-letter recorder. `connect` installs a real
    /// `SqlxDeadEventRecorder`; the field stays `Option` so a future builder or
    /// test can construct the worker without one without changing the shape.
    dead_events: Option<Arc<dyn DeadEventRecorder>>,
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

        metrics::describe_counter!(
            "agentforge_nats_legacy_subject_received_total",
            "Count of events received on the pre-#457 un-namespaced subject. \
             Drains to zero once all agent containers publish kind-namespaced; \
             the legacy-drop deploy is gated on this reaching and holding zero."
        );
        // Materialise the {subject="events.ingest"} series at 0 so dashboards
        // and the legacy-drop gate observe an explicit zero rather than "no
        // data". Absent-series would otherwise be indistinguishable from a true
        // zero, and a `== 0` gate would pass on a dead/never-run consumer.
        metrics::counter!("agentforge_nats_legacy_subject_received_total", "subject" => "events.ingest").increment(0);

        // Dead-letter recorder: persist permanently-dropped envelopes so an
        // operator has a durable record of why agent X's events were rejected.
        let dead_events: Arc<dyn DeadEventRecorder> = Arc::new(SqlxDeadEventRecorder::new(pool.clone()));

        Ok(Self {
            consumer,
            logic: EventConsumer::new(
                SqlxEventStore::new(pool.clone()),
                SqlxAgentDirectory::new(pool.clone()),
                NatsBroadcastBus::new(client),
                SqlxHmacSecretLookup::new(pool),
            ),
            dead_events: Some(dead_events),
        })
    }

    /// Override the dead-letter recorder (e.g. a test capture). Production
    /// `connect` already installs a `SqlxDeadEventRecorder`; this lets a caller
    /// swap it without changing the `connect` signature.
    pub fn with_dead_event_recorder(mut self, recorder: Arc<dyn DeadEventRecorder>) -> Self {
        self.dead_events = Some(recorder);
        self
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

        // #457 drain signal: count receipts on the pre-namespacing subject so
        // operators can confirm the legacy tail has reached zero before the
        // legacy-drop deploy. Namespaced receipts are the steady state and are
        // intentionally not counted here.
        if parse_events_ingest_subject(&subject).is_some_and(|parsed| parsed.is_legacy()) {
            metrics::counter!("agentforge_nats_legacy_subject_received_total", "subject" => "events.ingest")
                .increment(1);
        }

        let envelope = match serde_json::from_slice::<SignedEventEnvelope>(&message.payload) {
            Ok(envelope) => envelope,
            Err(err) => {
                tracing::warn!(error = %err, %subject, "dropping malformed event envelope");
                // A body that won't even decode into an envelope is a PERMANENT
                // drop the feature promises to capture. Count it on the same
                // unauthorized series the other event drops use, then dead-letter
                // it before the Term ack. Best-effort: `record` logs its own
                // failures and never blocks the ack.
                metrics::counter!("event_ingest_unauthorized_total", "reason" => "envelope_decode_failed").increment(1);
                if let Some(recorder) = &self.dead_events {
                    recorder
                        .record(DeadEvent {
                            source: "events.ingest",
                            reason: "envelope_decode_failed".to_string(),
                            subject: subject.clone(),
                            detail: Some(err.to_string()),
                            delivery_id: None,
                            org_id: None,
                            payload_excerpt: payload_excerpt(&message.payload),
                        })
                        .await;
                }
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
            Err(ConsumeError::Permanent { reason, source }) => {
                tracing::warn!(error = %source, %reason, %subject, "dropping permanently invalid event");
                // Dead-letter the drop before the Term ack. Best-effort: `record`
                // logs its own failures and never blocks the ack. org_id/delivery_id
                // are NULL — events.ingest carries no delivery id and the drop is
                // pre-/at-auth, so no trustworthy org is available.
                if let Some(recorder) = &self.dead_events {
                    recorder
                        .record(DeadEvent {
                            source: "events.ingest",
                            reason: reason.to_string(),
                            subject: subject.clone(),
                            detail: Some(source.to_string()),
                            delivery_id: None,
                            org_id: None,
                            payload_excerpt: payload_excerpt(&message.payload),
                        })
                        .await;
                }
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
    use chrono::Utc;
    use serde_json::Map;

    #[test]
    fn subject_parser_accepts_legacy_ingest_subject() {
        let agent_id = Uuid::now_v7();
        assert_eq!(parse_subject_agent_id(&format!("events.ingest.{agent_id}")).unwrap(), agent_id);
    }

    #[test]
    fn subject_parser_accepts_kind_namespaced_ingest_subject() {
        // #457: the consumer must resolve the same agent_id from the namespaced
        // shape so the `subject vs envelope` cross-check still holds.
        let agent_id = Uuid::now_v7();
        for kind in ["container", "cli", "api"] {
            assert_eq!(parse_subject_agent_id(&format!("events.ingest.{kind}.{agent_id}")).unwrap(), agent_id);
        }
    }

    #[test]
    fn subject_parser_rejects_other_subjects() {
        for bad in ["events.broadcast.org", "events.ingest.bogus.not-a-uuid", "events.ingest.>"] {
            let err = parse_subject_agent_id(bad).unwrap_err();
            // The drop must carry the queryable `bad_subject` reason (not the
            // generic `permanent`) so the dead_events row is filterable, matching
            // the orchestration path.
            assert!(
                matches!(err, ConsumeError::Permanent { reason: "bad_subject", .. }),
                "expected bad_subject permanent reject for {bad}, got {err:?}"
            );
        }
    }

    #[test]
    fn non_object_payload_data_drops_with_payload_not_object_reason() {
        // A non-object `data` is a permanent drop tagged `payload_not_object` so
        // the dead_events row is queryable by reason.
        let err = normalize_event_data(
            "pre_tool_use".to_string(),
            serde_json::json!(["not", "an", "object"]),
            None,
            Uuid::now_v7(),
            chrono::Utc::now().timestamp(),
        )
        .unwrap_err();
        assert!(
            matches!(err, ConsumeError::Permanent { reason: "payload_not_object", .. }),
            "expected payload_not_object reason, got {err:?}"
        );
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

    #[test]
    fn lifecycle_order_uses_envelope_time_when_payload_timestamp_is_missing_or_string() {
        let target = AgentTarget { agent_id: Uuid::new_v4(), organization_id: Uuid::new_v4(), cli_session_id: None };
        for data in [
            serde_json::json!({"sessionId": "session-123"}),
            serde_json::json!({"sessionId": "session-123", "timestamp": "legacy"}),
        ] {
            let envelope = SignedEventEnvelope {
                payload: SignedEventPayload { event_type: "user_prompt_submit".to_string(), data },
                timestamp: 123,
                agent_id: target.agent_id.to_string(),
                signature: String::new(),
            };
            let decoded =
                decode_event(target.clone(), envelope, "test-generation".to_string()).expect("decode lifecycle event");
            assert_eq!(decoded.runtime_patch.lifecycle_event_timestamp_ms, Some(123_000));
            assert_eq!(decoded.persisted.payload["timestamp"], 123_000);
        }
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

    fn derive_test_patch(event_type: &str, data: &Value) -> AgentRuntimePatch {
        derive_runtime_patch(event_type, data, 1_700_000_000_000, None, extract_session_id(data))
    }

    #[test]
    fn derive_patch_pre_tool_use_sets_tool_status_and_cwd() {
        let data = payload("pre_tool_use", serde_json::json!({ "tool": "Edit" }));
        let patch = derive_test_patch("pre_tool_use", &data);
        assert_eq!(patch.status, Some(AgentStatus::Working));
        assert_eq!(patch.current_tool, Some(CurrentToolUpdate::Set("Edit".to_owned())));
        assert_eq!(patch.cwd.as_deref(), Some("/w/p"));
    }

    #[test]
    fn derive_patch_pre_tool_use_missing_tool_leaves_column_untouched() {
        // A malformed pre_tool_use with no tool name must not clobber the
        // existing tool to NULL — prefer preserving the last known value.
        let data = payload("pre_tool_use", serde_json::json!({}));
        let patch = derive_test_patch("pre_tool_use", &data);
        assert_eq!(patch.current_tool, None, "missing tool → no write, not clear");
        assert_eq!(patch.status, Some(AgentStatus::Working));
    }

    #[test]
    fn derive_patch_post_tool_use_clears_and_does_not_flip_status() {
        let data = payload("post_tool_use", serde_json::json!({ "tool": "Read" }));
        let patch = derive_test_patch("post_tool_use", &data);
        assert_eq!(patch.current_tool, Some(CurrentToolUpdate::Clear));
        assert_eq!(patch.status, None);
    }

    #[test]
    fn derive_patch_stop_clears_tool_and_goes_idle() {
        let data = payload("stop", serde_json::json!({}));
        let patch = derive_test_patch("stop", &data);
        assert_eq!(patch.status, Some(AgentStatus::Idle));
        assert_eq!(patch.current_tool, Some(CurrentToolUpdate::Clear));
    }

    #[test]
    fn derive_patch_session_end_matches_stop() {
        let data = payload("session_end", serde_json::json!({}));
        let patch = derive_test_patch("session_end", &data);
        assert_eq!(patch.status, Some(AgentStatus::Idle));
        assert_eq!(patch.current_tool, Some(CurrentToolUpdate::Clear));
    }

    #[test]
    fn derive_patch_user_prompt_submit_only_flips_status() {
        let data = payload("user_prompt_submit", serde_json::json!({}));
        let patch = derive_test_patch("user_prompt_submit", &data);
        assert_eq!(patch.status, Some(AgentStatus::Working));
        assert_eq!(patch.current_tool, None);
    }

    #[test]
    fn derive_patch_unknown_event_with_cwd_only_writes_cwd() {
        // Any event carrying a non-empty cwd refreshes the column, so the
        // UI "Working Dir" survives page reload even without an active tool.
        let data = payload("notification", serde_json::json!({}));
        let patch = derive_test_patch("notification", &data);
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
        let patch = derive_test_patch("notification", &data);
        assert!(patch.is_noop(), "empty cwd + unknown event = noop patch");
    }

    #[test]
    fn runtime_patch_is_noop_when_all_fields_are_none() {
        let patch = AgentRuntimePatch::default();
        assert!(patch.is_noop());
        let partial = AgentRuntimePatch {
            status: None,
            current_tool: Some(CurrentToolUpdate::Clear),
            cwd: None,
            lifecycle_event_timestamp_ms: None,
            interactive_owner_session: None,
            expected_generation_fingerprint: None,
        };
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
        let patch = derive_test_patch("pre_tool_use", &data);

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
        let patch = derive_test_patch("pre_tool_use", &data);

        assert!(matches!(patch.current_tool, Some(CurrentToolUpdate::Set(_))));
        assert!(patch.cwd.is_some());
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn newer_stop_survives_older_working_redelivery(pool: PgPool) {
        let organization_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let initial_secret = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Event order', $2)")
            .bind(organization_id)
            .bind(format!("event-order-{organization_id}"))
            .execute(&pool)
            .await
            .expect("seed organization");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(organization_id)
            .execute(&pool)
            .await
            .expect("seed workspace");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("event-order-{user_id}@example.com"))
            .execute(&pool)
            .await
            .expect("seed user");
        sqlx::query(
            "INSERT INTO agents \
                 (id, organization_id, workspace_id, user_id, status, hmac_secret, interactive_lease_expires_at) \
             VALUES ($1, $2, $2, $3, 'idle', $4, NOW() + INTERVAL '60 seconds')",
        )
        .bind(agent_id)
        .bind(organization_id)
        .bind(user_id)
        .bind(&initial_secret)
        .execute(&pool)
        .await
        .expect("seed Agent");

        let store = SqlxEventStore::new(pool.clone());
        let directory = SqlxAgentDirectory::new(pool.clone());
        let persist = |event_type: &str, timestamp: i64| PersistedEvent {
            organization_id,
            agent_id,
            event_type: event_type.to_string(),
            payload: serde_json::json!({"type": event_type, "timestamp": timestamp}),
            session_id: Some("ordered-session".to_string()),
        };
        let patch = |status, timestamp| AgentRuntimePatch {
            status: Some(status),
            current_tool: None,
            cwd: None,
            lifecycle_event_timestamp_ms: Some(timestamp),
            interactive_owner_session: Some("ordered-session".to_string()),
            expected_generation_fingerprint: None,
        };

        store.persist(persist("user_prompt_submit", 1_000)).await.expect("persist initial working");
        assert_eq!(
            directory.apply_runtime_patch(agent_id, patch(AgentStatus::Working, 1_000)).await.unwrap(),
            AgentPatchOutcome::Applied
        );
        let claimed_owner: Option<String> =
            sqlx::query_scalar("SELECT interactive_owner_session_id FROM agents WHERE id = $1")
                .bind(agent_id)
                .fetch_one(&pool)
                .await
                .expect("read hook owner epoch");
        assert_eq!(
            claimed_owner.as_deref(),
            Some("ordered-session"),
            "the first signed Working hook must transfer the terminal/MCP bridge lease to its session epoch"
        );
        sqlx::query("UPDATE agents SET interactive_lease_expires_at = NOW() + INTERVAL '1 second' WHERE id = $1")
            .bind(agent_id)
            .execute(&pool)
            .await
            .expect("advance long work past its original bridge TTL");
        assert!(
            crate::participant_liveness::renew_interactive_owner_from_heartbeat(
                &pool,
                agent_id,
                "ordered-session",
                &container_generation_fingerprint(initial_secret.as_bytes()),
            )
            .await
            .expect("renew exact hook owner"),
            "current sidecar heartbeat must keep work exclusive beyond the bridge TTL"
        );
        let outlives_input_bridge: bool = sqlx::query_scalar(
            "SELECT interactive_lease_expires_at > NOW() + INTERVAL '60 seconds' FROM agents WHERE id = $1",
        )
        .bind(agent_id)
        .fetch_one(&pool)
        .await
        .expect("read durable terminal owner");
        assert!(outlives_input_bridge, "terminal execution ownership must survive work longer than 60 seconds");
        store.persist(persist("stop", 2_000)).await.expect("persist newer stop");
        assert_eq!(
            directory.apply_runtime_patch(agent_id, patch(AgentStatus::Idle, 2_000)).await.unwrap(),
            AgentPatchOutcome::Applied
        );

        // Exact transient-broadcast schedule: the old Working envelope is
        // redelivered after Stop committed. It is persisted again, but the
        // ledger CAS must not restore either status=working or its owner lease.
        store.persist(persist("user_prompt_submit", 1_000)).await.expect("persist redelivered working");
        assert_eq!(
            directory.apply_runtime_patch(agent_id, patch(AgentStatus::Working, 1_000)).await.unwrap(),
            AgentPatchOutcome::Superseded
        );
        let (status, lease): (AgentStatus, Option<chrono::DateTime<Utc>>) =
            sqlx::query_as("SELECT status, interactive_lease_expires_at FROM agents WHERE id = $1")
                .bind(agent_id)
                .fetch_one(&pool)
                .await
                .expect("read final lifecycle mirror");
        assert_eq!(status, AgentStatus::Idle);
        assert!(lease.is_none(), "older Working redelivery must not recreate execution ownership");

        // Exact verify/persist -> container-roll schedule. The stale X event is
        // already durable when the HMAC rotates to Y. Revalidation under the
        // lifecycle lock rejects X, and the generation-filtered ledger lets a
        // lower-timestamp Y event apply instead of being poisoned by X.
        let secret_x = Uuid::new_v4().to_string();
        let secret_y = Uuid::new_v4().to_string();
        let generation_x = container_generation_fingerprint(secret_x.as_bytes());
        let generation_y = container_generation_fingerprint(secret_y.as_bytes());
        sqlx::query("UPDATE agents SET hmac_secret = $2 WHERE id = $1")
            .bind(agent_id)
            .bind(&secret_x)
            .execute(&pool)
            .await
            .expect("install generation X");
        let persisted_generation_event = |timestamp, generation: &str| PersistedEvent {
            organization_id,
            agent_id,
            event_type: "user_prompt_submit".to_string(),
            payload: serde_json::json!({
                "type": "user_prompt_submit",
                "timestamp": timestamp,
                "containerGenerationFingerprint": generation,
            }),
            session_id: Some("ordered-session".to_string()),
        };
        let generation_patch = |timestamp, generation: &str| AgentRuntimePatch {
            status: Some(AgentStatus::Working),
            current_tool: None,
            cwd: None,
            lifecycle_event_timestamp_ms: Some(timestamp),
            interactive_owner_session: Some("ordered-session".to_string()),
            expected_generation_fingerprint: Some(generation.to_string()),
        };
        store
            .persist(persisted_generation_event(5_000, &generation_x))
            .await
            .expect("persist verified generation X event");
        sqlx::query("UPDATE agents SET hmac_secret = $2 WHERE id = $1")
            .bind(agent_id)
            .bind(&secret_y)
            .execute(&pool)
            .await
            .expect("roll to generation Y");
        assert_eq!(
            directory.apply_runtime_patch(agent_id, generation_patch(5_000, &generation_x)).await.unwrap(),
            AgentPatchOutcome::StaleGeneration
        );
        store.persist(persisted_generation_event(4_000, &generation_y)).await.expect("persist generation Y event");
        assert_eq!(
            directory.apply_runtime_patch(agent_id, generation_patch(4_000, &generation_y)).await.unwrap(),
            AgentPatchOutcome::Applied,
            "stale generation X ledger row must not block generation Y"
        );

        // A permanently lost Stop has a bounded recovery: once the five-minute
        // owner expires, stale status alone cannot block admission forever.
        sqlx::query("UPDATE agents SET interactive_lease_expires_at = NOW() - INTERVAL '1 second' WHERE id = $1")
            .bind(agent_id)
            .execute(&pool)
            .await
            .expect("simulate lost-stop lease expiry");
        let mut admission = pool.begin().await.expect("begin recovery check");
        agentforge_db::lock_agent_lifecycle_in_tx(&mut admission, agent_id).await.unwrap();
        assert_eq!(
            agentforge_db::agent_work_admission_is_idle_in_tx(&mut admission, organization_id, agent_id).await.unwrap(),
            Some(true),
            "expired bounded owner must recover even when status still says working"
        );
        admission.rollback().await.unwrap();

        // Exact persist -> orchestration claim -> patch schedule. A task claim
        // that wins the lifecycle lock makes the participant busy. When the
        // already-persisted Working hook resumes, it may mirror status but must
        // not create a second interactive owner.
        sqlx::query(
            "INSERT INTO participants (organization_id, agent_id, name, capabilities, status) \
             VALUES ($1, $2, 'claimed', ARRAY['codex'], 'busy')",
        )
        .bind(organization_id)
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("seed committed orchestration owner");
        store
            .persist(persisted_generation_event(6_000, &generation_y))
            .await
            .expect("persist Working before delayed patch");
        assert_eq!(
            directory.apply_runtime_patch(agent_id, generation_patch(6_000, &generation_y)).await.unwrap(),
            AgentPatchOutcome::Applied
        );
        let lease: Option<chrono::DateTime<Utc>> =
            sqlx::query_scalar("SELECT interactive_lease_expires_at FROM agents WHERE id = $1")
                .bind(agent_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(lease.is_none(), "delayed Working hook must not overlap the committed orchestration owner");

        // Legacy/string timestamps are normalized in the durable ledger, not
        // only in the in-memory patch. This proves the event cannot see its own
        // later `created_at` fallback and incorrectly supersede itself.
        let legacy_agent_id = Uuid::new_v4();
        let legacy_secret = Uuid::new_v4().to_string();
        let legacy_generation = container_generation_fingerprint(legacy_secret.as_bytes());
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, user_id, status, hmac_secret) \
             VALUES ($1, $2, $2, $3, 'idle', $4)",
        )
        .bind(legacy_agent_id)
        .bind(organization_id)
        .bind(user_id)
        .bind(&legacy_secret)
        .execute(&pool)
        .await
        .expect("seed legacy timestamp Agent");
        let decoded = decode_event(
            AgentTarget { agent_id: legacy_agent_id, organization_id, cli_session_id: None },
            SignedEventEnvelope {
                payload: SignedEventPayload {
                    event_type: "user_prompt_submit".to_string(),
                    data: serde_json::json!({"sessionId": "legacy-session", "timestamp": "legacy"}),
                },
                timestamp: 123,
                agent_id: legacy_agent_id.to_string(),
                signature: String::new(),
            },
            legacy_generation,
        )
        .expect("decode legacy timestamp event");
        assert_eq!(decoded.persisted.payload["timestamp"], 123_000);
        store.persist(decoded.persisted).await.expect("persist normalized legacy event");
        assert_eq!(
            directory.apply_runtime_patch(legacy_agent_id, decoded.runtime_patch).await.unwrap(),
            AgentPatchOutcome::Applied,
            "normalized string timestamp event must not supersede itself"
        );
    }
}
