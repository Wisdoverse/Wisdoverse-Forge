//! Prometheus metrics for the NATS auth callout pipeline.
//!
//! Two instruments, scoped to issue #38 phase 2:
//!
//! - `nats_auth_callout_duration_seconds` (histogram) — end-to-end latency of
//!   a single callout invocation. Emitted on every completion regardless of
//!   allow/deny outcome; operators dashboard the p99 to spot DB or JWT-signing
//!   regressions.
//! - `nats_auth_callout_unauthorized_total{reason}` (counter) — partitioned
//!   by a bounded set of short string reasons. Every deny path in the handler
//!   bumps exactly one of these, and dashboards alert on sustained
//!   `password_mismatch` or `agent_unknown` rates as an early credential-stuffing
//!   signal.
//!
//! # Reason label cardinality
//!
//! The `reason` label takes a `&'static str` from a closed set
//! (see the allowed values listed on `record_callout_unauthorized`).  This
//! keeps the series count bounded so the Prometheus time-series database cannot
//! be DOS'd from the wire (a forged request with a random UUID cannot mint a
//! new series — it hits `agent_unknown`).
//!
//! # Recorder lifecycle
//!
//! The `metrics` facade is a no-op until a recorder is installed. The main
//! server binary installs the Prometheus exporter during boot; tests do not,
//! and the macros silently skip the recording path — no `OnceLock` or lazy
//! initialisation is needed here.

use std::time::Duration;

/// Observe one completed callout invocation on the duration histogram.
///
/// Records the elapsed time from request receipt (before xkey decrypt) through
/// to response assembly (after JWT signing). Called on both allow and deny
/// paths so the histogram reflects the full mix of outcomes.
pub fn record_callout_duration(duration: Duration) {
    metrics::histogram!("nats_auth_callout_duration_seconds").record(duration.as_secs_f64());
}

/// Bump the unauthorized counter with a bounded-cardinality `reason` label.
///
/// Accepted values — keep this list exhaustive so the dashboards that panel on
/// `{reason=~"..."}` don't silently miss a new failure mode:
///
/// - `"agent_unknown"` — no row in `agents` for the claimed UUID (or the row
///   has a NULL `nats_connect_password`).
/// - `"password_mismatch"` — row exists but the presented password didn't
///   match. Indistinguishable on the wire from `agent_unknown` via the
///   50-150ms jitter.
/// - `"signing_failed"` — `nkeys::KeyPair::sign` or the inner/outer JWT
///   assembly failed. Almost never fires in practice; if it does, the
///   secrets are misconfigured.
/// - `"xkey_open_failed"` — inbound ciphertext failed XKey decrypt. Either a
///   wrong server xkey on the wire or an attacker forging without knowing
///   our recipient seed.
/// - `"bad_request"` — the inbound request was malformed: bad JWT structure,
///   wrong JSON shape, non-UUID user field, etc. Lumped together because
///   the client sees the same generic "auth failed" response.
/// - `"lookup_error"` — the DB lookup itself returned an error (pool
///   exhaustion, network, query error). Treated as a deny so we never leak
///   DB details to the client, but operators can alert on this specifically
///   to distinguish infra outages from credential stuffing.
pub fn record_callout_unauthorized(reason: &'static str) {
    metrics::counter!("nats_auth_callout_unauthorized_total", "reason" => reason).increment(1);
}

/// Bump the revoke counter with a bounded-cardinality `outcome` label. Called
/// from `AuthCalloutService::revoke` on every branch so operators can dashboard
/// the mix of successful KICKs vs. every fallback (DB clear + TTL only).
///
/// Accepted values (exhaustive):
///
/// - `"kick_published"` — the SYS-account publish on
///   `$SYS.REQ.SERVER.<name>.KICK` succeeded. Targeted revocation window ≤ 2s.
/// - `"no_tracked_connection"` — the tracker had no entry for this agent at
///   revoke time. Expected when the JWT had already expired or the agent
///   never connected. DB clear is still authoritative.
/// - `"no_sys_creds"` — `nats_callout.sys_password` is absent; revocation
///   degrades to DB clear + 15 min TTL. Non-zero rate means operators set
///   up partial NATS auth and should dashboard this to rotation SLOs.
/// - `"sys_connect_failed"` — SYS NATS dial errored. One log line per
///   failure at `warn!`; sustained rate = infra problem with the SYS
///   account boundary, NOT agent-level.
/// - `"kick_publish_failed"` — SYS NATS connect succeeded but the KICK
///   publish errored. Treated as a warning; revocation falls back to TTL.
pub fn record_callout_revoke(outcome: &'static str) {
    metrics::counter!("nats_auth_callout_revoke_total", "outcome" => outcome).increment(1);
}

/// Bump the worker-status counter when the subscribe loop encounters a
/// terminal condition. Used by ops monitoring to distinguish a healthy
/// pause from a silent exit.
pub fn record_callout_worker_status(status: &'static str) {
    metrics::counter!("nats_auth_callout_worker_status_total", "status" => status).increment(1);
}

/// Bump the infrastructure signing-error counter on a separate series from
/// `unauthorized_total{reason=signing_failed}`. Kept distinct so dashboards
/// that sum `unauthorized_total` for "total credential rejections" do not
/// double-count a single deny event when the fallback signing or xkey seal
/// path inside `build_deny_response` also fails.
///
/// Accepted values (exhaustive): `"deny_path_sign"`, `"deny_path_seal"`.
pub fn record_callout_signing_error(site: &'static str) {
    metrics::counter!("nats_auth_callout_signing_errors_total", "site" => site).increment(1);
}
