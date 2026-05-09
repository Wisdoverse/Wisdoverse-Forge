//! NATS protocol for orchestration assignment and result delivery.
//!
//! Decouples the API (dispatcher) and sidecar (worker) so neither needs to
//! know the other's deployment topology: API publishes to
//! `orchestration.assigned.<agent_id>`, the sidecar running inside that
//! agent's container subscribes and executes the wrapped CLI, then publishes
//! to `orchestration.result.<agent_id>` which a backend consumer turns into a
//! complete/fail DB update.
//!
//! Messages are HMAC-SHA256 signed with the same `agent_id:timestamp:payload`
//! canonicalization the event pipeline uses, so the sidecar's existing key
//! material and the backend's verifier work for this new channel unchanged.

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use crate::context_envelope::ContextEnvelope;

type HmacSha256 = Hmac<Sha256>;

/// Subject prefix for assignment messages: `orchestration.assigned.<agent_id>`.
pub const ASSIGN_SUBJECT_PREFIX: &str = "orchestration.assigned";

/// Subject prefix for result messages: `orchestration.result.<agent_id>`.
pub const RESULT_SUBJECT_PREFIX: &str = "orchestration.result";

/// JetStream stream used for durable task assignments.
pub const ORCHESTRATION_ASSIGNMENTS_STREAM: &str = "ORCHESTRATION_ASSIGNMENTS";
pub const DEFAULT_ASSIGNMENT_LEASE_SECS: i64 = 900;

/// NATS subject that carries an assignment for the given agent.
pub fn assign_subject(agent_id: Uuid) -> String {
    format!("{ASSIGN_SUBJECT_PREFIX}.{agent_id}")
}

/// Wildcard subject that matches assignments for every agent.
pub fn assign_subject_wildcard() -> String {
    format!("{ASSIGN_SUBJECT_PREFIX}.*")
}

/// NATS subject that carries a completion or failure result for the given agent.
pub fn result_subject(agent_id: Uuid) -> String {
    format!("{RESULT_SUBJECT_PREFIX}.{agent_id}")
}

/// Wildcard subject the backend consumer subscribes to in order to receive
/// results from every agent in every organization.
pub fn result_subject_wildcard() -> String {
    format!("{RESULT_SUBJECT_PREFIX}.*")
}

/// Durable consumer name used by one sidecar to resume its own assignment
/// backlog after restart. Keep format stable because the auth-callout
/// permission template depends on it.
pub fn assignment_consumer_name(agent_id: Uuid) -> String {
    format!("orch-assignment-{}", agent_id.simple())
}

/// Exact JetStream API subject the sidecar publishes when creating or
/// updating its per-agent durable assignment consumer.
pub fn assignment_consumer_create_subject(agent_id: Uuid) -> String {
    format!(
        "$JS.API.CONSUMER.CREATE.{}.{}.{}",
        ORCHESTRATION_ASSIGNMENTS_STREAM,
        assignment_consumer_name(agent_id),
        assign_subject(agent_id)
    )
}

/// Exact JetStream API subject the sidecar publishes when reading durable
/// consumer metadata.
pub fn assignment_consumer_info_subject(agent_id: Uuid) -> String {
    format!("$JS.API.CONSUMER.INFO.{}.{}", ORCHESTRATION_ASSIGNMENTS_STREAM, assignment_consumer_name(agent_id))
}

/// Exact JetStream API subject the sidecar publishes when pulling the next
/// assignment batch for its own durable consumer.
pub fn assignment_consumer_next_subject(agent_id: Uuid) -> String {
    format!("$JS.API.CONSUMER.MSG.NEXT.{}.{}", ORCHESTRATION_ASSIGNMENTS_STREAM, assignment_consumer_name(agent_id))
}

/// Permission pattern for JetStream ACK publishes emitted by the sidecar.
pub fn assignment_consumer_ack_subject_pattern(agent_id: Uuid) -> String {
    format!("$JS.ACK.{}.{}.>", ORCHESTRATION_ASSIGNMENTS_STREAM, assignment_consumer_name(agent_id))
}

/// Parse the trailing agent_id from an `orchestration.assigned.<uuid>` or
/// `orchestration.result.<uuid>` subject. Returns `None` on any shape mismatch.
pub fn parse_agent_id_from_subject(subject: &str, prefix: &str) -> Option<Uuid> {
    let rest = subject.strip_prefix(prefix)?.strip_prefix('.')?;
    Uuid::parse_str(rest).ok()
}

