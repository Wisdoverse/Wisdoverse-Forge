//! Redis-backed presence for agent liveness (ADR 0008 Phase 2).
//!
//! When `PRESENCE_REDIS_ENABLED` is set AND Redis is connected, agent liveness
//! (`last_seen` / offline detection) is served from a per-agent Redis TTL key
//! instead of a per-heartbeat PostgreSQL write. `participants`/`agents` remain
//! the durable source of truth for lease-relevant `busy`/`available` status;
//! only the ephemeral liveness signal moves to Redis.
//!
//! Redis is optional. Every Redis operation degrades to the Phase 1 PostgreSQL
//! path on a missing connection or an error. Because PostgreSQL
//! `last_heartbeat_at` is not written on steady-state Redis beats, a fallback to
//! the PG offline sweep would see stale timestamps and wrongly mark live agents
//! offline; the backend therefore grace-skips the PG sweep for `stale_after`
//! after any fallback, giving agents time to repopulate `last_heartbeat_at` via
//! the PG path before offline detection resumes.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agentforge_infra::RedisClient;
use tokio::sync::RwLock;
use uuid::Uuid;

const PRESENCE_KEY_PREFIX: &str = "af:presence:";

/// What the caller must do after attempting to record a heartbeat in Redis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedisRecord {
    /// Recorded in Redis and the agent was already live — a pure-Redis beat with
    /// no PostgreSQL write, broadcast, or auto-dispatch needed.
    SteadyState,
    /// Recorded in Redis but the agent's key was absent (first beat or a
    /// TTL-expired resurrection). The caller must run the PostgreSQL transition
    /// write so `busy`/`available` + `last_heartbeat_at` are correct.
    Transition,
    /// Redis is disabled or unavailable; the caller must use the full
    /// PostgreSQL path. A fallback was recorded so the PG offline sweep is graced.
    Unavailable,
}

/// Presence backend shared by the liveness worker. Cheap to clone (`Arc` inner).
#[derive(Clone)]
pub struct PresenceBackend {
    inner: Arc<Inner>,
}

struct Inner {
    redis: Option<Arc<RwLock<RedisClient>>>,
    enabled: bool,
    stale_after: Duration,
    /// When the worker last fell back from Redis to the PG path. `None` once
    /// Redis is healthy again or when running pure-PG from the start.
    fallback_since: Mutex<Option<Instant>>,
    /// When a steady-state Redis beat last skipped a PostgreSQL write. This is
    /// the only thing that makes `last_heartbeat_at` stale, so the PG offline
    /// sweep is graced after a fallback ONLY if a skip happened recently — a
    /// Redis-down-at-boot (no skip ever) keeps `last_heartbeat_at` fresh via the
    /// PG path and needs no grace.
    last_steady_skip: Mutex<Option<Instant>>,
}

impl PresenceBackend {
    /// Pure-PostgreSQL backend: every call reports `Unavailable`, so the worker
    /// uses the Phase 1 path. Used when the flag is off or no Redis handle exists.
    pub fn postgres_only(stale_after: Duration) -> Self {
        Self::new(None, false, stale_after)
    }

    /// Redis-backed backend gated by `enabled` (the `PRESENCE_REDIS_ENABLED`
    /// flag). When `enabled` is false this behaves like [`postgres_only`].
    pub fn new(redis: Option<Arc<RwLock<RedisClient>>>, enabled: bool, stale_after: Duration) -> Self {
        Self {
            inner: Arc::new(Inner {
                redis,
                enabled,
                stale_after,
                fallback_since: Mutex::new(None),
                last_steady_skip: Mutex::new(None),
            }),
        }
    }

    /// True when Redis presence is the active liveness source (flag on + handle
    /// present). Individual operations may still fall back on a live error.
    pub fn redis_enabled(&self) -> bool {
        self.inner.enabled && self.inner.redis.is_some()
    }

