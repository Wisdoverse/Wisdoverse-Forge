use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

use agentforge_core::AgentStatus;
use agentforge_core::orchestration_protocol::SignedEnvelope;
use agentforge_core::ws_protocol::ServerMessage;
use agentforge_jobs::event_consumer::{
    AgentDirectory, AgentRuntimePatch, AgentTarget, BroadcastBus, ConsumeError, EventConsumer, EventIngestOutcome,
    EventStore, HmacSecretLookup, PersistedEvent, SignedEventEnvelope, SignedEventPayload,
    TIMESTAMP_REPLAY_WINDOW_SECS,
};

/// Shared per-agent secret for the in-memory verification harness. The signed
/// envelope helper signs with this key and `MemoryHmac` returns it, so every
/// happy-path test exercises the real HMAC verify rather than a bypass.
const TEST_HMAC: &str = "event-consumer-test-hmac";

/// In-memory `HmacSecretLookup`: every agent in the map verifies against its
/// stored secret; any agent missing from the map is `Ok(None)` (unknown —
/// treated as unauthorized by the consumer).
#[derive(Clone, Default)]
struct MemoryHmac {
    by_agent: Arc<HashMap<Uuid, String>>,
}

impl MemoryHmac {
    fn with(agent_id: Uuid, secret: &str) -> Self {
        Self { by_agent: Arc::new(HashMap::from([(agent_id, secret.to_string())])) }
    }
}

#[async_trait]
impl HmacSecretLookup for MemoryHmac {
    async fn find_secret(&self, agent_id: Uuid) -> Result<Option<String>> {
        Ok(self.by_agent.get(&agent_id).cloned())
    }
}

#[derive(Clone, Default)]
struct MemoryEventStore {
    events: Arc<Mutex<Vec<PersistedEvent>>>,
    runtime_patches: Arc<Mutex<Vec<(Uuid, AgentRuntimePatch)>>>,
}

impl MemoryEventStore {
    async fn snapshot(&self) -> Vec<PersistedEvent> {
        self.events.lock().await.clone()
    }

    async fn runtime_patches(&self) -> Vec<(Uuid, AgentRuntimePatch)> {
        self.runtime_patches.lock().await.clone()
    }

    async fn status_updates(&self) -> Vec<(Uuid, AgentStatus)> {
        self.runtime_patches.lock().await.iter().filter_map(|(id, patch)| patch.status.map(|s| (*id, s))).collect()
    }
}

#[async_trait]
impl EventStore for MemoryEventStore {
    async fn ingest(&self, event: PersistedEvent, patch: AgentRuntimePatch) -> Result<EventIngestOutcome> {
        let mut events = self.events.lock().await;
        let sequence = event.payload.get("lifecycleSequence").and_then(serde_json::Value::as_i64);
        let latest_sequence = events
            .iter()
            .filter(|existing| {
                existing.agent_id == event.agent_id && existing.generation_fingerprint == event.generation_fingerprint
            })
            .filter_map(|existing| existing.payload.get("lifecycleSequence").and_then(serde_json::Value::as_i64))
            .max();
        if let Some(existing) = events.iter().find(|existing| {
            existing.agent_id == event.agent_id
                && existing.generation_fingerprint == event.generation_fingerprint
                && existing.ingest_event_id == event.ingest_event_id
        }) {
            return Ok(if existing == &event {
                if sequence.zip(latest_sequence).is_some_and(|(current, latest)| current < latest) {
                    EventIngestOutcome::DuplicateSuperseded
                } else {
                    EventIngestOutcome::DuplicateApplied
                }
            } else {
                EventIngestOutcome::EventIdConflict
            });
        }
        if sequence.zip(latest_sequence).is_some_and(|(current, latest)| current <= latest) {
            events.push(event);
            return Ok(EventIngestOutcome::Superseded);
        }
        if !patch.is_noop() {
            self.runtime_patches.lock().await.push((event.agent_id, patch));
        }
        events.push(event);
        Ok(EventIngestOutcome::Applied)
    }
}