/// Payload delivered from API to sidecar — enough for the sidecar to invoke
/// the wrapped CLI without a DB round-trip.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskAssignment {
    /// Durable assignment id. Kept optional for wire compatibility with old
    /// JSON payloads, but production sidecars reject assignments without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_id: Option<Uuid>,
    /// Task attempt produced by the DB claim transaction. Production sidecars
    /// reject assignments without it so results can be guarded against stale
    /// attempts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt: Option<i32>,
    /// Lease deadline produced by the DB claim transaction. Production
    /// sidecars reject assignments without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub task_id: Uuid,
    pub agent_id: Uuid,
    pub title: String,
    /// User-facing instruction from `params.task`; falls back to title when
    /// the task was created without structured params.
    pub task: String,
    /// Optional prompt body from `params.message`.
    #[serde(default)]
    pub message: String,
    pub priority: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_envelope: Option<ContextEnvelope>,
}

/// Outcome emitted by the sidecar once the wrapped CLI exits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TaskOutcome {
    /// CLI exited with status 0. `stdout` carries the captured output.
    Completed { stdout: String },
    /// CLI exited non-zero or failed to start. `stderr` carries the diagnostic
    /// detail and `exit_code` is `None` when the process never started.
    Failed { stderr: String, exit_code: Option<i32> },
}

/// Payload returned from sidecar to backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskResult {
    /// Durable assignment/result id. Kept optional for wire compatibility with
    /// old JSON payloads, but production result consumers reject missing ids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_id: Option<Uuid>,
    /// Task attempt from the assignment. Production result consumers reject
    /// missing attempts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt: Option<i32>,
    pub task_id: Uuid,
    pub agent_id: Uuid,
    pub outcome: TaskOutcome,
}

/// Signed envelope matching the event pipeline's existing shape
/// (`events.ingest.<agent_id>`): `payload` is any JSON-serializable body,
/// `signature` is hex HMAC-SHA256 of `agent_id:timestamp:payload`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEnvelope {
    pub payload: serde_json::Value,
    pub timestamp: i64,
    pub agent_id: String,
    pub signature: String,
}

impl SignedEnvelope {
    /// Build a signed envelope around an arbitrary payload.
    pub fn sign<T: Serialize>(
        hmac_key: &[u8],
        agent_id: &str,
        timestamp: i64,
        payload: &T,
    ) -> Result<Self, serde_json::Error> {
        let payload_value = serde_json::to_value(payload)?;
        let signature = compute_signature(hmac_key, agent_id, timestamp, &payload_value);
        Ok(Self { payload: payload_value, timestamp, agent_id: agent_id.to_string(), signature })
    }

    /// Verify the envelope signature using `hmac_key`. Returns `true` when the
    /// signature matches. Uses the `hmac` crate's constant-time comparison so
    /// this is safe against timing side-channels even with a weak signer.
    pub fn verify(&self, hmac_key: &[u8]) -> bool {
        let Ok(sig_bytes) = hex::decode(&self.signature) else { return false };
        let sign_data = sign_data(&self.agent_id, self.timestamp, &self.payload);
        let Ok(mut mac) = HmacSha256::new_from_slice(hmac_key) else { return false };
        mac.update(sign_data.as_bytes());
        mac.verify_slice(&sig_bytes).is_ok()
    }
}

fn sign_data(agent_id: &str, timestamp: i64, payload: &serde_json::Value) -> String {
    format!("{agent_id}:{timestamp}:{payload}")
}

