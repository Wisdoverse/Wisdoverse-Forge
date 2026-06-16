//! Self-fix domain policy: pure, no I/O. Sensitive-path circuit breaker + review vocab.

use agentforge_core::{AppError, ErrorKind};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

/// Glob-prefix directories whose ANY descendant is sensitive.
#[allow(dead_code)]
const SENSITIVE_DIR_PREFIXES: &[&str] = &["rust/crates/auth/", "rust/crates/db/migrations/", ".github/workflows/"];

/// Basenames sensitive wherever they appear.
#[allow(dead_code)]
const SENSITIVE_BASENAMES: &[&str] = &["middleware.rs", "mcp.rs", "security.rs"];

/// Exact repo-root-relative files (own-code + CODEOWNERS).
#[allow(dead_code)]
const SENSITIVE_EXACT: &[&str] = &[
    ".github/CODEOWNERS",
    "rust/crates/api/src/services/self_fix/mod.rs",
    "rust/crates/api/src/services/self_fix/bridge.rs",
    "rust/crates/api/src/services/self_fix/import.rs",
    "rust/crates/api/src/services/self_fix/merge_executor.rs",
    "rust/crates/api/src/services/github_app/mod.rs",
    "rust/crates/api/src/routes/self_fix.rs",
    "rust/crates/api/src/domain/self_fix.rs",
];

#[allow(dead_code)]
pub(crate) struct SensitivePathPolicy;

impl SensitivePathPolicy {
    /// True if ANY changed path is sensitive. Input paths are repo-root-relative,
    /// forward-slashed (the form `git diff --name-only` / diff-tree emits).
    #[allow(dead_code)]
    pub(crate) fn touches_sensitive(changed_paths: &[String]) -> bool {
        changed_paths.iter().any(|p| Self::is_sensitive(p))
    }

    fn is_sensitive(path: &str) -> bool {
        if SENSITIVE_DIR_PREFIXES.iter().any(|d| path.starts_with(d)) {
            return true;
        }
        if SENSITIVE_EXACT.contains(&path) {
            return true;
        }
        let basename = path.rsplit('/').next().unwrap_or(path);
        SENSITIVE_BASENAMES.contains(&basename)
    }
}

/// Review status vocabulary (mirrors orchestrator ReviewState; driven API-side).
pub(crate) mod review_status {
    #[allow(dead_code)]
    pub(crate) const IN_REVIEW: &str = "in_review";
    #[allow(dead_code)]
    pub(crate) const APPROVED: &str = "approved";
    #[allow(dead_code)]
    pub(crate) const CHANGES_REQUESTED: &str = "changes_requested";
    #[allow(dead_code)]
    pub(crate) const MERGED: &str = "merged";
    /// Routed to CODEOWNERS / manual merge; in-platform Approve disabled.
    #[allow(dead_code)]
    pub(crate) const SENSITIVE_BLOCKED: &str = "sensitive_blocked";
}

#[allow(dead_code)]
pub(crate) fn self_fix_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

/// Read-side projection of a self-fix task's PR review state for the in-platform
/// review surface (milestone 8/9). Pure assembly of the persisted task columns
/// plus a freshly-read CI-check verdict; carries no secrets and no internal URLs.
///
/// `checks_green` and `sensitive` are the two gates the frontend Approve button
/// keys on: Approve is enabled only when `checks_green && !sensitive`. Both are
/// computed server-side so a hostile or stale client cannot widen the gate.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelfFixReview {
    /// The self-fix task this snapshot describes.
    pub task_id: Uuid,
    /// Draft-PR number, once the Bridge has opened one (NULL before then).
    pub pr_number: Option<i32>,
    /// Canonical PR URL (the `html_url` GitHub returned).
    pub pr_url: Option<String>,
    /// Deep link to the PR's file diff (`<pr_url>/files`), for the reviewer.
    pub diff_url: Option<String>,
    /// PR head SHA recorded at open time; the merge gate re-verifies against it.
    pub head_sha: Option<String>,
    /// Live CI verdict on `head_sha`. Fails CLOSED: `false` whenever GitHub is
    /// unconfigured, no head is recorded, or the check read errored.
    pub checks_green: bool,
    /// True when the change touched a sensitive path and was hard-blocked from
    /// in-platform merge at open time (`review_status == sensitive_blocked`).
    pub sensitive: bool,
    /// Persisted review-status string (see [`review_status`]).
    pub review_status: Option<String>,
}

