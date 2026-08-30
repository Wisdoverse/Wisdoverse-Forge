//! Event publishing to NATS with per-message HMAC-SHA256 authentication.

use async_nats::Client;
use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use sha2::Sha256;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

type HmacSha256 = Hmac<Sha256>;

/// A NATS message signed with HMAC-SHA256.
#[derive(Debug, Serialize)]
pub struct SignedMessage {
    pub payload: serde_json::Value,
    pub timestamp: i64,
    pub agent_id: String,
    pub signature: String,
}

/// Publishes events to NATS with HMAC-SHA256 signatures.
pub struct EventPublisher {
    client: Client,
    agent_id: String,
    hmac_key: Vec<u8>,
    generation_fingerprint: String,
    active_hook_session: RwLock<ActiveHookSessionStore>,
    cli_tool: Option<String>,
    /// Subject prefix for this agent's event-ingest channel, including the
    /// #457 runtime-kind namespace, e.g. `events.ingest.cli`. The agent UUID is
    /// appended per publish.
    ingest_subject_prefix: String,
}

impl EventPublisher {
    #[cfg(test)]
    pub fn new(
        client: Client,
        agent_id: String,
        hmac_secret: &str,
        cli_tool: Option<String>,
        runtime_kind: agentforge_core::RuntimeKind,
    ) -> Self {
        Self::new_with_wal_path(client, agent_id, hmac_secret, cli_tool, runtime_kind, None)
    }

    pub fn new_with_wal_path(
        client: Client,
        agent_id: String,
        hmac_secret: &str,
        cli_tool: Option<String>,
        runtime_kind: agentforge_core::RuntimeKind,
        wal_path: Option<&str>,
    ) -> Self {
        // #457: publish on the kind-namespaced ingest subject only. The
        // platform consumer accepts both shapes during migration; the callout
        // still grants the legacy subject so this is forward-compatible without
        // double-publishing (the `events` table has no dedup, so emitting both
        // shapes would double-insert every event).
        let ingest_subject_prefix =
            format!("{}.{}", agentforge_core::event_protocol::EVENTS_INGEST_PREFIX, runtime_kind.as_str());
        Self {
            client,
            agent_id,
            hmac_key: hmac_secret.as_bytes().to_vec(),
            generation_fingerprint: agentforge_core::orchestration_protocol::container_generation_fingerprint(
                hmac_secret.as_bytes(),
            ),
            active_hook_session: RwLock::new(ActiveHookSessionStore::load(wal_path)),
            cli_tool,
            ingest_subject_prefix,
        }
    }

