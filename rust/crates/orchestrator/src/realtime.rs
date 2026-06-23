use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::auth::{self, AuthContext};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "orgId", default, skip_serializing_if = "String::is_empty")]
    pub org_id: String,
    pub payload: Value,
}

struct Client {
    org_id: String,
    tx: mpsc::Sender<Event>,
}

/// The NATS subject a `Broadcaster` event is relayed on. Mirrors VERBATIM the
/// `broadcast.{org_uuid}` subject the main API/jobs publishers use and the main
/// API's `/ws` forwarder relays to browser clients, so a relayed orchestrator
/// event reaches the existing browser socket with no main-API change.
fn broadcast_subject(org_id: &str) -> String {
    format!("broadcast.{org_id}")
}

pub struct Broadcaster {
    next_id: AtomicU64,
    clients: Mutex<HashMap<String, Client>>,
    /// Optional NATS relay. When `Some`, every broadcast is ALSO fire-and-forget
    /// published to `broadcast.{org_id}` so the main API forwards it to browsers
    /// (the orchestrator's own `/ws/events` is loopback-only / not browser
    /// reachable). `None` → in-process-only, today's behavior, no NATS
    /// dependency. `async_nats::Client` is an `Arc` handle — cheap to clone.
    nats: Option<async_nats::Client>,
}

impl Broadcaster {
    pub fn new() -> Self {
        Self { next_id: AtomicU64::new(1), clients: Mutex::new(HashMap::new()), nats: None }
    }

    /// Build a `Broadcaster` that also relays every event to NATS
    /// `broadcast.{org_id}` (best-effort) in addition to its in-process fan-out.
    pub fn with_nats(client: async_nats::Client) -> Self {
        Self { next_id: AtomicU64::new(1), clients: Mutex::new(HashMap::new()), nats: Some(client) }
    }

    /// Lock the client registry, recovering from a poisoned mutex instead of
    /// propagating the panic. A WS handler that panics while holding this lock
    /// must not cascade into every other caller — in particular the detached
    /// `review_escalation_reaper` calls `broadcast` from a `tokio::spawn`, where
    /// (as with the workflow activities) a panic would silently kill the task forever
    /// with no "loop exited" log. A torn client registry degrades to a dropped
    /// notification (best-effort already), never a process-wide cascade.
    fn clients(&self) -> std::sync::MutexGuard<'_, HashMap<String, Client>> {
        self.clients.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn subscribe(&self, org_id: &str) -> (String, mpsc::Receiver<Event>) {
        let client_id = format!("ws-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = mpsc::channel(64);
        self.clients().insert(client_id.clone(), Client { org_id: org_id.to_string(), tx });
        (client_id, rx)
    }

    pub fn unsubscribe(&self, client_id: &str) {
        self.clients().remove(client_id);
    }

    pub fn broadcast(&self, event: Event) {
        {
            let clients = self.clients();
            for client in clients.values() {
                if client.org_id == event.org_id {
                    // Best-effort fan-out. A closed channel is a normal disconnect
                    // (the handler loop ended and unsubscribe is pending) — silent.
                    // A FULL channel is a slow consumer dropping a real notification,
                    // which is worth a debug breadcrumb so the gap is observable.
                    if let Err(mpsc::error::TrySendError::Full(_)) = client.tx.try_send(event.clone()) {
                        tracing::debug!(
                            org_id = %event.org_id,
                            event_kind = %event.kind,
                            "realtime broadcast dropped: client channel full (slow consumer)"
                        );
                    }
                }
            }
        }

        self.relay_to_nats(&event);
    }

    /// Relay one event to NATS so the main API forwards it to browsers. Skipped
    /// when no NATS client is configured or the event has no org (a NATS subject
    /// is org-scoped and an empty `org_id` would broadcast to no one / a bogus
    /// subject). The publish is fire-and-forget on a detached task: `broadcast`
    /// is a synchronous method called from the WS fan-out AND the detached
    /// reapers, so it MUST NOT await NATS. A publish failure only logs.
    fn relay_to_nats(&self, event: &Event) {
        let Some(nats) = &self.nats else {
            return;
        };
        if event.org_id.is_empty() {
            return;
        }

        let bytes = match serde_json::to_vec(event) {
            Ok(bytes) => bytes,
            Err(err) => {
                // A serialize failure of our own `Event` is a defect, not a
                // transient gap — surface it at default verbosity. ids/error
                // only, never the payload.
                tracing::warn!(error = %err, event_kind = %event.kind, "orchestrator realtime NATS relay serialize failed");
                return;
            }
        };
        let subject = broadcast_subject(&event.org_id);
        let client = nats.clone();
        let event_kind = event.kind.clone();
        tokio::spawn(async move {
            if let Err(err) = client.publish(subject, bytes.into()).await {
                // Actionable: browser clients will miss this event. Surface at
                // default verbosity (ids/error only, never the payload). Still
                // best-effort — no panic, no propagation.
                tracing::warn!(error = %err, event_kind = %event_kind, "orchestrator realtime NATS relay publish failed");
            }
        });
    }

    pub fn client_count(&self) -> usize {
        self.clients().len()
    }
}

impl Default for Broadcaster {
    fn default() -> Self {
        Self::new()
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/events", get(ws_events))
}

async fn ws_events(State(state): State<AppState>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    match auth::require_api_auth(&state, &headers) {
        Err(response) => response,
        Ok(AuthContext::Session(claims)) if !claims.org_id.is_empty() => {
            let broadcaster = state.broadcaster.clone();
            let org_id = claims.org_id;
            ws.on_upgrade(move |socket| handle_ws(socket, broadcaster, org_id)).into_response()
        }
        Ok(_) => ws.on_upgrade(close_missing_org).into_response(),
    }
}

async fn close_missing_org(mut socket: WebSocket) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame { code: close_code::POLICY, reason: "authentication required".into() })))
        .await;
}

