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

pub struct Broadcaster {
    next_id: AtomicU64,
    clients: Mutex<HashMap<String, Client>>,
}

impl Broadcaster {
    pub fn new() -> Self {
        Self { next_id: AtomicU64::new(1), clients: Mutex::new(HashMap::new()) }
    }

    /// Lock the client registry, recovering from a poisoned mutex instead of
    /// propagating the panic. A WS handler that panics while holding this lock
    /// must not cascade into every other caller — in particular the detached
    /// reapers (`dispatch_reaper`, `review_escalation_reaper`) call `broadcast`
    /// from a `tokio::spawn`, where a panic would silently kill the task forever
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
