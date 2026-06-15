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
/// Deliberately minimal: the identifiers the worker needs (it re-reads the
/// authoritative `project_clone_attempts` row by `(project_id, attempt)` rather
/// than trusting a status/URL snapshot embedded here), plus an OPTIONAL
/// `run_after` the relay honors so a retry is genuinely delayed by its computed
/// backoff instead of re-running instantly. `run_after` is skipped when absent so
/// the relayed job's `run_at` is just `now()`.
///
/// `organization_id` (+ `workspace_id`) are carried so the worker can constrain
/// EVERY attempt/project load by tenant BEFORE it trusts the payload's
/// `project_id`/`attempt`: a poisoned outbox/job row that points `project_id` at
/// another org's attempt can never read across the org boundary, because the
/// worker scopes `find_attempt`/`project_dir_name` by this org and a mismatch
/// simply yields "no such attempt" (defense-in-depth on top of the attempt row's
/// own `organization_id` snapshot). They are NOT part of the dedup identity — the
/// `job_unique_key` stays `(project_id, attempt)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneOutboxPayload {
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
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
    pub fn now(organization_id: Uuid, workspace_id: Uuid, project_id: Uuid, attempt: i32) -> Self {
        Self { organization_id, workspace_id, project_id, attempt, run_after: None }
    }

    /// A delayed retry payload: the relay holds the job until `run_after`.
    pub fn delayed(
        organization_id: Uuid,
        workspace_id: Uuid,
        project_id: Uuid,
        attempt: i32,
        run_after: DateTime<Utc>,
    ) -> Self {
        Self { organization_id, workspace_id, project_id, attempt, run_after: Some(run_after) }
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
        let org = Uuid::nil();
        let ws = Uuid::nil();
        let project_id = Uuid::nil();
        let p1 = CloneOutboxPayload::now(org, ws, project_id, 1);
        let p2 = CloneOutboxPayload::now(org, ws, project_id, 2);
        assert_eq!(p1.job_unique_key(), format!("project_clone:{project_id}:1"));
        assert_ne!(p1.job_unique_key(), p2.job_unique_key());
    }

    #[test]
    fn unique_key_ignores_run_after() {
        // The backoff must not change the dedup identity: a delayed retry and an
        // immediate one for the SAME attempt share a unique_key (they are the same
        // job, just scheduled differently).
        let org = Uuid::nil();
        let ws = Uuid::nil();
        let project_id = Uuid::nil();
        let immediate = CloneOutboxPayload::now(org, ws, project_id, 2);
        let delayed = CloneOutboxPayload::delayed(org, ws, project_id, 2, Utc::now());
        assert_eq!(immediate.job_unique_key(), delayed.job_unique_key());
    }

    #[test]
    fn unique_key_ignores_tenant() {
        // The org/workspace are tenant-scoping context for the worker's loads, NOT
        // part of the dedup identity: two payloads with the SAME (project, attempt)
        // but different tenants share a unique_key (a project_id cannot belong to
        // two orgs, so this asserts the key is invariant under the new fields).
        let project_id = Uuid::new_v4();
        let a = CloneOutboxPayload::now(Uuid::new_v4(), Uuid::new_v4(), project_id, 1);
        let b = CloneOutboxPayload::now(Uuid::new_v4(), Uuid::new_v4(), project_id, 1);
        assert_eq!(a.job_unique_key(), b.job_unique_key());
    }

    #[test]
    fn payload_roundtrips_json() {
        let payload = CloneOutboxPayload::now(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), 3);
        let json = serde_json::to_value(&payload).expect("serialize");
        // A no-backoff payload omits run_after entirely.
        assert!(json.get("run_after").is_none(), "run_after must be omitted when None");
        // The tenant context is carried so the worker can scope its loads.
        assert!(json.get("organization_id").is_some(), "organization_id must serialize");
        assert!(json.get("workspace_id").is_some(), "workspace_id must serialize");
        let back: CloneOutboxPayload = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, payload);
    }

    #[test]
    fn delayed_payload_roundtrips_run_after() {
        let when = Utc::now();
        let payload = CloneOutboxPayload::delayed(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), 4, when);
        let json = serde_json::to_value(&payload).expect("serialize");
        assert!(json.get("run_after").is_some(), "run_after must serialize when set");
        let back: CloneOutboxPayload = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, payload);
        assert_eq!(back.run_after, Some(when));
    }
}
