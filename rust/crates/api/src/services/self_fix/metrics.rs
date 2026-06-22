//! Prometheus metrics for the self-fix guarded-merge pipeline.
//!
//! One instrument, scoped to the Merge Executor path (issue #803):
//!
//! - `agentforge_self_fix_merge_total{outcome}` (counter) — one increment per
//!   `approve_and_merge` call, partitioned by the first failing gate (or success).
//!   Operators dashboard this to distinguish successful merges from each refusal
//!   reason and alert on sustained `checks_red` or `sensitive_blocked` rates.
//!
//! # Outcome label cardinality
//!
//! The `outcome` label takes a `&'static str` from a CLOSED set (bounded
//! cardinality). Every call to `approve_and_merge` records exactly one label
//! from the following exhaustive list; no runtime value can mint a new series:
//!
//! - `"merged"` — the PR was successfully merged by the Merge Executor.
//! - `"already_merged"` — the PR was already merged; `approve_and_merge` returned
//!   an idempotent success (no new merge was performed).
//! - `"sensitive_blocked"` — the task was hard-refused because its `review_status`
//!   is `sensitive_blocked`; in-platform merge is disabled for this PR.
//! - `"checks_red"` — the Merge Executor refused because CI checks were not all
//!   green on the live or post-ready-transition head.
//! - `"head_moved"` — the Merge Executor refused because the PR head changed
//!   since the recorded head SHA (concurrent push or automation).
//! - `"exhausted"` — the task's `merge_attempts` reached the configured cap
//!   (`self_fix_max_merge_attempts`); `review_status` flipped to
//!   `changes_requested` so the operator must inspect and re-approve.
//! - `"failed"` — any other executor error: GitHub API I/O, unexpected gate
//!   failure, or an error whose policy code is not one of the three gated codes.
//!   Operators can inspect the `tracing` log for the specific cause.
//!
//! # Recorder lifecycle
//!
//! The `metrics` facade is a no-op until a recorder is installed. The main
//! server binary installs the Prometheus exporter during boot; tests do not,
//! and the macros silently skip the recording path — no `OnceLock` or lazy
//! initialisation is needed here.
//!
//! # Relationship to `agentforge_self_fix_pr_total`
//!
//! `agentforge_self_fix_pr_total{outcome=opened|failed}` (from
//! `self_fix_pr_worker::register_metrics`) meters the PR Bridge open/fail
//! outcomes. This counter is intentionally separate: it meters the MERGE
//! decision points, which occur after an operator has approved a PR and the
//! server re-verifies safety before merging. The two counters complement each
//! other and must not be summed together.

use agentforge_core::{AppError, ErrorKind};

/// Classify a merge-executor `AppError` into a bounded-cardinality outcome
/// label. Matches the three policy codes the Merge Executor can return as
/// gate failures; any other error (I/O, infra, unexpected policy) maps to
/// `"failed"`.
///
/// Policy codes checked (exhaustive for merge-gate errors):
/// - `"errors.self_fix.checks_not_green"` → `"checks_red"`
/// - `"errors.self_fix.head_moved"` (Conflict variant) → `"head_moved"`
/// - `"errors.self_fix.sensitive_path_blocked"` → `"sensitive_blocked"`
///
/// Note: `head_moved` is emitted as [`ErrorKind::Conflict`], not
/// `ValidationWithCode`, so it does not carry a dotted code. It is detected
/// by its variant before code matching.
fn merge_failure_label(err: &AppError) -> &'static str {
    match &err.kind {
        // Conflict is head_moved — the only Conflict the executor emits.
        ErrorKind::Conflict(_) => "head_moved",
        ErrorKind::ValidationWithCode { code, .. } if *code == "errors.self_fix.checks_not_green" => "checks_red",
        ErrorKind::ForbiddenWithCode { code, .. }
            if *code == "errors.self_fix.sensitive_path_blocked" =>
        {
            "sensitive_blocked"
        }
        // All other errors: I/O, unexpected, or unrecognised policy.
        _ => "failed",
    }
}

