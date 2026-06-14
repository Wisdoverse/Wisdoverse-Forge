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
/// Deliberately minimal: just the identifiers. The worker re-reads the
/// authoritative `project_clone_attempts` row by `(project_id, attempt)` rather
/// than trusting a snapshot embedded here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneOutboxPayload {
    pub project_id: Uuid,
    pub attempt: i32,
}

impl CloneOutboxPayload {
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
        let p1 = CloneOutboxPayload { project_id, attempt: 1 };
        let p2 = CloneOutboxPayload { project_id, attempt: 2 };
        assert_eq!(p1.job_unique_key(), format!("project_clone:{project_id}:1"));
        assert_ne!(p1.job_unique_key(), p2.job_unique_key());
    }

    #[test]
    fn payload_roundtrips_json() {
        let payload = CloneOutboxPayload { project_id: Uuid::new_v4(), attempt: 3 };
        let json = serde_json::to_value(&payload).expect("serialize");
        let back: CloneOutboxPayload = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, payload);
    }
}
