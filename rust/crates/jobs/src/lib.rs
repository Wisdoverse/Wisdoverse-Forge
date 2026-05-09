//! Wisdoverse Forge Jobs — PostgreSQL-backed job queue using `FOR UPDATE SKIP LOCKED`.
//!
//! Provides a reliable, transactional job queue for background task processing
//! including cleanup, email, webhook delivery, and scheduled cron jobs.
//!
//! # Architecture
//!
//! - **Queue operations** (`queue` module): enqueue, dequeue, complete, fail, stale lock release
//! - **Worker** (`worker` module): background loop with 1s polling fallback and `pg_notify` wake-up
//!
//! # Concurrency Model
//!
//! Multiple workers can safely process jobs from the same queue. The `FOR UPDATE SKIP LOCKED`
//! pattern ensures each job is claimed by exactly one worker without blocking other workers.
//!
//! # Retry Strategy
//!
//! Failed jobs are retried with exponential backoff (2^attempts seconds). After `max_attempts`
//! failures, the job moves to `dead` status for manual inspection.
//!
//! # Required Migration
//!
//! The `job_queue` table is created by the `001_init.sql` migration in `agentforge-db`.
//! A partial unique index for `unique_key` deduplication should be added:
//!
//! ```sql
//! CREATE UNIQUE INDEX idx_job_queue_unique_key
//!     ON job_queue(unique_key) WHERE unique_key IS NOT NULL;
//! ```

pub mod auth_lookup;
pub mod credential_consumer;
pub mod dependency_reconcile;
pub mod event_consumer;
pub mod orchestration_metrics;
pub mod orchestration_outbox_publisher;
mod orchestration_realtime;
pub mod orchestration_result_consumer;
pub mod participant_liveness;
pub mod queue;
pub mod worker;

pub use auth_lookup::{NatsConnectPasswordLookup, SqlxNatsConnectPasswordLookup};
pub use credential_consumer::{
    AgentOwner, AgentOwnerLookup, CredentialStreamWorker, CredentialWriter, HandleError as CredentialHandleError,
    SqlxAgentOwnerLookup, SqlxHmacSecretLookup as SqlxCredentialHmacSecretLookup, credentials_filter,
};
pub use dependency_reconcile::{DEFAULT_INTERVAL as DEPENDENCY_RECONCILE_DEFAULT_INTERVAL, DependencyReconcileWorker};
pub use event_consumer::{
    AgentDirectory, AgentTarget, BroadcastBus, BroadcastEnvelope, BroadcastMessage, EventConsumer, EventStore,
    EventStreamWorker, PersistedEvent, SignedEventEnvelope, SignedEventPayload,
};
pub use orchestration_metrics::{
    DEFAULT_CONTROL_PLANE_METRICS_INTERVAL, OrchestrationControlPlaneSnapshot, OrchestrationMetricsWorker,
    collect_control_plane_snapshot,
};
pub use orchestration_outbox_publisher::{OrchestrationOutboxPublisher, insert_assignment_outbox_in_tx};
pub use orchestration_result_consumer::{
    HandleError as OrchestrationResultHandleError, HmacSecretLookup, ORCHESTRATION_RESULTS_DURABLE,
    ORCHESTRATION_RESULTS_STREAM, OrchestrationResultConsumerConfig, OrchestrationResultWorker, ParticipantLookup,
    SqlxHmacSecretLookup, SqlxParticipantLookup, SqlxTaskWriter, TaskWriter, handle_message,
    handle_message_with_subject_prefix, results_filter, results_filter_for,
};
pub use participant_liveness::{
    DEFAULT_STALE_AFTER as PARTICIPANT_DEFAULT_STALE_AFTER,
    DEFAULT_STALE_SWEEP_INTERVAL as PARTICIPANT_DEFAULT_STALE_SWEEP_INTERVAL, ExpiredLeaseOutcome,
    ParticipantLivenessWorker, expire_working_leases, handle_heartbeat as handle_participant_heartbeat,
    mark_stale_offline as mark_stale_participants_offline, parse_heartbeat_agent_id,
};
pub use queue::{JobEntry, complete, dequeue, enqueue, fail, release_stale_locks};
pub use worker::Worker;

/// Crate version for health checks and diagnostics.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn register_metrics() {
    credential_consumer::register_metrics();
    orchestration_metrics::register_metrics();
    orchestration_outbox_publisher::register_metrics();
    orchestration_result_consumer::register_metrics();
}