async fn handle_ws(socket: WebSocket, broadcaster: Arc<Broadcaster>, org_id: String) {
    let (client_id, mut rx) = broadcaster.subscribe(&org_id);
    let (mut sender, mut receiver) = socket.split();

    loop {
        tokio::select! {
            maybe_event = rx.recv() => match maybe_event {
                Some(event) => {
                    match serde_json::to_string(&event) {
                        Ok(message) => {
                            if sender.send(Message::Text(message.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                None => break,
            },
            maybe_message = receiver.next() => match maybe_message {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(Message::Ping(data))) => {
                    if sender.send(Message::Pong(data)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(_)) => break,
            }
        }
    }

    broadcaster.unsubscribe(&client_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn relay_subject_matches_main_api_broadcast_format() {
        // Must equal the `broadcast.{org_uuid}` subject the jobs/api publishers
        // use and the main API's /ws forwarder relays verbatim — otherwise a
        // relayed orchestrator event never reaches the browser socket.
        let org = "11111111-2222-3333-4444-555555555555";
        assert_eq!(broadcast_subject(org), "broadcast.11111111-2222-3333-4444-555555555555");
    }

    #[test]
    fn relay_to_nats_without_client_is_noop_and_does_not_panic() {
        // The default (no-NATS) Broadcaster must relay-to-nothing without
        // panicking or requiring a tokio runtime: the in-process path must work
        // exactly as before with no NATS configured.
        let broadcaster = Broadcaster::new();
        broadcaster.relay_to_nats(&Event {
            kind: "workflow:status".to_string(),
            org_id: "org-1".to_string(),
            payload: json!({ "status": "completed" }),
        });
    }

    #[test]
    fn relayed_event_json_wire_contract_matches_browser_expectations() {
        // The relay publishes `serde_json::to_vec(event)` across the NATS
        // boundary and the browser parses it. Lock the exact JSON shape the FE
        // depends on: `kind` -> `type`, `org_id` -> `orgId`, and a `payload`
        // object. A serde rename regression here would silently break every
        // browser client while passing all behavioral tests.
        let event = Event {
            kind: "review.escalated".to_string(),
            org_id: "11111111-2222-3333-4444-555555555555".to_string(),
            payload: json!({ "reviewId": "r-1", "taskId": "t-9" }),
        };

        // `to_vec` is the EXACT serialization the relay publishes; parse it back
        // to assert the on-the-wire keys/values rather than the in-memory struct.
        let bytes = serde_json::to_vec(&event).expect("serialize event");
        let value: Value = serde_json::from_slice(&bytes).expect("parse relayed json");

        let obj = value.as_object().expect("event serializes to a json object");
        assert_eq!(
            obj.get("type").and_then(Value::as_str),
            Some("review.escalated"),
            "`kind` must serialize as `type`"
        );
        assert_eq!(
            obj.get("orgId").and_then(Value::as_str),
            Some("11111111-2222-3333-4444-555555555555"),
            "`org_id` must serialize as `orgId`"
        );
        assert_eq!(
            obj.get("payload"),
            Some(&json!({ "reviewId": "r-1", "taskId": "t-9" })),
            "`payload` passes through verbatim"
        );
        // No snake_case keys must leak across the boundary.
        assert!(obj.get("kind").is_none(), "`kind` must not appear under its rust name");
        assert!(obj.get("org_id").is_none(), "`org_id` must not appear under its rust name");
    }

    #[tokio::test]
    async fn broadcast_delivers_in_process_with_no_nats_configured() {
        let broadcaster = Broadcaster::new();
        let (_client_id, mut rx) = broadcaster.subscribe("org-1");
        broadcaster.broadcast(Event {
            kind: "review.escalated".to_string(),
            org_id: "org-1".to_string(),
            payload: json!({ "reviewId": "r-1" }),
        });
        let event = rx.try_recv().expect("in-process event delivered");
        assert_eq!(event.kind, "review.escalated");
        assert_eq!(event.org_id, "org-1");
    }
}