#[derive(Clone, Default)]
struct MemoryAgentDirectory {
    target: Arc<Mutex<Option<AgentTarget>>>,
}

impl MemoryAgentDirectory {
    async fn set_target(&self, target: AgentTarget) {
        *self.target.lock().await = Some(target);
    }
}

#[async_trait]
impl AgentDirectory for MemoryAgentDirectory {
    async fn resolve(&self, agent_id: Uuid) -> Result<Option<AgentTarget>> {
        let target = self.target.lock().await.clone().expect("agent target configured");
        assert_eq!(target.agent_id, agent_id);
        Ok(Some(target))
    }
}

#[derive(Clone, Default)]
struct MemoryBroadcastBus {
    published: Arc<Mutex<Vec<(String, ServerMessage)>>>,
}

impl MemoryBroadcastBus {
    async fn published(&self) -> Vec<(String, ServerMessage)> {
        self.published.lock().await.clone()
    }
}

#[async_trait]
impl BroadcastBus for MemoryBroadcastBus {
    async fn publish(&self, subject: String, message: ServerMessage) -> Result<()> {
        self.published.lock().await.push((subject, message));
        Ok(())
    }
}

fn subject(agent_id: Uuid) -> String {
    format!("events.ingest.{agent_id}")
}

/// Sign an event envelope the way the sidecar's `EventPublisher` does: HMAC
/// over the canonical `agent_id:timestamp:{"event_type":…,"data":…}` form.
/// We build a core `SignedEnvelope` (same wire shape + canonical form) to get
/// the signature, then copy its fields into a `SignedEventEnvelope`.
fn sign_event(secret: &str, agent_id: Uuid, payload: SignedEventPayload, timestamp: i64) -> SignedEventEnvelope {
    let payload_value = serde_json::to_value(&payload).expect("serialize event payload");
    let env = SignedEnvelope::sign(secret.as_bytes(), &agent_id.to_string(), timestamp, &payload_value)
        .expect("sign event envelope");
    SignedEventEnvelope { payload, timestamp, agent_id: agent_id.to_string(), signature: env.signature }
}

fn event_payload(event_type: &str) -> SignedEventPayload {
    let lifecycle_sequence = match event_type {
        "pre_tool_use" | "user_prompt_submit" => Some(1),
        "stop" | "session_end" => Some(2),
        _ => None,
    };
    let mut data = serde_json::json!({
        "id": format!("evt-{event_type}"),
        "timestamp": 1_700_000_000_000_u64,
        "sessionId": "contract-session",
        "cwd": "/workspace/project",
        "tool": "Read"
    });
    if let Some(sequence) = lifecycle_sequence {
        data["lifecycleSequence"] = serde_json::json!(sequence);
    }
    SignedEventPayload { event_type: event_type.to_string(), data }
}

/// A validly-signed event envelope with a fresh timestamp so the replay
/// window doesn't trip. Signed with `TEST_HMAC`, matching `MemoryHmac::with`.
fn signed_event(agent_id: Uuid, event_type: &str) -> SignedEventEnvelope {
    sign_event(TEST_HMAC, agent_id, event_payload(event_type), chrono::Utc::now().timestamp())
}

