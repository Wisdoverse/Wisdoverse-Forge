//! In-memory tracker of minted NATS connections so Unit 8 can later revoke
//! them via `$SYS.REQ.SERVER.<name>.KICK`.
//!
//! # Shape
//!
//! When the callout handler mints an allow-path JWT it records
//! `(agent_id → (server_id, client_cid, issued_at))` here. On `stop_agent`,
//! Unit 8 consults this tracker to find the NATS server that currently hosts
//! the agent's connection and publishes a KICK request targeting that exact
//! `client_cid` — no other connections are affected.
//!
//! # Why in-memory
//!
//! The tracker is a convenience index, not the source of truth. If the API
//! process crashes before emitting a KICK, the outstanding JWT will still
//! expire on its own (15 min TTL, per `DEFAULT_JWT_TTL`). The tracker
//! recovery story is therefore bounded: a missed KICK extends the revocation
//! window by at most one JWT lifetime, which is acceptable for the P2 threat
//! model (stop_agent is not a zero-trust primitive — it's a convenience for
//! quiescing an agent faster than the JWT's natural expiry).
//!
//! # Memory budget
//!
//! [`reap_expired`] is meant to be invoked periodically from Unit 8's worker
//! loop (the `run` method) so entries for agents that died without a
//! `stop_agent` call do not accumulate. The natural cap is
//! `max_agents_per_server * num_api_instances`; at 10k agents × 32 bytes per
//! entry that's a few hundred kB — trivial. The reaper is defensive, not
//! load-bearing.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use uuid::Uuid;

/// One tracked connection's addressing info.
///
/// `server_id` is the NATS server's public nkey (`N…`) — the same value that
/// arrives on `AuthorizationRequest.server_id`. Unit 8 uses it to target the
/// KICK subject; clustered deployments have multiple server IDs but a single
/// `server_name`, so KICK addressing is per-node.
#[derive(Debug, Clone)]
pub struct TrackedConnection {
    pub server_id: String,
    pub client_cid: u64,
    /// Wall-clock instant at mint. Consulted only by [`ConnectionTracker::reap_expired`];
    /// never serialized so drift between API instances is fine.
    pub issued_at: Instant,
}

/// Shared tracker holding one entry per currently-authorised agent.
///
/// `Clone` is cheap (an `Arc` clone). All mutations go through the inner
/// `Mutex`, which is held briefly — O(1) map operations, no I/O under lock.
#[derive(Clone, Default)]
pub struct ConnectionTracker {
    inner: Arc<Mutex<HashMap<Uuid, TrackedConnection>>>,
}

impl ConnectionTracker {
    /// Construct a fresh tracker with an empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Store `(server_id, client_cid)` for `agent_id`. Overwrites any previous
    /// entry for the same agent — a reconnect from the same agent produces a
    /// new `client_cid` and the old one has already been dropped by the NATS
    /// server, so forgetting the old entry is correct.
    pub async fn record(&self, agent_id: Uuid, server_id: String, client_cid: u64) {
        let mut guard = self.inner.lock().await;
        guard.insert(agent_id, TrackedConnection { server_id, client_cid, issued_at: Instant::now() });
    }

    /// Remove and return the tracked entry for `agent_id`, if any. Unit 8's
    /// `revoke` path calls this immediately before publishing the KICK: the
    /// entry is consumed so a second concurrent `stop_agent` does not double-KICK.
    pub async fn take(&self, agent_id: Uuid) -> Option<TrackedConnection> {
        let mut guard = self.inner.lock().await;
        guard.remove(&agent_id)
    }

    /// Drop entries older than `max_age`. Intended to be called periodically
    /// (every minute or so) by Unit 8's background loop to cap memory growth
    /// on long-lived API instances where agents die without going through
    /// `stop_agent`. The entry itself has no bearing on authorisation — the
    /// minted JWT expires on its own — so reaping is purely a memory-hygiene
    /// concern.
    pub async fn reap_expired(&self, max_age: Duration) {
        let now = Instant::now();
        let mut guard = self.inner.lock().await;
        guard.retain(|_, entry| now.duration_since(entry.issued_at) <= max_age);
    }

    /// Test / diagnostic helper: number of currently tracked entries.
    #[cfg(test)]
    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }

    /// Test / diagnostic helper: whether the tracker is empty. Paired with
    /// [`Self::len`] to satisfy `clippy::len_without_is_empty` on the
    /// `#[cfg(test)]` gate.
    #[cfg(test)]
    pub async fn is_empty(&self) -> bool {
        self.inner.lock().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn record_then_take_returns_entry() {
        let tracker = ConnectionTracker::new();
        let agent = Uuid::new_v4();
        tracker.record(agent, "NSERVER123".to_string(), 4242).await;

        let taken = tracker.take(agent).await.expect("take returns Some");
        assert_eq!(taken.server_id, "NSERVER123");
        assert_eq!(taken.client_cid, 4242);

        // take consumes — second take is None.
        assert!(tracker.take(agent).await.is_none());
    }

    #[tokio::test]
    async fn take_missing_returns_none() {
        let tracker = ConnectionTracker::new();
        let agent = Uuid::new_v4();
        assert!(tracker.take(agent).await.is_none());
    }

    #[tokio::test]
    async fn reap_expired_drops_old_entries() {
        let tracker = ConnectionTracker::new();
        let old = Uuid::new_v4();
        let fresh = Uuid::new_v4();

        // Record one entry, manually back-date its issued_at so reap will see
        // it as expired.
        tracker.record(old, "NSERVER-OLD".to_string(), 1).await;
        {
            let mut guard = tracker.inner.lock().await;
            // Back-date by 10 seconds — well past the 1-ms reap threshold below.
            let entry = guard.get_mut(&old).expect("just inserted");
            entry.issued_at = Instant::now()
                .checked_sub(Duration::from_secs(10))
                .expect("clock supports subtraction in test environment");
        }

        // Fresh entry inserted now — must survive the reap.
        tracker.record(fresh, "NSERVER-FRESH".to_string(), 2).await;

        tracker.reap_expired(Duration::from_millis(1)).await;

        assert!(tracker.take(old).await.is_none(), "old entry should have been reaped");
        let surviving = tracker.take(fresh).await.expect("fresh entry survives");
        assert_eq!(surviving.client_cid, 2);
    }

    #[tokio::test]
    async fn record_overwrites_previous_entry_for_same_agent() {
        // Agent reconnects: NATS assigns a new client_cid, we record again.
        // The old entry must be gone, not accumulated into a list.
        let tracker = ConnectionTracker::new();
        let agent = Uuid::new_v4();
        tracker.record(agent, "N1".to_string(), 100).await;
        tracker.record(agent, "N2".to_string(), 200).await;

        assert_eq!(tracker.len().await, 1);
        let taken = tracker.take(agent).await.expect("some entry");
        assert_eq!(taken.server_id, "N2");
        assert_eq!(taken.client_cid, 200);
    }
}