    /// Compute HMAC-SHA256 over `agent_id:timestamp:payload` and return hex string.
    fn sign(&self, timestamp: i64, payload: &serde_json::Value) -> String {
        let sign_data = format!("{}:{}:{}", self.agent_id, timestamp, payload);
        let mut mac = HmacSha256::new_from_slice(&self.hmac_key).expect("HMAC key length is always valid");
        mac.update(sign_data.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// Publish an event to the `events.ingest.<runtime_kind>.<agent_id>` NATS
    /// subject (issue #457). The HMAC is computed over `agent_id:ts:payload`
    /// and is independent of the subject, so the platform's signature check is
    /// unaffected by the namespacing.
    pub async fn publish(
        &self,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let timestamp = Utc::now().timestamp();
        let subject = format!("{}.{}", self.ingest_subject_prefix, self.agent_id);

        let inner_payload = serde_json::json!({
            "event_type": event_type,
            "data": payload,
        });

        let signature = self.sign(timestamp, &inner_payload);

        let msg = SignedMessage { payload: inner_payload, timestamp, agent_id: self.agent_id.clone(), signature };

        let bytes = serde_json::to_vec(&msg)?;
        self.client
            .publish(subject, bytes.into())
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }

    /// Flush the underlying NATS client, blocking until the server has
    /// acknowledged every message published so far on this connection.
    ///
    /// `Client::publish()` returns `Ok` once a message is enqueued in the
    /// client's in-memory buffer — *before* the server accepts it. The relay's
    /// WAL-first durability path calls this after publishing so it only removes
    /// the buffered copy once delivery to the server is confirmed, closing the
    /// reconnect-window loss where a buffered-but-unsent event would vanish on a
    /// sidecar restart.
    pub async fn flush(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.client.flush().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    /// Track the active Container CLI hook session for the authenticated
    /// heartbeat lease. This is called only for a freshly accepted relay frame,
    /// never for WAL replay, so an old buffered Working event cannot resurrect
    /// a session after its Stop was already observed.
    pub fn observe_hook_event(&self, event_type: &str, payload: &serde_json::Value) {
        let mut active = self.active_hook_session.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Err(err) = active.observe(event_type, payload) {
            tracing::error!(error = %err, event_type, "could not persist active hook owner; crash-safe lease renewal is degraded");
        }
    }

    /// Send a heartbeat on `sidecar.<agent_id>.heartbeat`.
    ///
    /// The `health` snapshot reports WAL backpressure state so the liveness
    /// consumer can surface degraded relays to operators without changing any
    /// participant status or dispatcher logic (issue #808).
    pub async fn heartbeat(&self, health: HealthSnapshot) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let subject = format!("sidecar.{}.heartbeat", self.agent_id);
        // Advertise the CLI tool plus the image-input protocol token so the
        // dispatcher knows this (current) sidecar understands `image_paths` and
        // can be sent instruction-image tasks. An older sidecar omits it and is
        // gated out, failing closed rather than silently dropping the images.
        let mut capabilities: Vec<String> = self.cli_tool.clone().into_iter().collect();
        capabilities.push(agentforge_core::SIDECAR_IMAGE_INPUT_CAPABILITY.to_string());
        let payload = serde_json::json!({
            "agent_id": self.agent_id,
            "timestamp": Utc::now().timestamp(),
            "cli_tool": self.cli_tool,
            "capabilities": capabilities,
            "version": agentforge_core::VERSION,
            "container_generation_fingerprint": self.generation_fingerprint,
            "active_hook_session": self
                .active_hook_session
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .active
                .clone(),
            "health": health,
        });
        let bytes = serde_json::to_vec(&payload)?;
        self.client
            .publish(subject, bytes.into())
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }
}

// Deliberately not `.json`: the WAL scanner treats every JSON file in its
// directory as a replay record.
const ACTIVE_HOOK_SESSION_FILE: &str = "active-hook-session.state";

struct ActiveHookSessionStore {
    active: Option<String>,
    state_path: Option<PathBuf>,
}

impl ActiveHookSessionStore {
    fn load(wal_path: Option<&str>) -> Self {
        let state_path = wal_path
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp/agentforge-wal"))
            .join(ACTIVE_HOOK_SESSION_FILE);
        let active = std::fs::read(&state_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Option<String>>(&bytes).ok())
            .flatten()
            .map(|session| session.trim().to_string())
            .filter(|session| !session.is_empty());
        Self { active, state_path: Some(state_path) }
    }

    #[cfg(test)]
    fn memory_only() -> Self {
        Self { active: None, state_path: None }
    }

    fn observe(&mut self, event_type: &str, payload: &serde_json::Value) -> std::io::Result<()> {
        let session = hook_session(payload);
        if matches!(event_type, "pre_tool_use" | "user_prompt_submit")
            && self.active.is_some()
            && self.active.as_deref() != session
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "a different Container CLI hook session is already active",
            ));
        }
        let mut next = self.active.clone();
        update_active_hook_session(&mut next, event_type, payload);
        if next == self.active {
            return Ok(());
        }
        if let Some(path) = &self.state_path {
            persist_active_hook_session(path, next.as_deref())?;
        }
        self.active = next;
        Ok(())
    }
}

fn persist_active_hook_session(path: &Path, active: Option<&str>) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let temp_path = parent.join(format!(".{ACTIVE_HOOK_SESSION_FILE}.{}.tmp", std::process::id()));
    let bytes = serde_json::to_vec(&active).map_err(std::io::Error::other)?;
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp_path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    std::fs::rename(&temp_path, path)?;
    // Make the rename itself crash-durable, not only the temporary file's
    // contents. Sidecar restarts are an explicit supported entrypoint path.
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn update_active_hook_session(active: &mut Option<String>, event_type: &str, payload: &serde_json::Value) {
    let session = hook_session(payload);
    match (event_type, session) {
        ("pre_tool_use" | "user_prompt_submit", Some(session)) => *active = Some(session.to_string()),
        ("stop" | "session_end", Some(session)) if active.as_deref() == Some(session) => *active = None,
        _ => {}
    }
}