#[tokio::test]
async fn persistable_event_is_stored_and_rebroadcast() {
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let store = MemoryEventStore::default();
    let agents = MemoryAgentDirectory::default();
    agents
        .set_target(AgentTarget {
            agent_id,
            organization_id: org_id,
            cli_session_id: Some("cli-session-1".to_string()),
        })
        .await;
    let bus = MemoryBroadcastBus::default();
    let consumer =
        EventConsumer::new(store.clone(), agents.clone(), bus.clone(), MemoryHmac::with(agent_id, TEST_HMAC));

    consumer.handle(&subject(agent_id), signed_event(agent_id, "pre_tool_use")).await.unwrap();

    let persisted = store.snapshot().await;
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].organization_id, org_id);
    assert_eq!(persisted[0].agent_id, agent_id);
    assert_eq!(persisted[0].event_type, "pre_tool_use");
    assert_eq!(persisted[0].session_id.as_deref(), Some("contract-session"));
    assert_eq!(persisted[0].payload["type"], "pre_tool_use");
    assert_eq!(persisted[0].payload["sessionId"], "contract-session");

    let published = bus.published().await;
    assert_eq!(published.len(), 2);
    assert_eq!(published[0].0, format!("broadcast.{org_id}"));
    let ServerMessage::Event { event_type, event_data, agent_id: event_agent, org_id: event_org } = &published[0].1
    else {
        panic!("first broadcast must be event");
    };
    assert_eq!(event_type, "pre_tool_use");
    assert_eq!(event_agent, "contract-session");
    assert_eq!(event_org, &org_id.to_string());
    assert_eq!(event_data["type"], "pre_tool_use");
    assert_eq!(event_data["sessionId"], "contract-session");

    assert_eq!(published[1].0, format!("broadcast.{org_id}"));
    let ServerMessage::TurnInvalidate { payload } = &published[1].1 else {
        panic!("second broadcast must invalidate turns");
    };
    assert_eq!(payload.agent_id, agent_id.to_string());
    assert_eq!(payload.timestamp, 1_700_000_000_000);
}

#[tokio::test]
async fn token_update_is_broadcast_without_persistence() {
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let store = MemoryEventStore::default();
    let agents = MemoryAgentDirectory::default();
    agents
        .set_target(AgentTarget {
            agent_id,
            organization_id: org_id,
            cli_session_id: Some("cli-session-2".to_string()),
        })
        .await;
    let bus = MemoryBroadcastBus::default();
    let consumer = EventConsumer::new(store.clone(), agents, bus.clone(), MemoryHmac::with(agent_id, TEST_HMAC));

    consumer.handle(&subject(agent_id), signed_event(agent_id, "token_update")).await.unwrap();

    assert!(store.snapshot().await.is_empty());
    assert!(store.runtime_patches().await.is_empty());
    let published = bus.published().await;
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].0, format!("broadcast.{org_id}"));
    let ServerMessage::Event { event_type, .. } = &published[0].1 else {
        panic!("token updates should only emit event broadcasts");
    };
    assert_eq!(event_type, "token_update");
}

#[tokio::test]
async fn malformed_payload_is_rejected_without_side_effects() {
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let store = MemoryEventStore::default();
    let agents = MemoryAgentDirectory::default();
    agents
        .set_target(AgentTarget {
            agent_id,
            organization_id: org_id,
            cli_session_id: Some("cli-session-3".to_string()),
        })
        .await;
    let bus = MemoryBroadcastBus::default();
    let consumer =
        EventConsumer::new(store.clone(), agents.clone(), bus.clone(), MemoryHmac::with(agent_id, TEST_HMAC));
    // Validly signed + fresh ts so the malformed payload reaches the decode
    // step (the thing under test) instead of being dropped at the HMAC or
    // ts-window gate that now runs first.
    let malformed = sign_event(
        TEST_HMAC,
        agent_id,
        SignedEventPayload { event_type: "pre_tool_use".to_string(), data: serde_json::json!(["not", "an", "object"]) },
        chrono::Utc::now().timestamp(),
    );

    let err = consumer.handle(&subject(agent_id), malformed).await.unwrap_err();
    assert!(err.to_string().contains("permanent event rejection"));
    assert!(store.snapshot().await.is_empty());
    assert!(store.runtime_patches().await.is_empty());
    assert!(bus.published().await.is_empty());
}

#[tokio::test]
async fn status_event_updates_agent_row() {
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let store = MemoryEventStore::default();
    let agents = MemoryAgentDirectory::default();
    agents.set_target(AgentTarget { agent_id, organization_id: org_id, cli_session_id: None }).await;
    let bus = MemoryBroadcastBus::default();
    let consumer = EventConsumer::new(store.clone(), agents, bus, MemoryHmac::with(agent_id, TEST_HMAC));

    consumer.handle(&subject(agent_id), signed_event(agent_id, "user_prompt_submit")).await.unwrap();
    consumer.handle(&subject(agent_id), signed_event(agent_id, "stop")).await.unwrap();

    assert_eq!(store.status_updates().await, vec![(agent_id, AgentStatus::Working), (agent_id, AgentStatus::Idle)]);
}

