//! Shared project-clone outbox/queue protocol. Kept in `agentforge_core` so the
//! API create path (which writes the transactional-outbox row) and the jobs
//! outbox publisher (which relays it into `job_queue`) cannot drift on the
//! `aggregate_type` / `event_type` discriminators, the queue name, the
//! idempotency-key format, or the JSON payload shape.
//!
//! The API crate cannot be a dependency of the jobs crate (that would be a
//! cycle), so these contracts live in `core`, which both crates already depend
//! on — the same arrangement as `orchestration_protocol` and
//! `credential_protocol`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The `orchestration_outbox.aggregate_type` discriminator for a project-clone
/// row, so the publisher can tell clone rows apart from assignment rows.
pub const CLONE_OUTBOX_AGGREGATE_TYPE: &str = "project_clone";

/// The `orchestration_outbox.event_type` discriminator for a clone-requested row.
pub const CLONE_OUTBOX_EVENT_TYPE: &str = "clone_requested";

/// The `job_queue.queue` name a relayed clone job lands on. The `project_clone`
/// worker (M5) dequeues from this queue.
pub const CLONE_JOB_QUEUE: &str = "project_clone";

/// Default `job_queue.max_attempts` for a relayed clone job. The clone worker
/// owns its own bounded-retry-as-new-attempt policy; this bounds the *transport*
/// retries of the queue row itself.
pub const CLONE_JOB_MAX_ATTEMPTS: i32 = 5;

/// The JSON payload of a `project_clone` transactional-outbox row (and of the
/// relayed `job_queue` job).
///
/// Deliberately minimal: just the identifiers (the worker re-reads the
/// authoritative `project_clone_attempts` row by `(project_id, attempt)` rather
/// than trusting a snapshot embedded here), plus an OPTIONAL `run_after` the relay
/// honors so a retry is genuinely delayed by its computed backoff instead of
/// re-running instantly. `run_after` is skipped when absent so first-attempt rows
/// stay byte-identical to the pre-backoff shape and the worker (which ignores it)
/// is unaffected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneOutboxPayload {
    pub project_id: Uuid,
    pub attempt: i32,
    /// Earliest wall-clock time the relay may enqueue this job (the retry backoff,
    /// computed from the attempt number). `None` ⇒ enqueue immediately. The relay
    /// maps this onto `job_queue.run_at`; the worker ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_after: Option<DateTime<Utc>>,
}

impl CloneOutboxPayload {
    /// A first-delivery payload (no backoff): enqueue as soon as it is relayed.
    pub fn now(project_id: Uuid, attempt: i32) -> Self {
        Self { project_id, attempt, run_after: None }
    }

    /// A delayed retry payload: the relay holds the job until `run_after`.
    pub fn delayed(project_id: Uuid, attempt: i32, run_after: DateTime<Utc>) -> Self {
        Self { project_id, attempt, run_after: Some(run_after) }
    }

    /// The idempotency key for the relayed job:
    /// `project_clone:<project_id>:<attempt>`. A retry (a new attempt) produces a
    /// distinct key; a duplicate publish of the same attempt is a no-op against
    /// the `idx_job_queue_unique_key` partial unique index.
    pub fn job_unique_key(&self) -> String {
        format!("{CLONE_OUTBOX_AGGREGATE_TYPE}:{}:{}", self.project_id, self.attempt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_key_is_attempt_scoped() {
        let project_id = Uuid::nil();
        let p1 = CloneOutboxPayload::now(project_id, 1);
        let p2 = CloneOutboxPayload::now(project_id, 2);
        assert_eq!(p1.job_unique_key(), format!("project_clone:{project_id}:1"));
        assert_ne!(p1.job_unique_key(), p2.job_unique_key());
    }

    #[test]
    fn unique_key_ignores_run_after() {
        // The backoff must not change the dedup identity: a delayed retry and an
        // immediate one for the SAME attempt share a unique_key (they are the same
        // job, just scheduled differently).
        let project_id = Uuid::nil();
        let immediate = CloneOutboxPayload::now(project_id, 2);
        let delayed = CloneOutboxPayload::delayed(project_id, 2, Utc::now());
        assert_eq!(immediate.job_unique_key(), delayed.job_unique_key());
    }

    #[test]
    fn payload_roundtrips_json() {
        let payload = CloneOutboxPayload::now(Uuid::new_v4(), 3);
        let json = serde_json::to_value(&payload).expect("serialize");
        // A no-backoff payload omits run_after entirely (byte-compatible with the
        // pre-backoff shape).
        assert!(json.get("run_after").is_none(), "run_after must be omitted when None");
        let back: CloneOutboxPayload = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, payload);
    }

    #[test]
    fn delayed_payload_roundtrips_run_after() {
        let when = Utc::now();
        let payload = CloneOutboxPayload::delayed(Uuid::new_v4(), 4, when);
        let json = serde_json::to_value(&payload).expect("serialize");
        assert!(json.get("run_after").is_some(), "run_after must serialize when set");
        let back: CloneOutboxPayload = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, payload);
        assert_eq!(back.run_after, Some(when));
    }
}
