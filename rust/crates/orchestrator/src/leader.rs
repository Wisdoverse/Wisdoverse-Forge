//! Opt-in leader election for the orchestrator's singleton background loops (CN-6).
//!
//! The dispatch and review-escalation reapers are singleton sweeps: already
//! CORRECT under N replicas (every mutating statement uses `FOR UPDATE SKIP
//! LOCKED`, so no row is processed twice), but WASTEFUL — every replica scans the
//! same tables each tick. When `ORCHESTRATOR_LEADER_ELECTION_ENABLED` is set, a
//! single replica is ELECTED leader and is the only one that runs the sweep; the
//! rest skip every tick. Default-off, so single-replica deployments behave
//! exactly as before and the data-plane `SKIP LOCKED` consumers are untouched.
//!
//! ## Stable leadership, not per-tick mutual exclusion
//!
//! A naive "acquire the lock, sweep, release" per tick only serialises
//! *concurrent* sweeps: with replicas whose 60s intervals are staggered, each
//! would still acquire the just-released lock a few seconds later and run the
//! same scan, so the multi-replica waste would remain. Instead the leader HOLDS
//! the advisory lock for its whole lifetime on a DEDICATED connection
//! ([`ensure_leader`] keeps that connection in a caller-owned slot across ticks).
//! Other replicas call `pg_try_advisory_lock` every tick, observe it held, and
//! skip — no rotation, no redundant scans.
//!
//! ## Connection ownership, liveness, and leak-freedom
//!
//! `pg_try_advisory_lock` is SESSION-level, bound to the connection that ran it.
//! Returning a pooled connection does NOT close it, so a lock acquired via
//! `fetch_one(&pool)` would LEAK. We therefore keep the lock connection OUT of
//! the pool for the leader's lifetime, and:
//!
//! - each tick, before trusting leadership, we `SELECT 1` on that connection. If
//!   it errored (DB restart, network drop), we DETACH + close it (so it is not
//!   returned to the pool still holding the lock), which ends the Postgres
//!   session and releases the lock, then re-run election so another replica can
//!   take over. Detach+close is required: dropping a healthy pooled connection
//!   returns it to the pool WITHOUT closing the session, so the session lock
//!   would survive — a leak.
//! - on clean process exit the pool is dropped and its connections close,
//!   releasing the lock.
//!
//! So no path leaks the lock, and a dead leader is detected and replaced. The
//! reaper work still runs against the SHARED pool, so F051's bounded multi-batch
//! sweep is unchanged.

use sqlx::Connection;
use sqlx::PgPool;
use sqlx::Postgres;
use sqlx::pool::PoolConnection;

/// Advisory-lock class. Namespaces the orchestrator's leader locks so they cannot
/// collide with advisory locks taken elsewhere (e.g. the API's analytics refresh
/// lock, which uses a different class). Value spells "ORCH" in ASCII.
pub(crate) const LEADER_LOCK_CLASS: i32 = 0x4f52_4348;

/// Per-loop lock id. Each singleton loop gets a DISTINCT id so the dispatch
/// reaper and the review-escalation reaper are independent singletons that never
/// block one another.
pub(crate) const DISPATCH_REAPER_LOCK_ID: i32 = 1;
pub(crate) const REVIEW_ESCALATION_REAPER_LOCK_ID: i32 = 2;

/// Leadership status for one tick.
#[derive(Debug)]
pub(crate) enum LeaderStatus {
    /// This replica holds the lock (newly acquired or still held); run the work.
    Leader,
    /// Another replica is the leader; skip the work this tick.
    NotLeader,
    /// The lock could not be acquired/checked (DB error); skip the work.
    LockError(sqlx::Error),
}