impl SelfFixReview {
    /// Pure assembly from a task's persisted PR columns plus a freshly-read CI
    /// verdict. No I/O — every input is already in hand at the call site so this
    /// stays unit-testable. `sensitive` is derived from the persisted review
    /// status; `diff_url` is the PR URL's `/files` deep link when a PR exists.
    #[allow(dead_code)]
    pub(crate) fn from_columns(
        task_id: Uuid,
        pr_number: Option<i32>,
        pr_url: Option<String>,
        head_sha: Option<String>,
        review_status: Option<String>,
        checks_green: bool,
    ) -> Self {
        let diff_url = pr_url.as_deref().map(|u| format!("{}/files", u.trim_end_matches('/')));
        let sensitive = review_status.as_deref() == Some(review_status::SENSITIVE_BLOCKED);
        Self { task_id, pr_number, pr_url, diff_url, head_sha, checks_green, sensitive, review_status }
    }
}

/// Wire result of an approve→merge call (milestone 8). Mirrors the service-layer
/// `MergeOutcome` but is the domain-owned, `Serialize`-derived shape the route
/// returns, keeping the merge-executor result type out of the HTTP boundary.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelfFixMergeResult {
    /// The PR that was merged.
    pub pr_number: i32,
    /// The head SHA that was actually merged (the fresh, re-verified head).
    pub merged_head_sha: String,
    /// `true` when the PR was already merged before this call (idempotent path).
    pub already_merged: bool,
}

#[allow(dead_code)]
pub(crate) struct SelfFixPolicy;

impl SelfFixPolicy {
    #[allow(dead_code)]
    pub(crate) fn sensitive_path_blocked() -> AppError {
        ErrorKind::ForbiddenWithCode {
            code: "errors.self_fix.sensitive_path_blocked",
            message: "This PR touches a security-sensitive path; in-platform merge is disabled. \
                      Route it to a CODEOWNERS review and merge manually."
                .into(),
        }
        .into()
    }