fn compute_signature(hmac_key: &[u8], agent_id: &str, timestamp: i64, payload: &serde_json::Value) -> String {
    let mut mac = HmacSha256::new_from_slice(hmac_key).expect("HMAC accepts any key length");
    mac.update(sign_data(agent_id, timestamp, payload).as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subjects_are_formatted_with_agent_id() {
        let id = Uuid::nil();
        assert_eq!(assign_subject(id), format!("orchestration.assigned.{id}"));
        assert_eq!(assign_subject_wildcard(), "orchestration.assigned.*");
        assert_eq!(result_subject(id), format!("orchestration.result.{id}"));
        assert_eq!(result_subject_wildcard(), "orchestration.result.*");
    }

    #[test]
    fn assignment_consumer_subjects_are_stable() {
        let id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        assert_eq!(assignment_consumer_name(id), "orch-assignment-11111111222233334444555555555555");
        assert_eq!(
            assignment_consumer_create_subject(id),
            "$JS.API.CONSUMER.CREATE.ORCHESTRATION_ASSIGNMENTS.orch-assignment-11111111222233334444555555555555.orchestration.assigned.11111111-2222-3333-4444-555555555555"
        );
        assert_eq!(
            assignment_consumer_info_subject(id),
            "$JS.API.CONSUMER.INFO.ORCHESTRATION_ASSIGNMENTS.orch-assignment-11111111222233334444555555555555"
        );
        assert_eq!(
            assignment_consumer_next_subject(id),
            [
                "$JS.API.CONSUMER.M",
                "S",
                "G.NEXT.ORCHESTRATION_ASSIGNMENTS.orch-assignment-11111111222233334444555555555555",
            ]
            .concat()
        );
        assert_eq!(
            assignment_consumer_ack_subject_pattern(id),
            "$JS.ACK.ORCHESTRATION_ASSIGNMENTS.orch-assignment-11111111222233334444555555555555.>"
        );
    }

    #[test]
    fn subject_parser_extracts_agent_id() {
        let id = Uuid::now_v7();
        assert_eq!(parse_agent_id_from_subject(&assign_subject(id), ASSIGN_SUBJECT_PREFIX), Some(id));
        assert_eq!(parse_agent_id_from_subject(&result_subject(id), RESULT_SUBJECT_PREFIX), Some(id));
    }

    #[test]
    fn subject_parser_rejects_other_shapes() {
        assert_eq!(parse_agent_id_from_subject("orchestration.assigned", ASSIGN_SUBJECT_PREFIX), None);
        assert_eq!(parse_agent_id_from_subject("orchestration.assigned.not-a-uuid", ASSIGN_SUBJECT_PREFIX), None);
        assert_eq!(parse_agent_id_from_subject("events.ingest.abc", ASSIGN_SUBJECT_PREFIX), None);
    }

    #[test]
    fn assignment_roundtrips() {
        let msg = TaskAssignment {
            delivery_id: Some(Uuid::now_v7()),
            attempt: Some(1),
            lease_expires_at: Some(Utc::now()),
            task_id: Uuid::now_v7(),
            agent_id: Uuid::now_v7(),
            title: "Sweep inbox".into(),
            task: "Summarise unread threads".into(),
            message: "Focus on the last 7 days".into(),
            priority: "high".into(),
            context_envelope: None,
        };
        let round: TaskAssignment = serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(round, msg);
    }

    #[test]
    fn result_completed_roundtrips() {
        let msg = TaskResult {
            delivery_id: Some(Uuid::now_v7()),
            attempt: Some(1),
            task_id: Uuid::now_v7(),
            agent_id: Uuid::now_v7(),
            outcome: TaskOutcome::Completed { stdout: "done".into() },
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["outcome"]["status"], "completed");
        let round: TaskResult = serde_json::from_value(json).unwrap();
        assert_eq!(round, msg);
    }

    #[test]
    fn result_failed_roundtrips() {
        let msg = TaskResult {
            delivery_id: Some(Uuid::now_v7()),
            attempt: Some(2),
            task_id: Uuid::now_v7(),
            agent_id: Uuid::now_v7(),
            outcome: TaskOutcome::Failed { stderr: "oom".into(), exit_code: Some(137) },
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["outcome"]["status"], "failed");
        assert_eq!(json["outcome"]["exit_code"], 137);
        let round: TaskResult = serde_json::from_value(json).unwrap();
        assert_eq!(round, msg);
    }

    #[test]
    fn signed_envelope_verifies_with_matching_key() {
        let assignment = TaskAssignment {
            delivery_id: Some(Uuid::now_v7()),
            attempt: Some(1),
            lease_expires_at: Some(Utc::now()),
            task_id: Uuid::now_v7(),
            agent_id: Uuid::now_v7(),
            title: "x".into(),
            task: "y".into(),
            message: String::new(),
            priority: "normal".into(),
            context_envelope: None,
        };
        let key = b"shared-secret";
        let env = SignedEnvelope::sign(key, &assignment.agent_id.to_string(), 123, &assignment).unwrap();
        assert!(env.verify(key));
    }

    #[test]
    fn signed_envelope_rejects_wrong_key() {
        let payload = serde_json::json!({ "x": 1 });
        let env = SignedEnvelope::sign(b"good", "agent-1", 0, &payload).unwrap();
        assert!(!env.verify(b"bad"));
    }

    #[test]
    fn signed_envelope_rejects_tampered_payload() {
        let payload = serde_json::json!({ "task": "original" });
        let mut env = SignedEnvelope::sign(b"k", "agent-1", 0, &payload).unwrap();
        env.payload = serde_json::json!({ "task": "swapped" });
        assert!(!env.verify(b"k"));
    }

    #[test]
    fn signed_envelope_rejects_tampered_timestamp() {
        let env = SignedEnvelope::sign(b"k", "agent-1", 100, &serde_json::json!({})).unwrap();
        let mut tampered = env.clone();
        tampered.timestamp = env.timestamp + 1;
        assert!(!tampered.verify(b"k"));
    }

    #[test]
    fn signed_envelope_rejects_malformed_signature() {
        let mut env = SignedEnvelope::sign(b"k", "agent-1", 0, &serde_json::json!({})).unwrap();
        env.signature = "not-hex".into();
        assert!(!env.verify(b"k"));
    }
}