// --- Issue #30: event consumer writes cwd + current_tool to the agents row ---

#[tokio::test]
async fn pre_tool_use_writes_current_tool_and_cwd() {
    // ChatView's "Working Dir" column and the admin panel's "Current tool"
    // field both read from the agents row, not the event stream. The previous
    // code only touched `status`, so those columns stayed NULL even while
    // events flowed in. This test pins the fix: a single `pre_tool_use` must
    // result in ONE patch that carries status=working, tool=Read, and cwd.
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let store = MemoryEventStore::default();
    let agents = MemoryAgentDirectory::default();
    agents.set_target(AgentTarget { agent_id, organization_id: org_id, cli_session_id: None }).await;
    let bus = MemoryBroadcastBus::default();
    let consumer = EventConsumer::new(store.clone(), agents, bus, MemoryHmac::with(agent_id, TEST_HMAC));

    consumer.handle(&subject(agent_id), signed_event(agent_id, "pre_tool_use")).await.unwrap();

    let patches = store.runtime_patches().await;
    assert_eq!(patches.len(), 1, "one UPDATE per event — avoid write amplification");
    let (id, patch) = &patches[0];
    assert_eq!(*id, agent_id);
    assert_eq!(patch.status, Some(AgentStatus::Working));
    assert_eq!(
        patch.current_tool,
        Some(agentforge_jobs::event_consumer::CurrentToolUpdate::Set("Read".to_owned())),
        "pre_tool_use must write the tool name",
    );
    assert_eq!(patch.cwd.as_deref(), Some("/workspace/project"));
}

#[tokio::test]
async fn post_tool_use_clears_current_tool() {
    // After the tool finishes, the UI should show "no active tool" instead
    // of the stale last tool. The `post_tool_use` event carries a `tool`
    // field too, but the column must be NULLed — otherwise a crashed agent
    // would show a forever-running tool.
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let store = MemoryEventStore::default();
    let agents = MemoryAgentDirectory::default();
    agents.set_target(AgentTarget { agent_id, organization_id: org_id, cli_session_id: None }).await;
    let bus = MemoryBroadcastBus::default();
    let consumer = EventConsumer::new(store.clone(), agents, bus, MemoryHmac::with(agent_id, TEST_HMAC));

    consumer.handle(&subject(agent_id), signed_event(agent_id, "post_tool_use")).await.unwrap();

    let patches = store.runtime_patches().await;
    assert_eq!(patches.len(), 1);
    let patch = &patches[0].1;
    assert_eq!(
        patch.current_tool,
        Some(agentforge_jobs::event_consumer::CurrentToolUpdate::Clear),
        "post_tool_use must clear current_tool, not carry the ending tool name forward",
    );
    assert_eq!(patch.status, None, "post_tool_use must not flip status by itself — only stop/session_end go Idle");
}

#[tokio::test]
async fn stop_event_clears_current_tool_and_sets_idle() {
    // Two columns move together on stop/session_end: status → idle AND
    // current_tool → NULL. A half-applied update (only status) would leave
    // a stale tool badge on the agent card.
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let store = MemoryEventStore::default();
    let agents = MemoryAgentDirectory::default();
    agents.set_target(AgentTarget { agent_id, organization_id: org_id, cli_session_id: None }).await;
    let bus = MemoryBroadcastBus::default();
    let consumer = EventConsumer::new(store.clone(), agents, bus, MemoryHmac::with(agent_id, TEST_HMAC));

    consumer.handle(&subject(agent_id), signed_event(agent_id, "stop")).await.unwrap();

    let patch = &store.runtime_patches().await[0].1;
    assert_eq!(patch.status, Some(AgentStatus::Idle));
    assert_eq!(patch.current_tool, Some(agentforge_jobs::event_consumer::CurrentToolUpdate::Clear));
}