    /// Record a heartbeat. On the Redis path this is a single `SET key 1 EX
    /// <stale_after> GET`: the returned prior value distinguishes a steady-state
    /// beat (key existed) from a transition (key absent). Any Redis problem
    /// degrades to `Unavailable` and arms the post-fallback grace window.
    pub async fn record(&self, agent_id: Uuid) -> RedisRecord {
        if !self.inner.enabled {
            return RedisRecord::Unavailable;
        }
        let Some(redis) = &self.inner.redis else {
            return RedisRecord::Unavailable;
        };
        let mut guard = redis.write().await;
        let Some(conn) = guard.connection_mut() else {
            self.note_fallback();
            return RedisRecord::Unavailable;
        };
        let key = format!("{PRESENCE_KEY_PREFIX}{agent_id}");
        let prior: Result<Option<String>, redis::RedisError> = redis::cmd("SET")
            .arg(&key)
            .arg("1")
            .arg("EX")
            .arg(self.inner.stale_after.as_secs())
            .arg("GET")
            .query_async(conn)
            .await;
        match prior {
            Ok(Some(_)) => {
                self.clear_fallback();
                // This beat skipped a PostgreSQL write — `last_heartbeat_at` for
                // this agent is now stale, which is what the sweep grace guards.
                *self.inner.last_steady_skip.lock().expect("presence steady-skip mutex") = Some(Instant::now());
                RedisRecord::SteadyState
            }
            Ok(None) => {
                self.clear_fallback();
                RedisRecord::Transition
            }
            Err(err) => {
                tracing::warn!(error = %err, %agent_id, "Redis presence SET failed; falling back to PostgreSQL heartbeat");
                metrics::counter!("agentforge_orchestration_presence_redis_errors_total", "op" => "set").increment(1);
                self.note_fallback();
                RedisRecord::Unavailable
            }
        }
    }

    /// Forget an agent's presence key. Called when a `Transition` beat's
    /// PostgreSQL write found no agent row: the `SET` already wrote the key, and
    /// leaving it would make the next beat report `SteadyState` and suppress the
    /// PG retry for the whole TTL. Best-effort — a failure just defers cleanup
    /// to TTL expiry.
    pub async fn forget(&self, agent_id: Uuid) {
        let Some(redis) = &self.inner.redis else {
            return;
        };
        let mut guard = redis.write().await;
        let Some(conn) = guard.connection_mut() else {
            return;
        };
        let key = format!("{PRESENCE_KEY_PREFIX}{agent_id}");
        let _: Result<i64, redis::RedisError> = redis::cmd("DEL").arg(&key).query_async(conn).await;
    }

    /// Given non-offline participant `agent_id`s, return those whose Redis
    /// presence key is absent (TTL-expired = offline). `None` means Redis was
    /// unavailable and the caller must fall back to the PG stale sweep (a
    /// fallback is recorded so that sweep is graced).
    pub async fn dead_agents(&self, candidates: &[Uuid]) -> Option<Vec<Uuid>> {
        if !self.inner.enabled {
            return None;
        }
        let Some(redis) = &self.inner.redis else {
            return None;
        };
        if candidates.is_empty() {
            return Some(Vec::new());
        }
        let mut guard = redis.write().await;
        let Some(conn) = guard.connection_mut() else {
            self.note_fallback();
            return None;
        };
        let mut pipe = redis::pipe();
        for agent_id in candidates {
            pipe.cmd("EXISTS").arg(format!("{PRESENCE_KEY_PREFIX}{agent_id}"));
        }
        let exists: Result<Vec<i64>, redis::RedisError> = pipe.query_async(conn).await;
        match exists {
            Ok(flags) => {
                self.clear_fallback();
                Some(
                    candidates
                        .iter()
                        .zip(flags)
                        .filter_map(|(agent_id, present)| (present == 0).then_some(*agent_id))
                        .collect(),
                )
            }
            Err(err) => {
                tracing::warn!(error = %err, "Redis presence EXISTS pipeline failed; falling back to PostgreSQL stale sweep");
                metrics::counter!("agentforge_orchestration_presence_redis_errors_total", "op" => "exists")
                    .increment(1);
                self.note_fallback();
                None
            }
        }
    }

