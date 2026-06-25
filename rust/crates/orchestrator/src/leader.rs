//! Opt-in leader election for the orchestrator's singleton background loops (CN-6).
//!
//! The dispatch and review-escalation reapers are singleton sweeps: already
//! CORRECT under N replicas (every mutating statement uses `FOR UPDATE SKIP
//! LOCKED`, so no row is processed twice), but WASTEFUL — every replica scans the
//! same tables each tick. When `ORCHESTRATOR_LEADER_ELECTION_ENABLED` is set,
//! each tick first tries to hold a Postgres advisory lock; only the replica that
//! holds it runs the sweep, the rest skip. Default-off, so single-replica
//! deployments behave exactly as before and the data-plane `SKIP LOCKED`
//! consumers are untouched.
//!
//! ## Why a dedicated connection + explicit unlock (not the pool)
//!
//! `pg_try_advisory_lock` takes a SESSION-level lock bound to the connection that
//! ran it. With a pooled connection, returning it to the pool does NOT close it,
//! so a session lock acquired via `fetch_one(&pool)` would LEAK — held by an idle
//! pooled connection, never released — and at a 60s tick that leak is permanent.
//! We therefore acquire a DEDICATED connection for the lock and, on the run path,
//! explicitly `pg_advisory_unlock` on that SAME connection before returning it.
//! On the skipped path the lock was never taken, so dropping the connection is
//! safe. A crash closes the connection, which Postgres treats as session end and
//! releases the lock — so there is no leak on any path. The protected work itself
//! still runs against the shared pool, so a reaper's bounded multi-batch sweep
//! (F051) is unchanged.

use std::future::Future;

use sqlx::PgPool;

/// Advisory-lock class. Namespaces the orchestrator's leader locks so they cannot
/// collide with advisory locks taken elsewhere (e.g. the API's analytics refresh
/// lock, which uses a different class). Value spells "ORCH" in ASCII.
pub(crate) const LEADER_LOCK_CLASS: i32 = 0x4f52_4348;

/// Per-loop lock id. Each singleton loop gets a DISTINCT id so the dispatch
/// reaper and the review-escalation reaper never block one another — they are
/// independent singletons, not one combined leader.
pub(crate) const DISPATCH_REAPER_LOCK_ID: i32 = 1;
pub(crate) const REVIEW_ESCALATION_REAPER_LOCK_ID: i32 = 2;

/// Outcome of a leader-gated tick.
#[derive(Debug)]
pub(crate) enum LeaderTick<T> {
    /// This replica held the lock and ran the work; carries its result.
    Ran(T),
    /// Another replica holds the lock; the work was NOT run this tick.
    Skipped,
    /// The lock could not be acquired/checked (DB error); the work was NOT run.
    LockError(sqlx::Error),
}

/// Run `work` only if this replica can acquire the advisory lock
/// `(LEADER_LOCK_CLASS, lock_id)`. See the module docs for the connection /
/// unlock contract. `work` runs against the caller's own resources (typically the
/// shared pool), not the lock connection.
pub(crate) async fn run_as_leader<F, Fut, T>(pool: &PgPool, lock_id: i32, work: F) -> LeaderTick<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    let mut conn = match pool.acquire().await {
        Ok(conn) => conn,
        Err(err) => return LeaderTick::LockError(err),
    };

    let locked: bool = match sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
        .bind(LEADER_LOCK_CLASS)
        .bind(lock_id)
        .fetch_one(&mut *conn)
        .await
    {
        Ok(locked) => locked,
        Err(err) => return LeaderTick::LockError(err),
    };

    if !locked {
        // Lock not taken — dropping the connection returns it to the pool clean.
        return LeaderTick::Skipped;
    }

    let outcome = work().await;

    // Best-effort release on the SAME connection that holds the lock. If it fails
    // (e.g. the connection just dropped), Postgres releases the session lock when
    // that connection closes, so the next acquire still succeeds — no leak.
    let _ = sqlx::query("SELECT pg_advisory_unlock($1, $2)")
        .bind(LEADER_LOCK_CLASS)
        .bind(lock_id)
        .execute(&mut *conn)
        .await;

    LeaderTick::Ran(outcome)
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
    async fn runs_work_when_lock_is_free_and_releases_it(pool: PgPool) {
        let outcome = run_as_leader(&pool, DISPATCH_REAPER_LOCK_ID, || async { 42 }).await;
        assert!(matches!(outcome, LeaderTick::Ran(42)), "free lock must run the work, got {outcome:?}");

        // The lock was released, so a SECOND call can acquire and run again.
        let again = run_as_leader(&pool, DISPATCH_REAPER_LOCK_ID, || async { 7 }).await;
        assert!(matches!(again, LeaderTick::Ran(7)), "lock must be released after a run, got {again:?}");
    }

    #[sqlx::test]
    async fn skips_work_when_another_holder_owns_the_lock(pool: PgPool) {
        // Simulate another replica/leader holding the lock on its own connection.
        let mut holder = pool.acquire().await.expect("holder connection");
        let held: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
            .bind(LEADER_LOCK_CLASS)
            .bind(DISPATCH_REAPER_LOCK_ID)
            .fetch_one(&mut *holder)
            .await
            .expect("hold the lock");
        assert!(held, "test setup: holder must take the lock");

        // run_as_leader acquires a DIFFERENT pooled connection and must observe
        // the lock as held -> Skipped, and must NOT run the work.
        let mut ran = false;
        let outcome = run_as_leader(&pool, DISPATCH_REAPER_LOCK_ID, || async {
            ran = true;
        })
        .await;
        assert!(matches!(outcome, LeaderTick::Skipped), "another holder must cause Skipped, got {outcome:?}");
        assert!(!ran, "work must not run while another replica holds the lock");

        // Release the holder so the test DB is left clean.
        let _ = sqlx::query("SELECT pg_advisory_unlock($1, $2)")
            .bind(LEADER_LOCK_CLASS)
            .bind(DISPATCH_REAPER_LOCK_ID)
            .execute(&mut *holder)
            .await;
    }

    #[sqlx::test]
    async fn distinct_lock_ids_do_not_block_each_other(pool: PgPool) {
        // Hold the dispatch-reaper lock; the review-escalation reaper (different
        // id) must still run — they are independent singletons.
        let mut holder = pool.acquire().await.expect("holder connection");
        let held: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
            .bind(LEADER_LOCK_CLASS)
            .bind(DISPATCH_REAPER_LOCK_ID)
            .fetch_one(&mut *holder)
            .await
            .expect("hold the dispatch lock");
        assert!(held);

        let outcome = run_as_leader(&pool, REVIEW_ESCALATION_REAPER_LOCK_ID, || async { "ran" }).await;
        assert!(matches!(outcome, LeaderTick::Ran("ran")), "a different lock id must not be blocked, got {outcome:?}");

        let _ = sqlx::query("SELECT pg_advisory_unlock($1, $2)")
            .bind(LEADER_LOCK_CLASS)
            .bind(DISPATCH_REAPER_LOCK_ID)
            .execute(&mut *holder)
            .await;
    }
}
