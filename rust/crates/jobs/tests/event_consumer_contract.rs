use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

use agentforge_core::AgentStatus;
use agentforge_jobs::event_consumer::{
    AgentDirectory, AgentRuntimePatch, AgentTarget, BroadcastBus, BroadcastEnvelope, EventConsumer, EventStore,
    PersistedEvent, SignedEventEnvelope, SignedEventPayload,
};

#[derive(Clone, Default)]
struct MemoryEventStore {
    events: Arc<Mutex<Vec<PersistedEvent>>>,
}

impl MemoryEventStore {
    async fn snapshot(&self) -> Vec<PersistedEvent> {
        self.events.lock().await.clone()
    }
}

#[async_trait]
impl EventStore for MemoryEventStore {
    async fn persist(&self, event: PersistedEvent) -> Result<()> {
        self.events.lock().await.push(event);
        Ok(())
    }
}

#[derive(Clone, Default)]
struct MemoryAgentDirectory {
    target: Arc<Mutex<Option<AgentTarget>>>,
    runtime_patches: Arc<Mutex<Vec<(Uuid, AgentRuntimePatch)>>>,
}

impl MemoryAgentDirectory {
    async fn set_target(&self, target: AgentTarget) {
        *self.target.lock().await = Some(target);
    }

    async fn runtime_patches(&self) -> Vec<(Uuid, AgentRuntimePatch)> {
        self.runtime_patches.lock().await.clone()
    }

    async fn status_updates(&self) -> Vec<(Uuid, AgentStatus)> {
        self.runtime_patches.lock().await.iter().filter_map(|(id, patch)| patch.status.map(|s| (*id, s))).collect()
    }
}

#[async_trait]
impl AgentDirectory for MemoryAgentDirectory {
    async fn resolve(&self, agent_id: Uuid) -> Result<Option<AgentTarget>> {
        let target = self.target.lock().await.clone().expect("agent target configured");
        assert_eq!(target.agent_id, agent_id);
        Ok(Some(target))
    }

    async fn apply_runtime_patch(&self, agent_id: Uuid, patch: AgentRuntimePatch) -> Result<bool> {
        self.runtime_patches.lock().await.push((agent_id, patch));
        Ok(true)
    }
}

#[derive(Clone, Default)]
struct MemoryBroadcastBus {
    published: Arc<Mutex<Vec<(String, BroadcastEnvelope)>>>,
}

impl MemoryBroadcastBus {
    async fn published(&self) -> Vec<(String, BroadcastEnvelope)> {
        self.published.lock().await.clone()
    }
}

#[async_trait]
impl BroadcastBus for MemoryBroadcastBus {
    async fn publish(&self, subject: String, message: BroadcastEnvelope) -> Result<()> {
        self.published.lock().await.push((subject, message));
        Ok(())
    }
}

fn subject(agent_id: Uuid) -> String {
    format!("events.ingest.{agent_id}")
}