    /// True while the PostgreSQL offline sweep must be skipped: we recently fell
    /// back from Redis AND a steady-state beat recently skipped a PG write, so
    /// some agent's `last_heartbeat_at` is stale and is still being repopulated
    /// by PG-path beats. Requiring a recent skip means a Redis-down-at-boot (no
    /// skip ever — every beat already took the PG path) runs the sweep
    /// immediately with fresh timestamps, with no offline-detection blackout.
    /// Both clocks self-expire after `stale_after`.
    pub fn pg_sweep_within_grace(&self) -> bool {
        let fell_back_recently = matches!(*self.inner.fallback_since.lock().expect("presence fallback mutex"), Some(at) if at.elapsed() < self.inner.stale_after);
        if !fell_back_recently {
            return false;
        }
        matches!(*self.inner.last_steady_skip.lock().expect("presence steady-skip mutex"), Some(at) if at.elapsed() < self.inner.stale_after)
    }

    fn note_fallback(&self) {
        let mut guard = self.inner.fallback_since.lock().expect("presence fallback mutex");
        if guard.is_none() {
            *guard = Some(Instant::now());
            metrics::counter!("agentforge_orchestration_presence_redis_fallback_total").increment(1);
        }
    }

    fn clear_fallback(&self) {
        *self.inner.fallback_since.lock().expect("presence fallback mutex") = None;
    }
}

/// Describe and materialise Phase 2 presence metrics at zero.
pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_orchestration_presence_redis_steady_total",
        "Heartbeats served entirely from Redis with no PostgreSQL write (the Phase 2 win)."
    );
    metrics::describe_counter!(
        "agentforge_orchestration_presence_redis_transition_total",
        "Redis-mode heartbeats that required a PostgreSQL transition write (first-seen/resurrection)."
    );
    metrics::describe_counter!(
        "agentforge_orchestration_presence_redis_fallback_total",
        "Transitions from the Redis presence path to the PostgreSQL fallback (Redis unavailability)."
    );
    metrics::describe_counter!(
        "agentforge_orchestration_presence_redis_errors_total",
        "Redis presence operations that errored, by op (set/exists)."
    );
    metrics::counter!("agentforge_orchestration_presence_redis_steady_total").increment(0);
    metrics::counter!("agentforge_orchestration_presence_redis_transition_total").increment(0);
    metrics::counter!("agentforge_orchestration_presence_redis_fallback_total").increment(0);
    metrics::counter!("agentforge_orchestration_presence_redis_errors_total", "op" => "set").increment(0);
    metrics::counter!("agentforge_orchestration_presence_redis_errors_total", "op" => "exists").increment(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_only_backend_is_never_redis_enabled() {
        let backend = PresenceBackend::postgres_only(Duration::from_secs(90));
        assert!(!backend.redis_enabled());
        assert!(!backend.pg_sweep_within_grace(), "pure-PG never arms the grace window");
    }

    #[test]
    fn flag_off_disables_redis_even_with_a_handle() {
        // enabled=false must behave like postgres_only even if a handle is present.
        let backend = PresenceBackend::new(None, false, Duration::from_secs(90));
        assert!(!backend.redis_enabled());
    }

    #[tokio::test]
    async fn disabled_backend_reports_unavailable_without_arming_grace() {
        let backend = PresenceBackend::postgres_only(Duration::from_secs(90));
        assert_eq!(backend.record(Uuid::new_v4()).await, RedisRecord::Unavailable);
        // A disabled backend is the steady PG state, not a fallback, so the
        // sweep must run normally (no grace).
        assert!(!backend.pg_sweep_within_grace());
        assert!(backend.dead_agents(&[Uuid::new_v4()]).await.is_none());
    }
}