#[tokio::test]
async fn event_without_cwd_does_not_overwrite_stored_cwd() {
    // Empty-string cwd comes from optional-field absence, not from a valid
    // "no working dir" state. Overwriting would wipe a previously-reported
    // cwd on every cwd-less event (e.g. token_update). Treat empty cwd as
    // "leave it alone".
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let store = MemoryEventStore::default();
    let agents = MemoryAgentDirectory::default();
    agents.set_target(AgentTarget { agent_id, organization_id: org_id, cli_session_id: None }).await;
    let bus = MemoryBroadcastBus::default();
    let consumer = EventConsumer::new(store.clone(), agents, bus, MemoryHmac::with(agent_id, TEST_HMAC));
    let envelope = sign_event(
        TEST_HMAC,
        agent_id,
        SignedEventPayload {
            event_type: "notification".to_string(),
            // `cwd = ""` mimics the hook serializer writing empty strings
            // for missing optional fields.
            data: serde_json::json!({ "id": "evt", "timestamp": 0u64, "cwd": "" }),
        },
        chrono::Utc::now().timestamp(),
    );

    consumer.handle(&subject(agent_id), envelope).await.unwrap();

    // No status, no tool, no cwd → nothing to patch → no directory call.
    assert!(store.runtime_patches().await.is_empty(), "noop patch must skip the UPDATE");
}

// -------------------------------------------------------------------
// Issue #458: HMAC verify + timestamp replay window for event ingest.
// Before this fix the consumer ignored the `signature` and `timestamp`
// fields entirely, so any party who could publish to `events.ingest.*`
// (or replay a captured frame) could forge agent telemetry and runtime
// state. These pin the closed gap.
// -------------------------------------------------------------------

/// Build a consumer + directory wired for an agent that verifies against
/// `TEST_HMAC`, returning everything the assertions need.
async fn wired_consumer(
    agent_id: Uuid,
    org_id: Uuid,
) -> (
    EventConsumer<MemoryEventStore, MemoryAgentDirectory, MemoryBroadcastBus, MemoryHmac>,
    MemoryEventStore,
    MemoryAgentDirectory,
    MemoryBroadcastBus,
) {
    let store = MemoryEventStore::default();
    let agents = MemoryAgentDirectory::default();
    agents.set_target(AgentTarget { agent_id, organization_id: org_id, cli_session_id: None }).await;
    let bus = MemoryBroadcastBus::default();
    let consumer =
        EventConsumer::new(store.clone(), agents.clone(), bus.clone(), MemoryHmac::with(agent_id, TEST_HMAC));
    (consumer, store, agents, bus)
}

#[tokio::test]
async fn rejects_event_with_wrong_hmac_signature() {
    // Envelope signed with a key the backend doesn't hold. Must be dropped
    // before any persist / runtime-patch / broadcast side effect.
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let (consumer, store, _agents, bus) = wired_consumer(agent_id, org_id).await;

    let forged = sign_event("attacker-key", agent_id, event_payload("pre_tool_use"), chrono::Utc::now().timestamp());
    let err = consumer.handle(&subject(agent_id), forged).await.unwrap_err();

    assert!(matches!(err, ConsumeError::Permanent { .. }), "bad signature must be permanent: {err}");
    assert!(err.to_string().contains("signature_mismatch"), "err = {err}");
    assert!(store.snapshot().await.is_empty(), "forged event must not persist");
    assert!(store.runtime_patches().await.is_empty(), "forged event must not patch the agent row");
    assert!(bus.published().await.is_empty(), "forged event must not broadcast");
}