fn signed_event(agent_id: Uuid, event_type: &str) -> SignedEventEnvelope {
    SignedEventEnvelope {
        payload: SignedEventPayload {
            event_type: event_type.to_string(),
            data: serde_json::json!({
                "id": format!("evt-{event_type}"),
                "timestamp": 1_700_000_000_000_u64,
                "cwd": "/workspace/project",
                "tool": "Read"
            }),
        },
        timestamp: 1_700_000_000,
        agent_id: agent_id.to_string(),
        signature: "sig".to_string(),
    }
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
    let consumer = EventConsumer::new(store.clone(), agents.clone(), bus.clone());

    consumer.handle(&subject(agent_id), signed_event(agent_id, "pre_tool_use")).await.unwrap();

    let persisted = store.snapshot().await;
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].organization_id, org_id);
    assert_eq!(persisted[0].agent_id, agent_id);
    assert_eq!(persisted[0].event_type, "pre_tool_use");
    assert_eq!(persisted[0].session_id.as_deref(), Some("cli-session-1"));
    assert_eq!(persisted[0].payload["type"], "pre_tool_use");
    assert_eq!(persisted[0].payload["sessionId"], "cli-session-1");

    let published = bus.published().await;
    assert_eq!(published.len(), 2);
    assert_eq!(published[0].0, format!("broadcast.{org_id}"));
    let BroadcastEnvelope::Event(event_message) = &published[0].1 else {
        panic!("first broadcast must be event");
    };
    assert_eq!(event_message.event_type, "pre_tool_use");
    assert_eq!(event_message.agent_id, "cli-session-1");
    assert_eq!(event_message.org_id, org_id.to_string());
    assert_eq!(event_message.event_data["type"], "pre_tool_use");
    assert_eq!(event_message.event_data["sessionId"], "cli-session-1");

    assert_eq!(published[1].0, format!("broadcast.{org_id}"));
    let BroadcastEnvelope::TurnInvalidate(invalidate) = &published[1].1 else {
        panic!("second broadcast must invalidate turns");
    };
    assert_eq!(invalidate.payload.agent_id, agent_id.to_string());
    assert_eq!(invalidate.payload.timestamp, 1_700_000_000_000);
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
    let consumer = EventConsumer::new(store.clone(), agents, bus.clone());

    consumer.handle(&subject(agent_id), signed_event(agent_id, "token_update")).await.unwrap();

    assert!(store.snapshot().await.is_empty());
    let published = bus.published().await;
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].0, format!("broadcast.{org_id}"));
    let BroadcastEnvelope::Event(event_message) = &published[0].1 else {
        panic!("token updates should only emit event broadcasts");
    };
    assert_eq!(event_message.event_type, "token_update");
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
    let consumer = EventConsumer::new(store.clone(), agents.clone(), bus.clone());
    let malformed = SignedEventEnvelope {
        payload: SignedEventPayload {
            event_type: "pre_tool_use".to_string(),
            data: serde_json::json!(["not", "an", "object"]),
        },
        timestamp: 1_700_000_000,
        agent_id: agent_id.to_string(),
        signature: "sig".to_string(),
    };

    let err = consumer.handle(&subject(agent_id), malformed).await.unwrap_err();
    assert!(err.to_string().contains("permanent event rejection"));
    assert!(store.snapshot().await.is_empty());
    assert!(agents.runtime_patches().await.is_empty());
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
    let consumer = EventConsumer::new(store, agents.clone(), bus);

    consumer.handle(&subject(agent_id), signed_event(agent_id, "user_prompt_submit")).await.unwrap();
    consumer.handle(&subject(agent_id), signed_event(agent_id, "stop")).await.unwrap();

    assert_eq!(agents.status_updates().await, vec![(agent_id, AgentStatus::Working), (agent_id, AgentStatus::Idle)]);
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
    let consumer = EventConsumer::new(store, agents.clone(), bus);

    consumer.handle(&subject(agent_id), signed_event(agent_id, "pre_tool_use")).await.unwrap();

    let patches = agents.runtime_patches().await;
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
    let consumer = EventConsumer::new(store, agents.clone(), bus);

    consumer.handle(&subject(agent_id), signed_event(agent_id, "post_tool_use")).await.unwrap();

    let patches = agents.runtime_patches().await;
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
    let consumer = EventConsumer::new(store, agents.clone(), bus);

    consumer.handle(&subject(agent_id), signed_event(agent_id, "stop")).await.unwrap();

    let patch = &agents.runtime_patches().await[0].1;
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
    let consumer = EventConsumer::new(store, agents.clone(), bus);
    let envelope = SignedEventEnvelope {
        payload: SignedEventPayload {
            event_type: "notification".to_string(),
            // `cwd = ""` mimics the hook serializer writing empty strings
            // for missing optional fields.
            data: serde_json::json!({ "id": "evt", "timestamp": 0u64, "cwd": "" }),
        },
        timestamp: 0,
        agent_id: agent_id.to_string(),
        signature: "sig".to_string(),
    };

    consumer.handle(&subject(agent_id), envelope).await.unwrap();

    // No status, no tool, no cwd → nothing to patch → no directory call.
    assert!(agents.runtime_patches().await.is_empty(), "noop patch must skip the UPDATE");
}