    #[allow(dead_code)]
    pub(crate) fn checks_not_green() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.self_fix.checks_not_green",
            message: "Required CI checks are not all green; cannot merge.".into(),
        }
        .into()
    }

    #[allow(dead_code)]
    pub(crate) fn head_moved() -> AppError {
        ErrorKind::Conflict("the PR head moved since review; re-review required".into()).into()
    }

    /// The GitHub App integration is not configured, so no PR can be opened.
    #[allow(dead_code)]
    pub(crate) fn github_not_configured() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.self_fix.github_not_configured",
            message: "The self-fix GitHub App is not configured on this deployment; \
                      no pull request can be opened. Set the github_app_* settings and retry."
                .into(),
        }
        .into()
    }

    /// The task is not a self-fix task, so the PR Bridge must not run on it.
    #[allow(dead_code)]
    pub(crate) fn not_a_self_fix_task() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.self_fix.not_a_self_fix_task",
            message: "This task is not a self-fix task; the PR Bridge cannot open a pull request for it.".into(),
        }
        .into()
    }

    /// The task has no PR linkage (number + head SHA) yet, so there is nothing
    /// for the Merge Executor to merge. Open the PR first.
    #[allow(dead_code)]
    pub(crate) fn no_pr_to_merge() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.self_fix.no_pr_to_merge",
            message: "This self-fix task has no open pull request to merge yet; open the PR first.".into(),
        }
        .into()
    }

    /// The task is not in a review state from which the server will merge. Only
    /// an approved (or, transitionally, in-review) self-fix PR may be merged.
    #[allow(dead_code)]
    pub(crate) fn not_approved_for_merge() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.self_fix.not_approved_for_merge",
            message: "This self-fix pull request has not been approved for merge.".into(),
        }
        .into()
    }

    /// The agent's change failed the trust-boundary import (symlink, gitlink,
    /// `.git`, oversize, path escape, or a churn/deletion cap). NO PR is opened.
    /// `reason` is a safe, attacker-independent summary (no tokens, no secrets).
    #[allow(dead_code)]
    pub(crate) fn rebuild_rejected(reason: String) -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.self_fix.rebuild_rejected",
            message: format!("The agent's change was rejected before any pull request was opened: {reason}"),
        }
        .into()
    }

    /// The agent produced no change versus the base tree; nothing to PR.
    #[allow(dead_code)]
    pub(crate) fn empty_change() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.self_fix.empty_change",
            message: "The agent's change is identical to the base branch; there is nothing to open a pull request for."
                .into(),
        }
        .into()
    }

    /// A server-owned git/clone/push step failed. `stage` is a static label
    /// (e.g. "clone", "push", "rebuild"); never includes a token-bearing URL.
    #[allow(dead_code)]
    pub(crate) fn git_step_failed(stage: &'static str) -> AppError {
        ErrorKind::Unavailable(format!("self-fix: server-owned git step failed: {stage}")).into()
    }

    /// The task's workspace path could not be resolved to a server-visible host
    /// path inside the managed workspace root (escape attempt or missing config).
    #[allow(dead_code)]
    pub(crate) fn workspace_unresolved() -> AppError {
        ErrorKind::Validation(
            "self-fix: could not resolve the task's workspace to a host path inside the managed root".into(),
        )
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked(paths: &[&str]) -> bool {
        SensitivePathPolicy::touches_sensitive(&paths.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn sensitive_paths_trip_the_breaker() {
        assert!(blocked(&["rust/crates/auth/src/jwt.rs"]));
        assert!(blocked(&["rust/crates/db/migrations/099_x.sql"]));
        assert!(blocked(&["rust/crates/api/src/middleware.rs"]));
        assert!(blocked(&["rust/crates/auth/src/middleware.rs"]));
        assert!(blocked(&["rust/crates/api/src/mcp.rs"]));
        assert!(blocked(&["rust/crates/api/src/domain/mcp.rs"]));
        assert!(blocked(&["rust/crates/api/src/repositories/agent/mcp.rs"]));
        assert!(blocked(&["rust/crates/platform/src/security.rs"]));
        assert!(blocked(&[".github/workflows/ci.yml"]));
        assert!(blocked(&[".github/CODEOWNERS"]));
        assert!(blocked(&["rust/crates/api/src/services/self_fix/bridge.rs"]));
        assert!(blocked(&["rust/crates/api/src/services/self_fix/merge_executor.rs"]));
        assert!(blocked(&["rust/crates/api/src/services/github_app/mod.rs"]));
        assert!(blocked(&["rust/crates/api/src/domain/self_fix.rs"]));
    }

    #[test]
    fn benign_paths_do_not_trip() {
        assert!(!blocked(&["src/app/features/board/TaskCard.tsx"]));
        assert!(!blocked(&["rust/crates/api/src/routes/licenses.rs"]));
        assert!(!blocked(&["docs/guides/configuration.md"]));
    }

    #[test]
    fn a_mix_with_one_sensitive_path_trips() {
        assert!(blocked(&["README.md", "rust/crates/auth/src/jwt.rs"]));
    }

    #[test]
    fn bare_prefix_without_rust_is_not_the_matcher_input_form() {
        assert!(!blocked(&["crates/auth/x.rs"]));
    }

    // -- SelfFixReview::from_columns (milestone 8 read-side projection) --------

    #[test]
    fn review_passes_columns_through_and_builds_diff_url() {
        let task_id = Uuid::new_v4();
        let review = SelfFixReview::from_columns(
            task_id,
            Some(7),
            Some("https://github.com/o/r/pull/7".to_string()),
            Some("deadbeef".to_string()),
            Some(review_status::IN_REVIEW.to_string()),
            true,
        );
        assert_eq!(review.task_id, task_id);
        assert_eq!(review.pr_number, Some(7));
        assert_eq!(review.pr_url.as_deref(), Some("https://github.com/o/r/pull/7"));
        assert_eq!(review.diff_url.as_deref(), Some("https://github.com/o/r/pull/7/files"));
        assert_eq!(review.head_sha.as_deref(), Some("deadbeef"));
        assert!(review.checks_green);
        assert!(!review.sensitive, "in_review is not sensitive");
        assert_eq!(review.review_status.as_deref(), Some("in_review"));
    }

    #[test]
    fn review_sensitive_flag_is_derived_from_blocked_status() {
        let review = SelfFixReview::from_columns(
            Uuid::new_v4(),
            Some(1),
            Some("https://github.com/o/r/pull/1".to_string()),
            Some("abc".to_string()),
            Some(review_status::SENSITIVE_BLOCKED.to_string()),
            true,
        );
        assert!(review.sensitive, "sensitive_blocked must set sensitive = true");
    }

    #[test]
    fn review_without_pr_has_no_diff_url() {
        let review = SelfFixReview::from_columns(Uuid::new_v4(), None, None, None, None, false);
        assert_eq!(review.pr_url, None);
        assert_eq!(review.diff_url, None);
        assert!(!review.checks_green);
        assert!(!review.sensitive, "absent review status is not sensitive");
    }

    #[test]
    fn review_diff_url_does_not_double_slash() {
        let review = SelfFixReview::from_columns(
            Uuid::new_v4(),
            Some(2),
            Some("https://github.com/o/r/pull/2/".to_string()),
            None,
            None,
            false,
        );
        assert_eq!(review.diff_url.as_deref(), Some("https://github.com/o/r/pull/2/files"));
    }
}