#[tokio::test]
async fn rejects_event_when_agent_has_no_stored_secret() {
    // Agent target resolves, but no HMAC secret is registered (unknown /
    // pre-migration / stopped). Treated identically to a bad signature.
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let store = MemoryEventStore::default();
    let agents = MemoryAgentDirectory::default();
    agents.set_target(AgentTarget { agent_id, organization_id: org_id, cli_session_id: None }).await;
    let bus = MemoryBroadcastBus::default();
    // Empty HMAC map → find_secret returns Ok(None) for every agent.
    let consumer = EventConsumer::new(store.clone(), agents.clone(), bus.clone(), MemoryHmac::default());

    let valid = signed_event(agent_id, "pre_tool_use");
    let err = consumer.handle(&subject(agent_id), valid).await.unwrap_err();

    assert!(err.to_string().contains("agent_unknown"), "err = {err}");
    assert!(store.snapshot().await.is_empty());
    assert!(store.runtime_patches().await.is_empty());
    assert!(bus.published().await.is_empty());
}

#[tokio::test]
async fn rejects_event_outside_replay_window() {
    // Correctly signed, but stamped well past the 5-minute window — the
    // canonical captured-and-replayed-later attack. Rejected on timestamp,
    // before signature lookup even runs.
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let (consumer, store, _agents, bus) = wired_consumer(agent_id, org_id).await;

    let stale_ts = chrono::Utc::now().timestamp() - (TIMESTAMP_REPLAY_WINDOW_SECS + 60);
    let stale = sign_event(TEST_HMAC, agent_id, event_payload("pre_tool_use"), stale_ts);
    let err = consumer.handle(&subject(agent_id), stale).await.unwrap_err();

    assert!(err.to_string().contains("timestamp_outside_window"), "err = {err}");
    assert!(store.snapshot().await.is_empty());
    assert!(store.runtime_patches().await.is_empty());
    assert!(bus.published().await.is_empty());
}

#[tokio::test]
async fn accepts_event_near_replay_window_edge() {
    // A still-old event, just inside the window, flows through the full handle
    // path and is persisted. We stamp `WINDOW - 5s` rather than exactly `WINDOW`
    // so the few milliseconds between stamping and the consumer re-reading the
    // clock can't push it over the edge (that race made this test flaky in CI:
    // envelope ts vs now came out 301 against a 300s window). The EXACT inclusive
    // boundary is pinned deterministically in `agentforge_jobs::replay_window`.
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let (consumer, store, _agents, _bus) = wired_consumer(agent_id, org_id).await;

    let near_edge_ts = chrono::Utc::now().timestamp() - (TIMESTAMP_REPLAY_WINDOW_SECS - 5);
    let edge = sign_event(TEST_HMAC, agent_id, event_payload("pre_tool_use"), near_edge_ts);
    consumer.handle(&subject(agent_id), edge).await.unwrap();

    assert_eq!(store.snapshot().await.len(), 1, "near-edge event must be accepted");
}

#[tokio::test]
async fn replayed_event_is_rejected_once_window_expires() {
    // A captured, validly-signed envelope is deduplicated by its stable event
    // ID while fresh, then rejected by the timestamp window once it expires.
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let (consumer, store, _agents, bus) = wired_consumer(agent_id, org_id).await;

    // Capture one envelope and feed it twice while still fresh. The second
    // delivery retries the broadcast but cannot duplicate database effects.
    let captured = signed_event(agent_id, "pre_tool_use");
    consumer.handle(&subject(agent_id), captured.clone()).await.unwrap();
    consumer.handle(&subject(agent_id), captured.clone()).await.unwrap();
    assert_eq!(store.snapshot().await.len(), 1, "fresh redelivery must reuse the durable receipt");
    assert_eq!(bus.published().await.len(), 4, "the latest receipt may retry its two broadcasts");

    // The same captured bytes, but representing a frame whose timestamp has
    // aged past the window, are rejected. (We rebuild with an old ts and the
    // matching signature to model the captured frame becoming stale.)
    let stale_ts = chrono::Utc::now().timestamp() - (TIMESTAMP_REPLAY_WINDOW_SECS + 1);
    let stale_replay = sign_event(TEST_HMAC, agent_id, event_payload("pre_tool_use"), stale_ts);
    let err = consumer.handle(&subject(agent_id), stale_replay).await.unwrap_err();
    assert!(err.to_string().contains("timestamp_outside_window"), "stale replay must be rejected: {err}");
    assert_eq!(store.snapshot().await.len(), 1, "stale replay must not append");
}

