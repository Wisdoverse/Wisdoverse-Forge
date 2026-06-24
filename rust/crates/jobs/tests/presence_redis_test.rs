//! Integration coverage for the ADR 0008 Phase 2 Redis presence backend.
//!
//! Gated on a reachable Redis: set `REDIS_TEST_URL` (e.g.
//! `redis://:password@127.0.0.1:6379`). When unset or unreachable the test
//! skips, mirroring the NATS-gated orchestration tests — so CI without a Redis
//! does not fail. The pure-PostgreSQL paths are covered by the unit tests in
//! `presence_store` and the existing `participant_heartbeat_liveness_test`.

use std::sync::Arc;
use std::time::Duration;

use agentforge_infra::RedisClient;
use agentforge_jobs::{PresenceBackend, RedisRecord};
use tokio::sync::RwLock;
use uuid::Uuid;

async fn backend(ttl: Duration) -> Option<PresenceBackend> {
    let url = std::env::var("REDIS_TEST_URL").ok()?;
    let client = RedisClient::connect(Some(&url)).await;
    if !client.is_connected() {
        eprintln!("skipping presence_redis_test: REDIS_TEST_URL set but not connectable");
        return None;
    }
    Some(PresenceBackend::new(Some(Arc::new(RwLock::new(client))), true, ttl))
}

#[tokio::test]
async fn record_distinguishes_first_beat_from_steady_state() {
    let Some(backend) = backend(Duration::from_secs(30)).await else {
        eprintln!("skipping: no REDIS_TEST_URL");
        return;
    };
    assert!(backend.redis_enabled());
    let agent = Uuid::new_v4();

    // First beat: key absent -> Transition (caller does the PG status write).
    assert_eq!(backend.record(agent).await, RedisRecord::Transition);
    // Second beat: key present -> SteadyState (zero PostgreSQL).
    assert_eq!(backend.record(agent).await, RedisRecord::SteadyState);
    assert_eq!(backend.record(agent).await, RedisRecord::SteadyState);
    // A successful Redis op clears any fallback grace.
    assert!(!backend.pg_sweep_within_grace());
}

#[tokio::test]
async fn dead_agents_reports_only_expired_keys() {
    // Short TTL so the key expires within the test.
    let Some(backend) = backend(Duration::from_secs(1)).await else {
        eprintln!("skipping: no REDIS_TEST_URL");
        return;
    };
    let live = Uuid::new_v4();
    let never_seen = Uuid::new_v4();

    backend.record(live).await; // sets live's key with a 1s TTL

    // Immediately: live is present, never_seen is absent (dead).
    let dead = backend.dead_agents(&[live, never_seen]).await.expect("redis available");
    assert!(!dead.contains(&live), "a just-recorded agent is live");
    assert!(dead.contains(&never_seen), "an agent with no key is dead");

    // After the TTL elapses, live expires and is reported dead.
    tokio::time::sleep(Duration::from_millis(1300)).await;
    let dead = backend.dead_agents(&[live]).await.expect("redis available");
    assert!(dead.contains(&live), "an expired key means offline");
}

#[tokio::test]
async fn forget_clears_a_stray_key_so_the_next_beat_retries() {
    let Some(backend) = backend(Duration::from_secs(30)).await else {
        eprintln!("skipping: no REDIS_TEST_URL");
        return;
    };
    let agent = Uuid::new_v4();

    // First beat sets the key (Transition). Simulate the "no agent row" path by
    // forgetting it; the agent must then look absent again so the PG write retries.
    assert_eq!(backend.record(agent).await, RedisRecord::Transition);
    backend.forget(agent).await;
    assert_eq!(
        backend.record(agent).await,
        RedisRecord::Transition,
        "after forget the next beat is a transition again, not a suppressed steady-state"
    );
}

#[tokio::test]
async fn empty_candidate_set_is_handled() {
    let Some(backend) = backend(Duration::from_secs(30)).await else {
        eprintln!("skipping: no REDIS_TEST_URL");
        return;
    };
    assert_eq!(backend.dead_agents(&[]).await, Some(Vec::new()));
}
