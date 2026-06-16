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
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use crate::context_envelope::ContextEnvelope;
use crate::runtime_capability::RuntimeKind;

type HmacSha256 = Hmac<Sha256>;

/// Subject prefix for assignment messages: `orchestration.assigned.<agent_id>`.
pub const ASSIGN_SUBJECT_PREFIX: &str = "orchestration.assigned";

/// Subject prefix for result messages: `orchestration.result.<agent_id>`.
pub const RESULT_SUBJECT_PREFIX: &str = "orchestration.result";

/// JetStream stream used for durable task assignments.
pub const ORCHESTRATION_ASSIGNMENTS_STREAM: &str = "ORCHESTRATION_ASSIGNMENTS";
pub const DEFAULT_ASSIGNMENT_LEASE_SECS: i64 = 900;

/// NATS subject that carries an assignment for the given agent in the legacy
/// (pre-#457) un-namespaced shape: `orchestration.assigned.<uuid>`.
pub fn assign_subject(agent_id: Uuid) -> String {
    format!("{ASSIGN_SUBJECT_PREFIX}.{agent_id}")
}

/// #457 phase 1c kind-namespaced assignment subject:
/// `orchestration.assigned.<kind>.<uuid>` (`kind` = the target agent's
/// `runtime_kind`). New sidecars bind their per-agent durable to filter this
/// shape; the platform dual-publishes both shapes during the drain window.
pub fn assign_subject_kind(kind: RuntimeKind, agent_id: Uuid) -> String {
    format!("{ASSIGN_SUBJECT_PREFIX}.{}.{}", kind.as_str(), agent_id)
}

/// Wildcard subject that matches assignments for every agent.
///
/// Multi-token (`.>`) so it captures BOTH the legacy `orchestration.assigned.<uuid>`
/// (3-token) and the future #457 kind-namespaced `orchestration.assigned.<kind>.<uuid>`
/// (4-token) shapes. The assigned producer/consumer/grants stay 3-token in
/// phase 1b; widening only the stream subject now is behaviourally inert (the
/// only assigned publisher still emits 3-token, which `.>` still captures) and
/// de-risks the future assigned PR to a pure grant/parser change.
pub fn assign_subject_wildcard() -> String {
    format!("{ASSIGN_SUBJECT_PREFIX}.>")
}

/// NATS subject that carries a completion or failure result for the given agent
/// in the legacy (pre-#457) un-namespaced shape: `orchestration.result.<uuid>`.
pub fn result_subject(agent_id: Uuid) -> String {
    format!("{RESULT_SUBJECT_PREFIX}.{agent_id}")
}

/// #457 kind-namespaced result subject: `orchestration.result.<kind>.<uuid>`.
/// This is what current sidecars publish; `kind` is the publishing agent's
/// own `runtime_kind`.
pub fn result_subject_kind(kind: RuntimeKind, agent_id: Uuid) -> String {
    format!("{RESULT_SUBJECT_PREFIX}.{}.{}", kind.as_str(), agent_id)
}

/// Wildcard subject the backend consumer subscribes to in order to receive
/// results from every agent in every organization.
///
/// Multi-token (`.>`) so it captures BOTH the legacy `orchestration.result.<uuid>`
/// (3-token) and the #457 namespaced `orchestration.result.<kind>.<uuid>`
/// (4-token) shapes. Used by BOTH the JetStream stream subjects (streams.rs)
/// AND the result consumer's filter_subject, so the two cannot drift.
pub fn result_subject_wildcard() -> String {
    format!("{RESULT_SUBJECT_PREFIX}.>")
}

/// A successfully parsed `orchestration.result` subject (issue #457 phase 1b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedResultSubject {
    /// Trailing agent UUID — the result's owning agent regardless of shape, so
    /// the consumer's subject-vs-envelope/payload cross-checks stay stable.
    pub agent_id: Uuid,
    /// `Some(kind)` for the namespaced shape; `None` for the legacy shape.
    pub runtime_kind: Option<RuntimeKind>,
}

impl ParsedResultSubject {
    /// True for the pre-#457 (un-namespaced) shape; drives the legacy-drain metric.
    pub fn is_legacy(&self) -> bool {
        self.runtime_kind.is_none()
    }
}