/// Increment `agentforge_self_fix_merge_total{outcome}` for one completed
/// `approve_and_merge` call.
///
/// `outcome` must be one of the closed set defined in this module's doc
/// (`merged`, `already_merged`, `sensitive_blocked`, `checks_red`,
/// `head_moved`, `exhausted`, `failed`). Passing any other value will silently
/// create a new series — DO NOT call with runtime-derived strings.
pub fn record_merge_outcome(outcome: &'static str) {
    metrics::counter!("agentforge_self_fix_merge_total", "outcome" => outcome).increment(1);
}

/// Classify a merge-executor error and record the appropriate outcome label.
/// Convenience wrapper for the `Err` arm in `approve_and_merge`.
pub fn record_merge_failure(err: &AppError) {
    record_merge_outcome(merge_failure_label(err));
}

/// Register metric descriptions and prime all seven outcome series at 0.
///
/// Called once at server boot (before any traffic) so Prometheus sees all
/// `{outcome}` series from the first scrape — dashboards and alert rules that
/// query `agentforge_self_fix_merge_total` do not need to wait for the first
/// merge attempt to appear.
pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_self_fix_merge_total",
        "Self-fix guarded-merge outcomes, labeled merged|already_merged|sensitive_blocked|checks_red|head_moved|exhausted|failed"
    );
    // Prime all seven label values at 0 so every series exists from t=0.
    for outcome in &["merged", "already_merged", "sensitive_blocked", "checks_red", "head_moved", "exhausted", "failed"] {
        metrics::counter!("agentforge_self_fix_merge_total", "outcome" => *outcome).increment(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentforge_core::{AppError, ErrorKind};

    #[test]
    fn register_metrics_primes_series() {
        // Smoke test: must not panic regardless of whether a recorder is installed.
        register_metrics();
    }

    #[test]
    fn record_merge_outcome_does_not_panic() {
        // Smoke test for each closed-set label.
        for outcome in &["merged", "already_merged", "sensitive_blocked", "checks_red", "head_moved", "exhausted", "failed"] {
            record_merge_outcome(outcome);
        }
    }

    #[test]
    fn merge_failure_label_checks_not_green() {
        let err = AppError::from(ErrorKind::ValidationWithCode {
            code: "errors.self_fix.checks_not_green",
            message: "CI not green".into(),
        });
        assert_eq!(merge_failure_label(&err), "checks_red");
    }

    #[test]
    fn merge_failure_label_head_moved() {
        let err = AppError::from(ErrorKind::Conflict("the PR head moved since review".into()));
        assert_eq!(merge_failure_label(&err), "head_moved");
    }

    #[test]
    fn merge_failure_label_sensitive_blocked() {
        let err = AppError::from(ErrorKind::ForbiddenWithCode {
            code: "errors.self_fix.sensitive_path_blocked",
            message: "sensitive path blocked".into(),
        });
        assert_eq!(merge_failure_label(&err), "sensitive_blocked");
    }

    #[test]
    fn merge_failure_label_unknown_maps_to_failed() {
        let err = AppError::from(ErrorKind::Unavailable("github I/O timeout".into()));
        assert_eq!(merge_failure_label(&err), "failed");
    }

    #[test]
    fn merge_failure_label_internal_maps_to_failed() {
        let err = AppError::from(anyhow::anyhow!("unexpected error"));
        assert_eq!(merge_failure_label(&err), "failed");
    }

    #[test]
    fn merge_failure_label_unrelated_validation_code_maps_to_failed() {
        let err = AppError::from(ErrorKind::ValidationWithCode {
            code: "errors.self_fix.no_pr_to_merge",
            message: "no PR to merge".into(),
        });
        assert_eq!(merge_failure_label(&err), "failed");
    }
}
