//! Central JetStream stream provisioning. Ensures every stream the backend
//! depends on exists with the shape this version expects. Idempotent via
//! `create_or_update_stream`.

use agentforge_core::credential_protocol::creds_subject_wildcard;
use agentforge_core::orchestration_protocol::{
    ORCHESTRATION_ASSIGNMENTS_STREAM, assign_subject_wildcard, result_subject_wildcard,
};
use agentforge_jobs::{EVENTS_STREAM, ORCHESTRATION_RESULTS_STREAM};
use anyhow::{Context, Result};
use async_nats::jetstream::{self, stream};
use std::time::Duration;

pub const CREDENTIALS_STREAM: &str = "CREDENTIALS";
pub const EVENTS_STREAM_SUBJECT: &str = "events.ingest.>";

pub async fn ensure(client: async_nats::Client) -> Result<()> {
    let js = jetstream::new(client);

    js.create_or_update_stream(stream::Config {
        name: EVENTS_STREAM.to_string(),
        subjects: vec![EVENTS_STREAM_SUBJECT.to_string()],
        retention: stream::RetentionPolicy::Limits,
        max_age: Duration::from_secs(24 * 60 * 60),
        storage: stream::StorageType::File,
        discard: stream::DiscardPolicy::Old,
        ..Default::default()
    })
    .await
    .context("create or update EVENTS stream")?;

    js.create_or_update_stream(stream::Config {
        name: CREDENTIALS_STREAM.to_string(),
        subjects: vec![creds_subject_wildcard()],
        retention: stream::RetentionPolicy::WorkQueue,
        max_age: Duration::from_secs(24 * 60 * 60),
        storage: stream::StorageType::File,
        max_messages_per_subject: 100,
        discard: stream::DiscardPolicy::Old,
        ..Default::default()
    })
    .await
    .context("create or update CREDENTIALS stream")?;

    js.create_or_update_stream(stream::Config {
        name: ORCHESTRATION_ASSIGNMENTS_STREAM.to_string(),
        subjects: vec![assign_subject_wildcard()],
        retention: stream::RetentionPolicy::WorkQueue,
        max_age: Duration::from_secs(24 * 60 * 60),
        storage: stream::StorageType::File,
        max_messages_per_subject: 1_000,
        discard: stream::DiscardPolicy::Old,
        ..Default::default()
    })
    .await
    .context("create or update ORCHESTRATION_ASSIGNMENTS stream")?;

    js.create_or_update_stream(stream::Config {
        name: ORCHESTRATION_RESULTS_STREAM.to_string(),
        subjects: vec![result_subject_wildcard()],
        retention: stream::RetentionPolicy::WorkQueue,
        max_age: Duration::from_secs(24 * 60 * 60),
        storage: stream::StorageType::File,
        max_messages_per_subject: 1_000,
        discard: stream::DiscardPolicy::Old,
        ..Default::default()
    })
    .await
    .context("create or update ORCHESTRATION_RESULTS stream")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_stream_name_is_stable() {
        assert_eq!(CREDENTIALS_STREAM, "CREDENTIALS");
    }

    #[test]
    fn events_stream_name_is_stable() {
        assert_eq!(EVENTS_STREAM, "EVENTS");
        assert_eq!(agentforge_jobs::EVENTS_FILTER, "events.>");
        assert_eq!(EVENTS_STREAM_SUBJECT, "events.ingest.>");
    }

    #[test]
    fn orchestration_results_stream_name_is_stable() {
        assert_eq!(ORCHESTRATION_RESULTS_STREAM, "ORCHESTRATION_RESULTS");
    }

    #[test]
    fn orchestration_assignments_stream_name_is_stable() {
        assert_eq!(ORCHESTRATION_ASSIGNMENTS_STREAM, "ORCHESTRATION_ASSIGNMENTS");
    }
}