/// Parse an `orchestration.result` subject into `(agent_id, runtime_kind?)`,
/// accepting both the legacy 3-token (`orchestration.result.<uuid>`) and the
/// #457 namespaced 4-token (`orchestration.result.<kind>.<uuid>`) shapes.
///
/// Mirrors `event_protocol::parse_events_ingest_subject`: anything else — wrong
/// prefix, unknown kind token, non-UUID tail, wildcards, or extra trailing
/// tokens — returns `None` so the caller rejects it as a forged/unsupported
/// subject rather than guessing an identity.
pub fn parse_result_subject(subject: &str) -> Option<ParsedResultSubject> {
    parse_result_subject_with_prefix(subject, RESULT_SUBJECT_PREFIX)
}

/// Prefix-relative variant of [`parse_result_subject`]. The result consumer is
/// generic over `subject_prefix` (integration tests isolate their JetStream
/// stream under a custom prefix such as `orchestration.result.contract.<id>`),
/// so the legacy-vs-namespaced shape is detected RELATIVE to `prefix`: the
/// remainder after `prefix.` is either `<uuid>` (legacy) or `<kind>.<uuid>`
/// (namespaced).
pub fn parse_result_subject_with_prefix(subject: &str, prefix: &str) -> Option<ParsedResultSubject> {
    let rest = subject.strip_prefix(prefix)?.strip_prefix('.')?;
    let mut tokens = rest.split('.');
    match (tokens.next(), tokens.next(), tokens.next()) {
        // legacy: <prefix>.<uuid>
        (Some(uuid), None, None) => {
            Some(ParsedResultSubject { agent_id: Uuid::parse_str(uuid).ok()?, runtime_kind: None })
        }
        // namespaced: <prefix>.<kind>.<uuid>
        (Some(kind), Some(uuid), None) => Some(ParsedResultSubject {
            agent_id: Uuid::parse_str(uuid).ok()?,
            runtime_kind: Some(RuntimeKind::parse_legacy(kind).ok()?),
        }),
        _ => None,
    }
}

/// Durable consumer name used by one sidecar to resume its own assignment
/// backlog after restart. Keep format stable because the auth-callout
/// permission template depends on it.
pub fn assignment_consumer_name(agent_id: Uuid) -> String {
    format!("orch-assignment-{}", agent_id.simple())
}

/// Exact JetStream API subject the sidecar publishes when creating or
/// updating its per-agent durable assignment consumer (legacy single filter).
pub fn assignment_consumer_create_subject(agent_id: Uuid) -> String {
    format!(
        "$JS.API.CONSUMER.CREATE.{}.{}.{}",
        ORCHESTRATION_ASSIGNMENTS_STREAM,
        assignment_consumer_name(agent_id),
        assign_subject(agent_id)
    )
}

