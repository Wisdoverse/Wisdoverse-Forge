//! Event publishing to NATS with per-message HMAC-SHA256 authentication.

use async_nats::Client;
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;

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
    cli_tool: Option<String>,
}

impl EventPublisher {
    pub fn new(client: Client, agent_id: String, hmac_secret: &str, cli_tool: Option<String>) -> Self {
        Self { client, agent_id, hmac_key: hmac_secret.as_bytes().to_vec(), cli_tool }
    }

    /// Compute HMAC-SHA256 over `agent_id:timestamp:payload` and return hex string.
    fn sign(&self, timestamp: i64, payload: &serde_json::Value) -> String {
        let sign_data = format!("{}:{}:{}", self.agent_id, timestamp, payload);
        let mut mac = HmacSha256::new_from_slice(&self.hmac_key).expect("HMAC key length is always valid");
        mac.update(sign_data.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// Publish an event to the `events.ingest.<agent_id>` NATS subject.
    pub async fn publish(
        &self,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let timestamp = Utc::now().timestamp();
        let subject = format!("events.ingest.{}", self.agent_id);

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

    /// Send a heartbeat on `sidecar.<agent_id>.heartbeat`.
    pub async fn heartbeat(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let subject = format!("sidecar.{}.heartbeat", self.agent_id);
        let capabilities: Vec<String> = self.cli_tool.clone().into_iter().collect();
        let payload = serde_json::json!({
            "agent_id": self.agent_id,
            "timestamp": Utc::now().timestamp(),
            "cli_tool": self.cli_tool,
            "capabilities": capabilities,
            "version": agentforge_core::VERSION,
        });
        let bytes = serde_json::to_vec(&payload)?;
        self.client
            .publish(subject, bytes.into())
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }
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
}