#[tokio::test]
async fn superseded_lifecycle_redelivery_does_not_rebroadcast() {
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let (consumer, store, _agents, bus) = wired_consumer(agent_id, org_id).await;
    let working = signed_event(agent_id, "pre_tool_use");

    consumer.handle(&subject(agent_id), working.clone()).await.unwrap();
    consumer.handle(&subject(agent_id), signed_event(agent_id, "stop")).await.unwrap();
    consumer.handle(&subject(agent_id), working).await.unwrap();

    assert_eq!(store.snapshot().await.len(), 2, "redelivery must reuse its original receipt");
    assert_eq!(bus.published().await.len(), 4, "a superseded receipt must not resurrect stale browser state");
}

#[tokio::test]
async fn reused_event_id_with_different_content_is_rejected() {
    let agent_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let (consumer, store, _agents, bus) = wired_consumer(agent_id, org_id).await;
    let first = signed_event(agent_id, "pre_tool_use");
    consumer.handle(&subject(agent_id), first.clone()).await.unwrap();

    let mut changed_payload = first.payload;
    changed_payload.data["tool"] = serde_json::json!("Write");
    let changed = sign_event(TEST_HMAC, agent_id, changed_payload, first.timestamp);
    let err = consumer.handle(&subject(agent_id), changed).await.unwrap_err();

    assert!(err.to_string().contains("event_id_conflict"), "conflicting receipt must be permanent: {err}");
    assert_eq!(store.snapshot().await.len(), 1);
    assert_eq!(bus.published().await.len(), 2);
}

#[tokio::test]
async fn rejects_event_subject_envelope_agent_mismatch() {
    // Subject addresses agent A; the envelope claims agent B. Rejected before
    // verification so a forger who controls B's secret cannot speak for A.
    let subject_agent = Uuid::now_v7();
    let envelope_agent = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let (consumer, store, _agents, bus) = wired_consumer(subject_agent, org_id).await;

    // Validly signed for envelope_agent, delivered on subject_agent's subject.
    let mismatched =
        sign_event(TEST_HMAC, envelope_agent, event_payload("pre_tool_use"), chrono::Utc::now().timestamp());
    let err = consumer.handle(&subject(subject_agent), mismatched).await.unwrap_err();

    assert!(err.to_string().contains("envelope_agent_mismatch"), "err = {err}");
    assert!(store.snapshot().await.is_empty());
    assert!(store.runtime_patches().await.is_empty());
    assert!(bus.published().await.is_empty());
}

// -------------------------------------------------------------------
// #811 dead-letter capture for the EVENT INGEST worker (NATS-gated).
// Mirrors `forged_result_is_recorded_as_a_dead_event` for the orchestration
// path: a permanently-dropped events.ingest message must be captured to the
// recorder before the worker Terms it. Skips when no NATS / DB is reachable.
// -------------------------------------------------------------------

use agentforge_infra::nats::connect_nats;
use agentforge_jobs::{
    DEAD_EVENT_PAYLOAD_MAX_BYTES, DeadEvent, DeadEventRecorder, EVENTS_FILTER, EVENTS_STREAM, EventStreamWorker,
};
use async_nats::jetstream::{self, stream};
use sqlx::PgPool;
use tokio::sync::watch;

/// Collects every dead event the event worker records.
#[derive(Default)]
struct CapturingDeadEventRecorder {
    events: Mutex<Vec<DeadEvent>>,
}

#[async_trait]
impl DeadEventRecorder for CapturingDeadEventRecorder {
    async fn record(&self, ev: DeadEvent) {
        self.events.lock().await.push(ev);
    }
}

/// Connect to a local NATS server, skipping (returning `None`) when unavailable
/// so CI without infra still passes. Mirrors the orchestration contract test.
async fn try_connect() -> Option<async_nats::Client> {
    for (label, url) in nats_candidates() {
        match tokio::time::timeout(Duration::from_millis(500), connect_nats(&url)).await {
            Ok(Ok(client)) => return Some(client),
            Ok(Err(err)) => eprintln!("skipping: NATS connect {label}: {err}"),
            Err(_) => eprintln!("skipping: NATS connect {label}: timeout"),
        }
    }
    None
}