/// #457 phase 1c: the CONSUMER.CREATE API subject for the SAME per-agent durable
/// but bound to the kind-namespaced single filter `orchestration.assigned.<kind>.<uuid>`.
///
/// The filter token is embedded in the API subject ON PURPOSE: it is the
/// security boundary. NATS only lets the sidecar create a consumer whose filter
/// matches the granted subject, so a per-agent grant pins the consumer to the
/// agent's OWN assignment subject. Do NOT replace this with the filter-LESS
/// `$JS.API.CONSUMER.CREATE.<stream>.<durable>` form (the shape required by
/// `filter_subjects` plural) — that was empirically shown to let a rooted
/// sidecar create a consumer under its own durable name but filtering ANOTHER
/// agent's subject, draining that agent's WorkQueue assignments. Single filter
/// only.
pub fn assignment_consumer_create_subject_kind(kind: RuntimeKind, agent_id: Uuid) -> String {
    format!(
        "$JS.API.CONSUMER.CREATE.{}.{}.{}",
        ORCHESTRATION_ASSIGNMENTS_STREAM,
        assignment_consumer_name(agent_id),
        assign_subject_kind(kind, agent_id)
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
    /// #457 phase 1c: the target agent's `runtime_kind`, threaded at ENQUEUE
    /// time (both enqueue sites already read the agent row) so the outbox
    /// publisher can build the kind-namespaced assignment subject without an
    /// extra `agents` query on the publish hot path. `None` on outbox rows
    /// written before this change — the publisher falls back to `Container`,
    /// and the legacy subject (still dual-published) covers old sidecars anyway.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_kind: Option<RuntimeKind>,
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
        // #457 phase 1b: wildcards widened to multi-token `.>` so they capture
        // the namespaced 4-token shape (stream subject + consumer filter).
        assert_eq!(assign_subject_wildcard(), "orchestration.assigned.>");
        assert_eq!(result_subject(id), format!("orchestration.result.{id}"));
        assert_eq!(result_subject_wildcard(), "orchestration.result.>");
    }

    #[test]
    fn namespaced_result_subject_uses_kind() {
        let id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        assert_eq!(
            result_subject_kind(RuntimeKind::Cli, id),
            "orchestration.result.cli.11111111-2222-3333-4444-555555555555"
        );
        assert_eq!(
            result_subject_kind(RuntimeKind::Container, id),
            "orchestration.result.container.11111111-2222-3333-4444-555555555555"
        );
    }

    #[test]
    fn parse_result_subject_accepts_legacy_and_namespaced() {
        let id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();

        let legacy = parse_result_subject("orchestration.result.11111111-2222-3333-4444-555555555555").unwrap();
        assert_eq!(legacy.agent_id, id);
        assert_eq!(legacy.runtime_kind, None);
        assert!(legacy.is_legacy());

        for (tok, kind) in [("container", RuntimeKind::Container), ("cli", RuntimeKind::Cli), ("api", RuntimeKind::Api)]
        {
            let parsed =
                parse_result_subject(&format!("orchestration.result.{tok}.11111111-2222-3333-4444-555555555555"))
                    .unwrap();
            assert_eq!(parsed.agent_id, id);
            assert_eq!(parsed.runtime_kind, Some(kind));
            assert!(!parsed.is_legacy());
        }
        // round-trip the namespaced builder through the parser
        let parsed = parse_result_subject(&result_subject_kind(RuntimeKind::Cli, id)).unwrap();
        assert_eq!(parsed.runtime_kind, Some(RuntimeKind::Cli));
    }

    #[test]
    fn parse_result_subject_rejects_bad_shapes() {
        for bad in [
            "orchestration.result.bogus.11111111-2222-3333-4444-555555555555", // unknown kind token
            "orchestration.result.not-a-uuid",
            "orchestration.result.cli.not-a-uuid",
            "orchestration.result.cli", // kind but no uuid
            "orchestration.result.>",
            "orchestration.result.*",
            "orchestration.result.cli.*",
            "orchestration.result.cli.11111111-2222-3333-4444-555555555555.extra",
            "orchestration.assigned.11111111-2222-3333-4444-555555555555", // wrong prefix
        ] {
            assert!(parse_result_subject(bad).is_none(), "should reject {bad}");
        }
    }

    #[test]
    fn namespaced_assign_subject_and_create_subject_are_stable() {
        let id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        assert_eq!(
            assign_subject_kind(RuntimeKind::Cli, id),
            "orchestration.assigned.cli.11111111-2222-3333-4444-555555555555"
        );
        // #457 phase 1c: the namespaced CONSUMER.CREATE grant keeps the durable
        // name AND embeds the 4-token filter as its trailing tokens — the
        // single-filter security form. (If this ever became filter-LESS, i.e.
        // ...CREATE.<stream>.<durable> with no trailing subject, a rooted
        // sidecar could filter another agent's assignments.)
        assert_eq!(
            assignment_consumer_create_subject_kind(RuntimeKind::Cli, id),
            "$JS.API.CONSUMER.CREATE.ORCHESTRATION_ASSIGNMENTS.orch-assignment-11111111222233334444555555555555.orchestration.assigned.cli.11111111-2222-3333-4444-555555555555"
        );
        // The CREATE subject must end with the agent's own uuid (filter present).
        assert!(assignment_consumer_create_subject_kind(RuntimeKind::Cli, id).ends_with(&id.to_string()));
        // INFO/NEXT/ACK embed only the (unchanged) durable name, never the filter.
        assert_eq!(
            assignment_consumer_info_subject(id),
            "$JS.API.CONSUMER.INFO.ORCHESTRATION_ASSIGNMENTS.orch-assignment-11111111222233334444555555555555"
        );
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
            runtime_kind: Some(RuntimeKind::Cli),
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
            runtime_kind: None,
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
