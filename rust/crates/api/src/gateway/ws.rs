//! WebSocket handler with NATS-backed event broadcast.
//!
//! Clients connect to `/ws?token=<jwt>`. The JWT is verified, and the
//! connection subscribes to the tenant broadcast root plus active scoped
//! subjects on NATS. Messages from NATS are forwarded to the WebSocket client
//! in real time.
//!
//! If NATS is unavailable, the WebSocket still upgrades but sends a warning
//! message indicating real-time updates are not available.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use bollard::query_parameters::{AttachContainerOptions, ResizeContainerTTYOptions};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use uuid::Uuid;

use agentforge_core::{AppError, OrgId, ProjectId, TeamId, TenantScope, UserId, WorkspaceId};
use agentforge_platform::DockerClient;

use crate::domain::gateway::{
    GatewayTerminalAttachTarget, WebSocketOriginPolicy, WebSocketOriginRejection, admin_subscription_subjects,
    docker_unavailable_message, parse_gateway_client_message, realtime_disconnected_frame, realtime_unavailable_frame,
    subscription_subjects, terminal_error_frame, terminal_output_frame, terminal_payload_agent_id,
    terminal_payload_dimension, websocket_unauthorized_error,
};
use crate::health::AppState;

/// Query parameters for the WebSocket upgrade request.
#[derive(Debug, Deserialize)]
pub struct WsQuery {
    /// JWT token (WebSocket cannot use Authorization header).
    pub token: String,
}

struct TerminalSession {
    container_id: String,
    input_tx: mpsc::UnboundedSender<Vec<u8>>,
    task: JoinHandle<()>,
}

type OutboundTx = mpsc::UnboundedSender<String>;

/// `GET /ws?token=<jwt>` — upgrade to WebSocket with JWT authentication.
///
/// Verifies the JWT from the query parameter, extracts the org_id for
/// tenant-scoped NATS subscription, and upgrades the connection.
pub async fn ws_handler(
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    let origin = headers.get(axum::http::header::ORIGIN).and_then(|value| value.to_str().ok());
    if let Err(rejection) =
        WebSocketOriginPolicy::validate(origin, state.config.cors_origin.as_deref(), state.config.is_production())
    {
        match &rejection {
            WebSocketOriginRejection::Disallowed(origin) => {
                tracing::warn!(origin, "WebSocket origin rejected");
            }
            WebSocketOriginRejection::MissingInProduction => {
                tracing::warn!("WebSocket missing Origin header in production");
            }
        }
        return Err(rejection.into_app_error());
    }

    // Verify JWT from query param
    let claims = state.jwt.verify_token(&query.token).map_err(|_| websocket_unauthorized_error())?;

    let scope = TenantScope::with_axes(
        OrgId::from(claims.org),
        UserId::from(claims.sub),
        claims.workspace_id.map(WorkspaceId::from),
        claims.team_id.map(TeamId::from),
        claims.project_id.map(ProjectId::from),
    );

    // Role drives the audience-scoped admin subscriptions (e.g. the CLI image
    // toast). Captured here from the verified JWT, never from client input.
    let role = claims.role;

    // Upgrade connection — authentication is done, hand off to async handler
    Ok(ws.on_upgrade(move |socket| handle_ws(socket, state, scope, role)))
}