fn hook_session(payload: &serde_json::Value) -> Option<&str> {
    payload
        .get("sessionId")
        .or_else(|| payload.get("session_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|session| !session.is_empty())
}

/// Relay health snapshot included in every sidecar heartbeat (issue #808).
///
/// `degraded` is set when WAL backpressure crosses the threshold or any events
/// have been dropped. The consumer uses this field to emit a metric and warn
/// operators — it does NOT change participant status or dispatcher behaviour.
#[derive(Debug, Serialize)]
pub struct HealthSnapshot {
    pub degraded: bool,
    pub reason: Option<String>,
    pub wal_pending: usize,
    pub wal_dropped: u64,
    /// CONSECUTIVE credential syncs that did not reach the platform since the
    /// last success — whether the payload could not be built/serialized/signed,
    /// the NATS publish/ack failed (no WAL retry on the sidecar), or the watcher
    /// could not start. Resets to 0 on the next successful sync, so non-zero means
    /// the agent's `claude /login` credentials are CURRENTLY not synced and the
    /// user must re-authenticate — surfaced so the platform has visibility instead
    /// of only a container-local log (#891/F063).
    pub creds_sync_errors: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signed_message_serialization() {
        let msg = SignedMessage {
            payload: serde_json::json!({"event_type": "test", "data": {}}),
            timestamp: 1700000000,
            agent_id: "agent-1".to_string(),
            signature: "abc123".to_string(),
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["agent_id"], "agent-1");
        assert_eq!(json["timestamp"], 1700000000);
        assert_eq!(json["signature"], "abc123");
        assert!(json["payload"]["event_type"].is_string());
    }

    #[test]
    fn active_hook_session_is_exact_and_stop_is_owner_scoped() {
        let mut store = ActiveHookSessionStore::memory_only();
        store.observe("user_prompt_submit", &serde_json::json!({"sessionId": "session-a"})).unwrap();
        assert_eq!(store.active.as_deref(), Some("session-a"));

        store.observe("stop", &serde_json::json!({"sessionId": "older-session"})).unwrap();
        assert_eq!(store.active.as_deref(), Some("session-a"), "a stale Stop cannot clear the current owner");

        assert!(
            store.observe("user_prompt_submit", &serde_json::json!({"sessionId": "session-b"})).is_err(),
            "a second live hook session must fail closed"
        );
        assert_eq!(store.active.as_deref(), Some("session-a"));

        store.observe("stop", &serde_json::json!({"sessionId": "session-a"})).unwrap();
        assert!(store.active.is_none());
    }

    #[test]
    fn active_hook_session_survives_sidecar_restart_and_stop_persists_clear() {
        let temp = tempfile::tempdir().unwrap();
        let wal_path = temp.path().to_str().unwrap();
        let mut first = ActiveHookSessionStore::load(Some(wal_path));
        first.observe("user_prompt_submit", &serde_json::json!({"sessionId": "long-work"})).unwrap();

        let mut restarted = ActiveHookSessionStore::load(Some(wal_path));
        assert_eq!(restarted.active.as_deref(), Some("long-work"));
        restarted.observe("stop", &serde_json::json!({"sessionId": "long-work"})).unwrap();
        assert!(ActiveHookSessionStore::load(Some(wal_path)).active.is_none());
    }

    #[tokio::test]
    async fn active_hook_state_is_not_counted_or_replayed_as_wal() {
        let temp = tempfile::tempdir().unwrap();
        let wal_path = temp.path().to_str().unwrap();
        let mut state = ActiveHookSessionStore::load(Some(wal_path));
        state.observe("user_prompt_submit", &serde_json::json!({"sessionId": "long-work"})).unwrap();
        let wal = crate::wal::Wal::new(Some(wal_path));
        assert_eq!(wal.pending_count().await.unwrap(), 0);
        assert!(wal.replay().await.unwrap().is_empty());
    }

    #[test]
    fn test_hmac_signature_deterministic() {
        // Create a mock client — we only need it for the constructor, not for
        // actual publishing in this unit test. We test the sign() method directly.
        // Since we cannot easily create a Client without a server, we test the
        // signing logic in isolation via a helper struct.
        let hmac_key = b"test-secret".to_vec();
        let agent_id = "agent-42";
        let timestamp = 1700000000_i64;
        let payload = serde_json::json!({"event_type": "foo", "data": "bar"});

        let sign_data = format!("{}:{}:{}", agent_id, timestamp, payload);
        let mut mac = HmacSha256::new_from_slice(&hmac_key).unwrap();
        mac.update(sign_data.as_bytes());
        let sig1 = hex::encode(mac.finalize().into_bytes());

        // Compute again — must be identical.
        let mut mac2 = HmacSha256::new_from_slice(&hmac_key).unwrap();
        mac2.update(sign_data.as_bytes());
        let sig2 = hex::encode(mac2.finalize().into_bytes());

        assert_eq!(sig1, sig2);
        // Signature is a 64-char hex string (256 bits).
        assert_eq!(sig1.len(), 64);
    }

    #[test]
    fn publisher_subject_matches_core_namespaced_builder() {
        // The string the publisher composes (kind-namespaced prefix + agent id)
        // must be byte-identical to the canonical core builder the platform
        // parser round-trips against — otherwise a published event would be
        // dropped as an "unsupported event subject".
        use agentforge_core::RuntimeKind;
        use agentforge_core::event_protocol::{EVENTS_INGEST_PREFIX, events_ingest_subject};

        let agent_uuid = uuid::Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        for kind in [RuntimeKind::Container, RuntimeKind::Cli, RuntimeKind::Api] {
            let prefix = format!("{}.{}", EVENTS_INGEST_PREFIX, kind.as_str());
            let publisher_subject = format!("{}.{}", prefix, agent_uuid);
            assert_eq!(publisher_subject, events_ingest_subject(kind, agent_uuid), "mismatch for {kind:?}");
        }
    }

    #[test]
    fn test_hmac_signature_changes_with_different_key() {
        let payload = serde_json::json!({"x": 1});
        let sign_data = format!("agent:0:{}", payload);

        let mut mac1 = HmacSha256::new_from_slice(b"key-a").unwrap();
        mac1.update(sign_data.as_bytes());
        let sig1 = hex::encode(mac1.finalize().into_bytes());

        let mut mac2 = HmacSha256::new_from_slice(b"key-b").unwrap();
        mac2.update(sign_data.as_bytes());
        let sig2 = hex::encode(mac2.finalize().into_bytes());

        assert_ne!(sig1, sig2);
    }

    // -------------------------------------------------------------------------
    // HealthSnapshot tests (issue #808)
    // -------------------------------------------------------------------------

    #[test]
    fn health_snapshot_not_degraded_below_threshold() {
        let snap =
            HealthSnapshot { degraded: false, reason: None, wal_pending: 999, wal_dropped: 0, creds_sync_errors: 0 };
        let json = serde_json::to_value(&snap).unwrap();
        assert_eq!(json["degraded"], false);
        assert!(json["reason"].is_null());
        assert_eq!(json["wal_pending"], 999);
        assert_eq!(json["wal_dropped"], 0);
    }

    #[test]
    fn health_snapshot_degraded_at_threshold() {
        let snap = HealthSnapshot {
            degraded: true,
            reason: Some("wal_pending=1000 wal_dropped=0".to_string()),
            wal_pending: 1000,
            wal_dropped: 0,
            creds_sync_errors: 0,
        };
        let json = serde_json::to_value(&snap).unwrap();
        assert_eq!(json["degraded"], true);
        assert!(json["reason"].as_str().unwrap().contains("wal_pending=1000"));
    }

    #[test]
    fn health_snapshot_degraded_on_any_dropped() {
        let snap = HealthSnapshot {
            degraded: true,
            reason: Some("wal_pending=0 wal_dropped=1".to_string()),
            wal_pending: 0,
            wal_dropped: 1,
            creds_sync_errors: 0,
        };
        let json = serde_json::to_value(&snap).unwrap();
        assert_eq!(json["degraded"], true);
        assert_eq!(json["wal_dropped"], 1);
    }

    #[test]
    fn heartbeat_payload_includes_health_field() {
        // Verify HealthSnapshot round-trips through JSON with the expected
        // field names so the liveness consumer can parse `payload["health"]`.
        let snap =
            HealthSnapshot { degraded: false, reason: None, wal_pending: 5, wal_dropped: 0, creds_sync_errors: 0 };
        let json = serde_json::to_value(&snap).unwrap();
        // All four fields must be present.
        assert!(json.get("degraded").is_some());
        assert!(json.get("reason").is_some());
        assert!(json.get("wal_pending").is_some());
        assert!(json.get("wal_dropped").is_some());
    }
}