fn nats_candidates() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let docker_env = read_docker_env();
    let mut push = |label: String, url: String| {
        if seen.insert(url.clone()) {
            out.push((label, url));
        }
    };
    if let Ok(url) = std::env::var("NATS_URL") {
        push("env:NATS_URL".to_string(), url);
    }
    if let Some(url) = docker_env.get("NATS_URL").cloned() {
        push("docker/.env:NATS_URL".to_string(), url);
    }
    let port = std::env::var("NATS_PORT")
        .ok()
        .or_else(|| docker_env.get("NATS_PORT").cloned())
        .unwrap_or_else(|| "4222".to_string());
    if let Some(password) =
        std::env::var("NATS_BACKEND_PASSWORD").ok().or_else(|| docker_env.get("NATS_BACKEND_PASSWORD").cloned())
    {
        push("docker/.env backend user".to_string(), format!("nats://backend:{password}@127.0.0.1:{port}"));
    }
    push("localhost anonymous".to_string(), format!("nats://127.0.0.1:{port}"));
    out
}

fn read_docker_env() -> HashMap<String, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docker/.env");
    let Ok(contents) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

/// Ensure the production `EVENTS` JetStream stream exists (the worker's `connect`
/// opens it by name). WorkQueue retention so each test message is consumed once.
async fn ensure_events_stream(client: async_nats::Client) {
    let js = jetstream::new(client);
    js.create_or_update_stream(stream::Config {
        name: EVENTS_STREAM.to_string(),
        subjects: vec![EVENTS_FILTER.to_string()],
        retention: stream::RetentionPolicy::WorkQueue,
        storage: stream::StorageType::File,
        max_age: Duration::from_secs(24 * 60 * 60),
        discard: stream::DiscardPolicy::Old,
        ..Default::default()
    })
    .await
    .expect("ensure EVENTS stream");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn malformed_event_envelope_is_recorded_as_a_dead_event(pool: PgPool) {
    // A body that won't decode into a SignedEventEnvelope is a permanent drop the
    // worker must dead-letter (source="events.ingest", reason="envelope_decode_failed")
    // before it acks. Pre-decode, so org_id is NULL and the DB is never touched
    // by the handler — only the recorder is exercised.
    let Some(client) = try_connect().await else {
        return;
    };
    ensure_events_stream(client.clone()).await;

    let recorder = Arc::new(CapturingDeadEventRecorder::default());
    let worker = EventStreamWorker::connect(pool.clone(), client.clone())
        .await
        .expect("connect event worker")
        .with_dead_event_recorder(recorder.clone());
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let handle = tokio::spawn(async move { worker.run(shutdown_rx).await });

    let agent_id = Uuid::now_v7();
    let event_subject = format!("events.ingest.{agent_id}");
    // Oversized non-JSON body → decode fails AND the stored excerpt must truncate.
    let garbage = vec![b'Q'; DEAD_EVENT_PAYLOAD_MAX_BYTES * 2];
    let js = jetstream::new(client.clone());
    js.publish(event_subject.clone(), garbage.into())
        .await
        .expect("publish malformed accepted")
        .await
        .expect("publish malformed ack");

    let mut captured = Vec::new();
    for _ in 0..40 {
        captured = recorder.events.lock().await.clone();
        if !captured.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert_eq!(captured.len(), 1, "exactly one dead event for the malformed envelope");
    let ev = &captured[0];
    assert_eq!(ev.source, "events.ingest");
    assert_eq!(ev.reason, "envelope_decode_failed");
    assert_eq!(ev.subject, event_subject);
    assert!(ev.org_id.is_none(), "org_id is NULL on a pre-decode drop");
    let excerpt = ev.payload_excerpt.as_ref().expect("excerpt stored");
    assert!(excerpt.len() <= DEAD_EVENT_PAYLOAD_MAX_BYTES, "excerpt truncated at the cap");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}
