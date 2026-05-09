//! Command subscription via NATS request-reply pattern.

use async_nats::Client;
use futures::StreamExt;

/// Handles incoming commands from the platform addressed to this sidecar.
pub struct CommandHandler {
    client: Client,
    agent_id: String,
}

impl CommandHandler {
    pub fn new(client: Client, agent_id: String) -> Self {
        Self { client, agent_id }
    }

    /// Subscribe to `sidecar.<agent_id>.cmd` and dispatch commands until shutdown.
    pub async fn run(&self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let subject = format!("sidecar.{}.cmd", self.agent_id);

        let mut subscriber = match self.client.subscribe(subject.clone()).await {
            Ok(sub) => sub,
            Err(err) => {
                tracing::error!(error = %err, "Failed to subscribe to commands");
                return;
            }
        };

        tracing::info!(subject = %subject, "Command handler listening");

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("Command handler shutting down");
                        break;
                    }
                }
                msg = subscriber.next() => {
                    match msg {
                        Some(nats_msg) => {
                            let reply = Self::handle_command(&self.agent_id, &nats_msg.payload);
                            if let Some(reply_to) = nats_msg.reply
                                && let Err(err) = self.client.publish(reply_to, reply.into()).await
                            {
                                tracing::warn!(error = %err, "Failed to send command reply");
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    }

    /// Process a single command payload and return the response bytes.
    ///
    /// Public for testing.
    pub fn handle_command(agent_id: &str, payload: &[u8]) -> Vec<u8> {
        let cmd: serde_json::Value =
            serde_json::from_slice(payload).unwrap_or(serde_json::json!({"error": "invalid JSON"}));

        let cmd_type = cmd["type"].as_str().unwrap_or("unknown");
        tracing::debug!(command = cmd_type, "Handling command");

        let response = match cmd_type {
            "ping" => serde_json::json!({"ok": true, "pong": true}),
            "status" => {
                serde_json::json!({"ok": true, "agent_id": agent_id, "status": "running"})
            }
            "prompt" => serde_json::json!({"ok": true, "accepted": true, "type": "prompt"}),
            "interrupt" => serde_json::json!({"ok": true, "accepted": true, "type": "interrupt"}),
            _ => serde_json::json!({"ok": false, "error": format!("unknown command: {}", cmd_type)}),
        };

        serde_json::to_vec(&response).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_ping() {
        let payload = serde_json::to_vec(&serde_json::json!({"type": "ping"})).unwrap();
        let resp = CommandHandler::handle_command("agent-1", &payload);
        let v: serde_json::Value = serde_json::from_slice(&resp).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["pong"], true);
    }

    #[test]
    fn test_handle_status() {
        let payload = serde_json::to_vec(&serde_json::json!({"type": "status"})).unwrap();
        let resp = CommandHandler::handle_command("agent-42", &payload);
        let v: serde_json::Value = serde_json::from_slice(&resp).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["agent_id"], "agent-42");
        assert_eq!(v["status"], "running");
    }

    #[test]
    fn test_handle_prompt() {
        let payload = serde_json::to_vec(&serde_json::json!({
            "type": "prompt",
            "prompt": "hello"
        }))
        .unwrap();
        let resp = CommandHandler::handle_command("agent-1", &payload);
        let v: serde_json::Value = serde_json::from_slice(&resp).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["accepted"], true);
        assert_eq!(v["type"], "prompt");
    }

    #[test]
    fn test_handle_interrupt() {
        let payload = serde_json::to_vec(&serde_json::json!({
            "type": "interrupt"
        }))
        .unwrap();
        let resp = CommandHandler::handle_command("agent-1", &payload);
        let v: serde_json::Value = serde_json::from_slice(&resp).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["accepted"], true);
        assert_eq!(v["type"], "interrupt");
    }

    #[test]
    fn test_handle_unknown_command() {
        let payload = serde_json::to_vec(&serde_json::json!({"type": "reboot"})).unwrap();
        let resp = CommandHandler::handle_command("agent-1", &payload);
        let v: serde_json::Value = serde_json::from_slice(&resp).unwrap();
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("unknown command"));
    }

    #[test]
    fn test_handle_invalid_json() {
        let resp = CommandHandler::handle_command("agent-1", b"not json");
        let v: serde_json::Value = serde_json::from_slice(&resp).unwrap();
        // Invalid JSON falls through to the "unknown" command type branch.
        assert_eq!(v["ok"], false);
    }

    #[test]
    fn test_handle_missing_type_field() {
        let payload = serde_json::to_vec(&serde_json::json!({"foo": "bar"})).unwrap();
        let resp = CommandHandler::handle_command("agent-1", &payload);
        let v: serde_json::Value = serde_json::from_slice(&resp).unwrap();
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap().contains("unknown"));
    }
}