/// Handle an established WebSocket connection.
///
/// Subscribes to `broadcast.{org_id}` on NATS and forwards messages
/// to the client. Handles ping/pong and graceful disconnect.
async fn handle_ws(socket: WebSocket, state: AppState, scope: TenantScope, role: String) {
    let org_id = scope.org_id().as_uuid();
    tracing::info!(org_id = %org_id, "WebSocket connected");

    let (mut ws_tx, mut ws_rx) = socket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<String>();
    let nats_tasks = spawn_nats_forwarders(&state, &scope, &role, outbound_tx.clone());
    let mut terminals: HashMap<Uuid, TerminalSession> = HashMap::new();

    loop {
        tokio::select! {
            outbound = outbound_rx.recv() => {
                let Some(text) = outbound else {
                    break;
                };
                if ws_tx.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        let pong_sent = ws_tx.send(Message::Pong(data)).await;
                        if pong_sent.is_err() {
                            tracing::debug!(org_id = %org_id, "Pong send failed, connection dead");
                            break;
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        handle_client_message(&state, &scope, &outbound_tx, &mut terminals, &text).await;
                    }
                    Some(Err(e)) => {
                        tracing::debug!(error = %e, org_id = %org_id, "WebSocket receive error");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    for task in nats_tasks {
        task.abort();
    }
    for (_, session) in terminals {
        session.task.abort();
    }

    tracing::info!(org_id = %org_id, "WebSocket disconnected");
}

fn spawn_nats_forwarders(
    state: &AppState,
    scope: &TenantScope,
    role: &str,
    outbound_tx: OutboundTx,
) -> Vec<JoinHandle<()>> {
    let Some(client) = state.nats.client().cloned() else {
        let _ = outbound_tx.send(realtime_unavailable_frame());
        return Vec::new();
    };

    subscription_subjects(scope)
        .into_iter()
        .chain(admin_subscription_subjects(role))
        .map(|subject| {
            let client = client.clone();
            let outbound_tx = outbound_tx.clone();
            let org_id = scope.org_id().as_uuid();
            tokio::spawn(async move {
                match client.subscribe(subject.clone()).await {
                    Ok(mut subscriber) => {
                        while let Some(nats_msg) = subscriber.next().await {
                            let text = String::from_utf8_lossy(&nats_msg.payload).into_owned();
                            if outbound_tx.send(text).is_err() {
                                break;
                            }
                        }
                        tracing::warn!(org_id = %org_id, subject, "NATS subscription ended unexpectedly");
                        let _ = outbound_tx.send(realtime_disconnected_frame());
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, subject, "Failed to subscribe to NATS");
                        let _ = outbound_tx.send(realtime_unavailable_frame());
                    }
                }
            })
        })
        .collect()
}

async fn handle_client_message(
    state: &AppState,
    scope: &TenantScope,
    outbound_tx: &OutboundTx,
    terminals: &mut HashMap<Uuid, TerminalSession>,
    text: &str,
) {
    let Some(msg) = parse_gateway_client_message(text) else {
        return;
    };

    match msg.kind.as_str() {
        "terminal_attach" => attach_terminal(state, scope, outbound_tx, terminals, &msg.payload).await,
        "terminal_data" => write_terminal_data(outbound_tx, terminals, &msg.payload),
        "terminal_input" => write_terminal_keys(outbound_tx, terminals, &msg.payload),
        "terminal_resize" => resize_terminal(state, outbound_tx, terminals, &msg.payload).await,
        "terminal_detach" => detach_terminal(terminals, &msg.payload),
        _ => {}
    }
}

async fn attach_terminal(
    state: &AppState,
    scope: &TenantScope,
    outbound_tx: &OutboundTx,
    terminals: &mut HashMap<Uuid, TerminalSession>,
    payload: &Value,
) {
    let Some(agent_id) = terminal_payload_agent_id(payload) else {
        return;
    };
    detach_terminal_by_id(terminals, agent_id);

    let Some(docker) = state.docker.clone() else {
        tracing::warn!(agent_id = %agent_id, "terminal attach rejected: docker unavailable");
        let _ = outbound_tx.send(terminal_error_frame(agent_id, docker_unavailable_message()));
        return;
    };

    let target = state.gateway_terminal_service().attach_target(scope, agent_id).await;
    let container_id = match target {
        GatewayTerminalAttachTarget::Ready { container_id } => container_id,
        GatewayTerminalAttachTarget::Rejected { message } => {
            tracing::warn!(agent_id = %agent_id, reason = %message, "terminal attach rejected");
            let _ = outbound_tx.send(terminal_error_frame(agent_id, message));
            return;
        }
    };

    let cols = terminal_payload_dimension(payload, "cols").unwrap_or(80).max(1);
    let rows = terminal_payload_dimension(payload, "rows").unwrap_or(24).max(1);
    let (input_tx, input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let task = tokio::spawn(run_terminal_attach(
        docker,
        agent_id,
        container_id.clone(),
        cols,
        rows,
        input_rx,
        outbound_tx.clone(),
    ));

    terminals.insert(agent_id, TerminalSession { container_id, input_tx, task });
}

fn write_terminal_data(outbound_tx: &OutboundTx, terminals: &HashMap<Uuid, TerminalSession>, payload: &Value) {
    let Some(agent_id) = terminal_payload_agent_id(payload) else {
        return;
    };
    let Some(data) = payload.get("data").and_then(Value::as_str) else {
        return;
    };
    write_terminal_bytes(outbound_tx, terminals, agent_id, data.as_bytes().to_vec());
}

fn write_terminal_keys(outbound_tx: &OutboundTx, terminals: &HashMap<Uuid, TerminalSession>, payload: &Value) {
    let Some(agent_id) = terminal_payload_agent_id(payload) else {
        return;
    };
    let Some(keys) = payload.get("keys").and_then(Value::as_array) else {
        return;
    };
    let data = keys.iter().filter_map(Value::as_str).collect::<String>();
    if !data.is_empty() {
        write_terminal_bytes(outbound_tx, terminals, agent_id, data.into_bytes());
    }
}

fn write_terminal_bytes(
    outbound_tx: &OutboundTx,
    terminals: &HashMap<Uuid, TerminalSession>,
    agent_id: Uuid,
    bytes: Vec<u8>,
) {
    match terminals.get(&agent_id) {
        Some(session) if session.input_tx.send(bytes).is_err() => {
            let _ = outbound_tx.send(terminal_error_frame(agent_id, "terminal input stream is closed"));
        }
        Some(_) => {}
        None => {
            let _ = outbound_tx.send(terminal_error_frame(agent_id, "terminal is not attached"));
        }
    }
}

async fn resize_terminal(
    state: &AppState,
    outbound_tx: &OutboundTx,
    terminals: &HashMap<Uuid, TerminalSession>,
    payload: &Value,
) {
    let Some(agent_id) = terminal_payload_agent_id(payload) else {
        return;
    };
    let Some(session) = terminals.get(&agent_id) else {
        return;
    };
    let Some(docker) = state.docker.clone() else {
        let _ = outbound_tx.send(terminal_error_frame(agent_id, docker_unavailable_message()));
        return;
    };
    let cols = terminal_payload_dimension(payload, "cols").unwrap_or(80).max(1);
    let rows = terminal_payload_dimension(payload, "rows").unwrap_or(24).max(1);
    if let Err(err) = resize_container_tty(&docker, &session.container_id, cols, rows).await {
        tracing::debug!(error = %err, agent_id = %agent_id, "failed to resize terminal");
    }
}

fn detach_terminal(terminals: &mut HashMap<Uuid, TerminalSession>, payload: &Value) {
    if let Some(agent_id) = terminal_payload_agent_id(payload) {
        detach_terminal_by_id(terminals, agent_id);
    }
}

fn detach_terminal_by_id(terminals: &mut HashMap<Uuid, TerminalSession>, agent_id: Uuid) {
    if let Some(session) = terminals.remove(&agent_id) {
        session.task.abort();
    }
}

async fn run_terminal_attach(
    docker: Arc<DockerClient>,
    agent_id: Uuid,
    container_id: String,
    cols: u16,
    rows: u16,
    mut input_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    outbound_tx: OutboundTx,
) {
    if let Err(err) = resize_container_tty(&docker, &container_id, cols, rows).await {
        tracing::debug!(error = %err, agent_id = %agent_id, "initial terminal resize failed");
    }

    let attached = docker
        .inner()
        .attach_container(
            &container_id,
            Some(AttachContainerOptions {
                stdin: true,
                stdout: true,
                stderr: true,
                stream: true,
                logs: true,
                detach_keys: None,
            }),
        )
        .await;

    let attached = match attached {
        Ok(attached) => attached,
        Err(err) => {
            tracing::warn!(agent_id = %agent_id, container_id = %container_id, error = %err, "terminal attach failed");
            let _ = outbound_tx.send(terminal_error_frame(agent_id, format!("terminal attach failed: {err}")));
            return;
        }
    };

    let mut output = attached.output;
    let mut input = attached.input;

    loop {
        tokio::select! {
            chunk = input_rx.recv() => {
                let Some(chunk) = chunk else {
                    break;
                };
                if let Err(err) = input.write_all(&chunk).await {
                    tracing::warn!(agent_id = %agent_id, error = %err, "terminal input failed");
                    let _ = outbound_tx.send(terminal_error_frame(agent_id, format!("terminal input failed: {err}")));
                    break;
                }
            }
            chunk = output.next() => {
                match chunk {
                    Some(Ok(output)) => {
                        if outbound_tx.send(terminal_output_frame(agent_id, output.as_ref())).is_err() {
                            break;
                        }
                    }
                    Some(Err(err)) => {
                        tracing::warn!(agent_id = %agent_id, error = %err, "terminal output failed");
                        let _ = outbound_tx.send(terminal_error_frame(agent_id, format!("terminal output failed: {err}")));
                        break;
                    }
                    None => break,
                }
            }
        }
    }
}

async fn resize_container_tty(
    docker: &DockerClient,
    container_id: &str,
    cols: u16,
    rows: u16,
) -> Result<(), bollard::errors::Error> {
    docker
        .inner()
        .resize_container_tty(container_id, ResizeContainerTTYOptions { h: rows as i32, w: cols as i32 })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_query_deserializes() {
        let query: WsQuery = serde_json::from_str(r#"{"token":"eyJhbGciOiJIUzI1NiJ9.test.sig"}"#).unwrap();
        assert_eq!(query.token, "eyJhbGciOiJIUzI1NiJ9.test.sig");
    }

    #[test]
    fn ws_query_missing_token_fails() {
        let result = serde_json::from_str::<WsQuery>(r#"{}"#);
        assert!(result.is_err());
    }

    #[test]
    fn ws_query_empty_token_deserializes() {
        let query: WsQuery = serde_json::from_str(r#"{"token":""}"#).unwrap();
        assert!(query.token.is_empty());
    }
}
