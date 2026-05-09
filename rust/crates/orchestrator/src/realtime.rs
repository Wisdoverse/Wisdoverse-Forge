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

    pub fn subscribe(&self, org_id: &str) -> (String, mpsc::Receiver<Event>) {
        let client_id = format!("ws-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = mpsc::channel(64);
        self.clients
            .lock()
            .expect("websocket client lock poisoned")
            .insert(client_id.clone(), Client { org_id: org_id.to_string(), tx });
        (client_id, rx)
    }

    pub fn unsubscribe(&self, client_id: &str) {
        self.clients.lock().expect("websocket client lock poisoned").remove(client_id);
    }

    pub fn broadcast(&self, event: Event) {
        let clients = self.clients.lock().expect("websocket client lock poisoned");
        for client in clients.values() {
            if client.org_id == event.org_id {
                let _ = client.tx.try_send(event.clone());
            }
        }
    }

    pub fn client_count(&self) -> usize {
        self.clients.lock().expect("websocket client lock poisoned").len()
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
