//! Single source of truth for the signed-envelope replay window shared by the
//! event, orchestration-result, and credential consumers.
//!
//! The bound is skew tolerance (applied on BOTH sides of `now`), not expiry: an
//! envelope is accepted when its timestamp is within ±[`TIMESTAMP_REPLAY_WINDOW_SECS`]
//! of the consumer's clock. Keeping the rule in one pure, clock-injected
//! function lets the exact boundary be tested deterministically instead of
//! racing `Utc::now()` from a test (which made the per-consumer edge tests
//! flaky: stamping an event at exactly `now - WINDOW` and then re-reading the
//! clock one second later pushed it outside the window).

/// Accept signed envelopes whose `timestamp` is within ±5 minutes of the
/// consumer's clock. Same window across every signed-envelope consumer.
pub const TIMESTAMP_REPLAY_WINDOW_SECS: i64 = 300;

/// `true` when `timestamp` is within ±[`TIMESTAMP_REPLAY_WINDOW_SECS`] of
/// `now_secs` (inclusive on both edges). Pure and clock-injected so the
/// boundary is deterministically testable — callers pass `Utc::now().timestamp()`.
pub fn within_replay_window(now_secs: i64, timestamp: i64) -> bool {
    (now_secs - timestamp).abs() <= TIMESTAMP_REPLAY_WINDOW_SECS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_is_inclusive_and_symmetric() {
        // A fixed reference instant: no wall-clock read, so the exact edge is
        // pinned deterministically.
        let now = 1_700_000_000;

        // Exactly WINDOW seconds old, and exactly WINDOW ahead (clock skew):
        // both inclusive, both accepted.
        assert!(within_replay_window(now, now - TIMESTAMP_REPLAY_WINDOW_SECS));
        assert!(within_replay_window(now, now + TIMESTAMP_REPLAY_WINDOW_SECS));

        // One second past the bound on either side: rejected.
        assert!(!within_replay_window(now, now - TIMESTAMP_REPLAY_WINDOW_SECS - 1));
        assert!(!within_replay_window(now, now + TIMESTAMP_REPLAY_WINDOW_SECS + 1));

        // Dead centre.
        assert!(within_replay_window(now, now));
    }
}