/// Ensure this replica's leadership for the singleton loop keyed by `lock_id`,
/// keeping the lock-holding connection in `slot` across ticks.
///
/// - If `slot` already holds a live connection (verified by `SELECT 1`), we are
///   still the leader → [`LeaderStatus::Leader`].
/// - If that connection is dead, it is dropped (releasing the lock) and election
///   is retried.
/// - Otherwise we try to acquire the advisory lock on a fresh dedicated
///   connection; success stores it in `slot` and returns `Leader`, a held lock
///   returns [`LeaderStatus::NotLeader`].
pub(crate) async fn ensure_leader(
    pool: &PgPool,
    lock_id: i32,
    slot: &mut Option<PoolConnection<Postgres>>,
) -> LeaderStatus {
    // Already leader? Verify the lock connection is still alive — a session lock
    // is only as durable as its connection.
    if slot.is_some() {
        let alive = {
            let conn = slot.as_mut().expect("slot is Some");
            sqlx::query("SELECT 1").execute(&mut **conn).await.is_ok()
        };
        if alive {
            return LeaderStatus::Leader;
        }
        // Connection lost. DETACH + close it (so it is not returned to the pool
        // still owning the session lock), guaranteeing the lock is released, then
        // fall through to re-elect.
        if let Some(conn) = slot.take() {
            let _ = conn.detach().close().await;
        }
    }

    // Not (or no longer) leader: try to acquire on a fresh dedicated connection.
    let mut conn = match pool.acquire().await {
        Ok(conn) => conn,
        Err(err) => return LeaderStatus::LockError(err),
    };
    let locked: bool = match sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
        .bind(LEADER_LOCK_CLASS)
        .bind(lock_id)
        .fetch_one(&mut *conn)
        .await
    {
        Ok(locked) => locked,
        Err(err) => return LeaderStatus::LockError(err),
    };

    if locked {
        // Keep the connection (and thus the session lock) for our lifetime.
        *slot = Some(conn);
        LeaderStatus::Leader
    } else {
        // Lock held elsewhere — dropping `conn` returns it to the pool clean.
        LeaderStatus::NotLeader
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn singleton_loops_use_distinct_lock_ids() {
        assert_ne!(
            DISPATCH_REAPER_LOCK_ID, REVIEW_ESCALATION_REAPER_LOCK_ID,
            "each singleton loop needs its own lock id or they would block each other"
        );
    }

    #[sqlx::test]
    async fn becomes_leader_when_lock_is_free(pool: PgPool) {
        let mut slot = None;
        let status = ensure_leader(&pool, DISPATCH_REAPER_LOCK_ID, &mut slot).await;
        assert!(matches!(status, LeaderStatus::Leader), "free lock must elect us, got {status:?}");
        assert!(slot.is_some(), "leader must keep the lock connection in the slot");
    }

    #[sqlx::test]
    async fn holds_lock_across_ticks_so_other_replicas_never_run(pool: PgPool) {
        // This is the CN-6 contract that the per-tick design got wrong: once a
        // replica is leader it STAYS leader across ticks, and other replicas keep
        // observing the lock as held — no rotation, no redundant sweeps.
        let mut leader_slot = None;
        assert!(matches!(ensure_leader(&pool, DISPATCH_REAPER_LOCK_ID, &mut leader_slot).await, LeaderStatus::Leader));

        // A second replica (its own slot) must NOT become leader while we hold it.
        let mut other_slot = None;
        let other = ensure_leader(&pool, DISPATCH_REAPER_LOCK_ID, &mut other_slot).await;
        assert!(matches!(other, LeaderStatus::NotLeader), "another replica must not run, got {other:?}");
        assert!(other_slot.is_none(), "a non-leader must not retain a lock connection");

        // Next tick: we are STILL leader (lock held across ticks, not released).
        assert!(matches!(ensure_leader(&pool, DISPATCH_REAPER_LOCK_ID, &mut leader_slot).await, LeaderStatus::Leader));
        // And the other replica is STILL blocked — proving stability, not rotation.
        let mut other_slot2 = None;
        assert!(matches!(
            ensure_leader(&pool, DISPATCH_REAPER_LOCK_ID, &mut other_slot2).await,
            LeaderStatus::NotLeader
        ));
    }

    #[sqlx::test]
    async fn lock_is_released_when_the_leader_session_closes(pool: PgPool) {
        // A session-level advisory lock is released when the holding SESSION ends,
        // not merely when a pooled connection is returned (returning it to the
        // pool keeps the session — and the lock — alive). Elect a leader, then
        // detach + close its session to simulate the leader process dying; a new
        // replica must then be able to take over.
        let mut leader_slot = None;
        assert!(matches!(ensure_leader(&pool, DISPATCH_REAPER_LOCK_ID, &mut leader_slot).await, LeaderStatus::Leader));
        leader_slot.take().expect("leader conn").detach().close().await.expect("close leader session");

        let mut new_slot = None;
        let taken = ensure_leader(&pool, DISPATCH_REAPER_LOCK_ID, &mut new_slot).await;
        assert!(
            matches!(taken, LeaderStatus::Leader),
            "a freed lock must be acquirable by a new leader, got {taken:?}"
        );
    }

    #[sqlx::test]
    async fn distinct_lock_ids_do_not_block_each_other(pool: PgPool) {
        let mut dispatch_slot = None;
        assert!(matches!(
            ensure_leader(&pool, DISPATCH_REAPER_LOCK_ID, &mut dispatch_slot).await,
            LeaderStatus::Leader
        ));

        // The review-escalation reaper (different id) is an independent singleton.
        let mut review_slot = None;
        let review = ensure_leader(&pool, REVIEW_ESCALATION_REAPER_LOCK_ID, &mut review_slot).await;
        assert!(matches!(review, LeaderStatus::Leader), "a different lock id must not be blocked, got {review:?}");
    }
}
