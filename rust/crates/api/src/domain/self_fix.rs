//! Self-fix domain policy: pure, no I/O. Sensitive-path circuit breaker + review vocab.

use agentforge_core::{AppError, AppResult, ErrorKind, SelfFixPrJob};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

/// Glob-prefix directories whose ANY descendant is sensitive.
#[allow(dead_code)]
const SENSITIVE_DIR_PREFIXES: &[&str] = &[
    "rust/crates/auth/",
    "rust/crates/db/migrations/",
    ".github/workflows/",
    "rust/crates/api/src/services/self_fix/",
    "rust/crates/api/src/services/github_app/",
    "rust/crates/api/src/domain/maintenance_delivery/",
    "rust/crates/api/src/repositories/orchestration/maintenance_delivery/",
    // Self-fix safety decisions beside `domain/self_fix.rs` (head reconciliation).
    "rust/crates/api/src/domain/self_fix/",
];

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
    "rust/crates/api/src/routes/self_fix.rs",
    "rust/crates/api/src/domain/self_fix.rs",
    "rust/crates/api/src/domain/maintenance.rs",
    "rust/crates/api/src/domain/maintenance_delivery.rs",
    "rust/crates/api/src/repositories/orchestration/maintenance_request.rs",
    "rust/crates/api/src/repositories/orchestration/maintenance_delivery.rs",
    // The merge mutex, decision lock/budget, review-status, attempt and head writes.
    "rust/crates/api/src/repositories/orchestration/self_fix_delivery.rs",
    // The in-transaction administrator re-check used by the merge gate and head reconciliation.
    "rust/crates/api/src/repositories/user/mod.rs",
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
mod reconcile;
pub(crate) use reconcile::{
    Comparison, PullRequestRead, RECONCILE_HEAD_AUDIT_ACTION, RECONCILE_HEAD_AUDIT_RESOURCE, ReconcileAction,
    ReconcileHeadInput, ReconcileHeadResult, ReconcileObservation, reconcile_head_action, reconcile_head_audit_details,
    reconcile_refusal,
};

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

pub(crate) fn self_fix_pr_job_payload(task_id: Uuid, org_id: Uuid) -> AppResult<Value> {
    serde_json::to_value(SelfFixPrJob { task_id, org_id }).map_err(|err| AppError::from(anyhow::Error::from(err)))
}

pub(crate) fn decode_self_fix_pr_job_payload(payload: Value) -> Result<SelfFixPrJob, serde_json::Error> {
    serde_json::from_value(payload)
}

/// The configured repository and one observed default-branch revision. The
/// bridge uses this same snapshot for both rebuilding and the PR base.
/// Permission flags describe installation-token grants, not CI results or
/// human approval. No credentials are included.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfFixRepositorySetup {
    pub repository: String,
    pub default_branch: String,
    pub base_sha: String,
    pub contents_write: bool,
    pub pull_requests_write: bool,
    pub checks_read: bool,
    pub squash_merge_allowed: bool,
}

impl SelfFixRepositorySetup {
    pub(crate) fn validate(&self) -> AppResult<()> {
        SelfFixPolicy::require_repository_writable(false, false, self.contents_write, self.pull_requests_write)?;
        if !valid_default_branch(&self.default_branch)
            || self.base_sha.len() != 40
            || !self.base_sha.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(SelfFixPolicy::repository_base_unavailable());
        }
        Ok(())
    }
}

/// Reject invalid Git refs before provider metadata enters a URL or git command.
pub(crate) fn valid_default_branch(branch: &str) -> bool {
    !branch.is_empty()
        && branch != "@"
        && !branch.starts_with('-')
        && !branch.ends_with('.')
        && !branch.contains("..")
        && !branch.contains("@{")
        && !branch.chars().any(|c| c.is_control() || " ~^:?*[\\".contains(c))
        && branch.split('/').all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
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
    /// True when the latest human verdict on the task's current maintenance
    /// report is `rejected`; the server then refuses the Forge merge (#1314).
    pub current_revision_rejected: bool,
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
        Self {
            task_id,
            pr_number,
            pr_url,
            diff_url,
            head_sha,
            checks_green,
            sensitive,
            review_status,
            current_revision_rejected: false,
        }
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

/// Domain-owned classifier for self-fix guarded-merge metrics.
///
/// Services record metrics, but the user-visible error-code policy belongs here
/// so the service layer does not own HTTP/domain error contracts.
#[allow(dead_code)]
pub(crate) struct SelfFixMergeMetricPolicy;

impl SelfFixMergeMetricPolicy {
    pub(crate) const OUTCOMES: &'static [&'static str] = &[
        "merged",
        "already_merged",
        "sensitive_blocked",
        "checks_red",
        "head_moved",
        "exhausted",
        "failed",
        "running",
        "unconfirmed",
    ];

    /// Classify a merge-executor `AppError` into a bounded-cardinality outcome
    /// label. Matches the three policy codes the Merge Executor can return as
    /// gate failures; any other error maps to `"failed"`.
    pub(crate) fn failure_label(err: &AppError) -> &'static str {
        match &err.kind {
            ErrorKind::Conflict(_) => "head_moved",
            ErrorKind::ValidationWithCode { code, .. } if *code == "errors.self_fix.checks_not_green" => "checks_red",
            ErrorKind::ForbiddenWithCode { code, .. } if *code == "errors.self_fix.sensitive_path_blocked" => {
                "sensitive_blocked"
            }
            _ => "failed",
        }
    }

    /// A definite refusal: our own pre-merge gates (red CI, head moved,
    /// sensitive) or the provider refusing the merge call (405/409). Any other
    /// failure may follow a completed merge, so the task stays `approved`.
    pub(crate) fn is_definite_refusal(err: &AppError) -> bool {
        matches!(Self::failure_label(err), "checks_red" | "head_moved" | "sensitive_blocked")
    }
}

/// Live pull-request state as reported by the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullRequestState {
    Open,
    Merged,
    ClosedUnmerged,
}

/// Review status at the start of a merge attempt that may proceed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MergeEntry {
    InReview,
    /// A previous attempt was started but not confirmed.
    Approved,
}

/// What an approve call does after taking the merge mutex (#1314).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MergeEntryAction {
    /// The pull request is merged: persist `merged`, count no attempt.
    MarkMerged,
    /// Closed without merge: persist `changes_requested` and refuse.
    MarkClosed,
    /// The last attempt may still be settling at the provider: refuse, change nothing.
    WaitUnconfirmed,
    /// At the cap with an unconfirmed, still-open attempt: keep `approved`, refuse.
    ExhaustedUnconfirmed,
    /// At the cap from `in_review`: persist `changes_requested`, refuse.
    Exhausted,
    /// Run the decision transaction and the executor.
    Proceed,
}

/// How long an unconfirmed attempt is given to finish or be dropped by the
/// provider, measured from the task row's last write.
/// Must exceed `MERGE_DECISION_BUDGET_MS`, which bounds how stale that write's timestamp can be.
pub(crate) const MERGE_SETTLE_WINDOW_SECS: i64 = 30;

/// Age budget of the merge decision transaction, also its `lock_timeout`. The
/// committed `approved` timestamp is the transaction start, so it is at most
/// this much (plus the commit's own latency) older than the commit.
/// Must stay below `MERGE_SETTLE_WINDOW_SECS`, so the effective window is at least 25 s.
pub(crate) const MERGE_DECISION_BUDGET_MS: u64 = 5_000;
// Compile-time check of the relation above.
const _: () = assert!(MERGE_DECISION_BUDGET_MS < MERGE_SETTLE_WINDOW_SECS as u64 * 1_000);

impl SelfFixPolicy {
    /// The single entry decision for an approve call: entry status x live pull
    /// request state (read only for `approved`) x seconds since the task row's
    /// last write x whether the attempt cap is reached.
    pub(crate) fn merge_entry_action(
        entry: MergeEntry,
        pull_request: Option<PullRequestState>,
        seconds_since_write: i64,
        at_cap: bool,
    ) -> MergeEntryAction {
        match (entry, pull_request) {
            (MergeEntry::InReview, _) if at_cap => MergeEntryAction::Exhausted,
            (MergeEntry::InReview, _) => MergeEntryAction::Proceed,
            (MergeEntry::Approved, Some(PullRequestState::Merged)) => MergeEntryAction::MarkMerged,
            (MergeEntry::Approved, Some(PullRequestState::ClosedUnmerged)) => MergeEntryAction::MarkClosed,
            // Open, or unknown (never read): wait out the settle window first.
            (MergeEntry::Approved, _) if seconds_since_write < MERGE_SETTLE_WINDOW_SECS => {
                MergeEntryAction::WaitUnconfirmed
            }
            (MergeEntry::Approved, None) => MergeEntryAction::WaitUnconfirmed,
            (MergeEntry::Approved, Some(PullRequestState::Open)) if at_cap => MergeEntryAction::ExhaustedUnconfirmed,
            (MergeEntry::Approved, Some(PullRequestState::Open)) => MergeEntryAction::Proceed,
        }
    }

    /// Review status after a definite refusal, from the pull request state
    /// re-read right after it: a provider refusal can still race a merge.
    pub(crate) fn review_status_after_refusal(pull_request: PullRequestState) -> &'static str {
        match pull_request {
            PullRequestState::Merged => review_status::MERGED,
            PullRequestState::ClosedUnmerged => review_status::CHANGES_REQUESTED,
            PullRequestState::Open => review_status::IN_REVIEW,
        }
    }

    /// The decision transaction is too old to commit: its `approved` timestamp
    /// would shorten the settle window by more than the budget.
    pub(crate) fn decision_over_budget(transaction_age_ms: i64) -> bool {
        transaction_age_ms > MERGE_DECISION_BUDGET_MS as i64
    }

    /// The previous attempt is unconfirmed and still inside the settle window.
    pub(crate) fn merge_unconfirmed() -> AppError {
        ErrorKind::ConflictWithCode {
            code: "errors.self_fix.merge_unconfirmed",
            message: "The last finish attempt is not confirmed yet. Try again in a moment.".into(),
        }
        .into()
    }

    /// No attempts remain and the last one is unconfirmed with the pull request still open.
    pub(crate) fn merge_attempts_exhausted_unconfirmed() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.self_fix.merge_attempts_exhausted_unconfirmed",
            message: "No finish attempts remain and the last one is not confirmed. Merge or close the pull request on \
                      GitHub, then select Finish this fix to update the status."
                .into(),
        }
        .into()
    }
}

#[allow(dead_code)]
pub(crate) struct SelfFixPolicy;

impl SelfFixPolicy {
    pub(crate) fn require_repository_writable(
        archived: bool,
        disabled: bool,
        contents_write: bool,
        pull_requests_write: bool,
    ) -> AppResult<()> {
        if archived || disabled {
            return Err(ErrorKind::ValidationWithCode {
                code: "errors.self_fix.repository_unavailable",
                message:
                    "The configured repository is archived or disabled. Restore repository access before retrying."
                        .into(),
            }
            .into());
        }
        if !contents_write || !pull_requests_write {
            return Err(ErrorKind::ValidationWithCode {
                code: "errors.self_fix.repository_permissions",
                message: "The GitHub App installation needs Contents and Pull requests read/write permissions. \
                          Update the installation permissions and retry after its cached token expires or restart the API."
                    .into(),
            }
            .into());
        }
        Ok(())
    }

    pub(crate) fn repository_base_unavailable() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.self_fix.repository_base_unavailable",
            message: "The configured repository has no usable default-branch revision. \
                      Confirm its default branch contains a commit before retrying."
                .into(),
        }
        .into()
    }

    pub(crate) fn repository_base_changed() -> AppError {
        ErrorKind::Conflict(
            "The existing pull request targets a different base branch. Review it on GitHub and create a new task \
             for the current repository default branch before retrying."
                .into(),
        )
        .into()
    }

    pub(crate) fn repository_access_failed() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.self_fix.repository_access",
            message: "The GitHub App cannot read the configured repository or default branch. \
                      Check GITHUB_APP_REPO, the installation's repository access and Contents permission, then retry."
                .into(),
        }
        .into()
    }

    /// Whether an `open_pr` failure is permanent — retrying can never succeed —
    /// versus transient (a network/GitHub incident worth retrying with backoff).
    ///
    /// Permanent: config/policy/validation/not-found/auth errors, plus `Conflict`
    /// (the PR head moved — it needs re-review, not a retry). Transient:
    /// `Unavailable` (GitHub/network down) and `Internal` (unexpected — retry and
    /// let it dead-letter if it persists). Owning this `ErrorKind` policy in the
    /// domain (not the worker service) keeps the DDD boundary intact (F047).
    #[allow(dead_code)]
    pub(crate) fn is_open_pr_error_permanent(err: &AppError) -> bool {
        matches!(
            err.kind,
            ErrorKind::NotFound(_)
                | ErrorKind::Validation(_)
                | ErrorKind::ValidationWithCode { .. }
                | ErrorKind::Unprocessable(_)
                | ErrorKind::Unauthorized
                | ErrorKind::Forbidden(_)
                | ErrorKind::ForbiddenWithCode { .. }
                | ErrorKind::Conflict(_)
        )
    }

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

    /// The task has exhausted its configurable merge-attempt budget. The caller
    /// has already flipped `review_status` to `changes_requested`; this error
    /// surfaces the refusal to the operator so they can inspect the PR and
    /// either force-push a fix or close the task.
    #[allow(dead_code)]
    pub(crate) fn merge_attempts_exhausted() -> AppError {
        ErrorKind::ValidationWithCode {
            code: "errors.self_fix.merge_attempts_exhausted",
            message: "Merge retry limit reached; review needs changes before another attempt.".into(),
        }
        .into()
    }

    /// Another approve call holds this task's merge mutex, or the decision
    /// transaction could not finish inside `MERGE_DECISION_BUDGET_MS` (#1314).
    pub(crate) fn merge_attempt_running() -> AppError {
        ErrorKind::ConflictWithCode {
            code: "errors.self_fix.merge_running",
            message: "Another operation on this change is in progress. Try again in a moment.".into(),
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
                      no pull request can be opened. Configure all four GITHUB_APP_* variables, restart the API and retry."
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
        assert!(blocked(&["rust/crates/api/src/domain/self_fix/reconcile.rs"]));
        assert!(blocked(&["rust/crates/api/src/domain/self_fix/any_future_module.rs"]));
        assert!(blocked(&["rust/crates/api/src/services/self_fix/reconcile.rs"]));
        assert!(blocked(&["rust/crates/api/src/services/github_app/mod.rs"]));
        assert!(blocked(&["rust/crates/api/src/services/github_app/verification.rs"]));
        assert!(blocked(&["rust/crates/api/src/services/github_app/nested/client.rs"]));
        assert!(blocked(&["rust/crates/api/src/domain/self_fix.rs"]));
        assert!(blocked(&["rust/crates/api/src/domain/maintenance_delivery.rs"]));
        assert!(blocked(&["rust/crates/api/src/domain/maintenance_delivery/tests.rs"]));
        assert!(blocked(&["rust/crates/api/src/repositories/orchestration/maintenance_delivery.rs"]));
        assert!(blocked(&["rust/crates/api/src/repositories/orchestration/maintenance_delivery/outcomes.rs"]));
    }

    #[test]
    fn benign_paths_do_not_trip() {
        assert!(!blocked(&["src/app/features/board/TaskCard.tsx"]));
        assert!(!blocked(&["rust/crates/api/src/routes/licenses.rs"]));
        assert!(!blocked(&["docs/guides/configuration.md"]));
    }

    #[test]
    fn merge_gate_queries_stay_on_the_protected_path() {
        assert!(blocked(&["rust/crates/api/src/repositories/orchestration/self_fix_delivery.rs"]));
        // The shared repository file is not protected, so the merge-gate
        // queries must not move back into it.
        let read = |file: &str| {
            std::fs::read_to_string(format!("{}/src/repositories/orchestration/{file}", env!("CARGO_MANIFEST_DIR")))
                .unwrap()
        };
        assert!(
            !read("mod.rs").contains("pg_try_advisory_lock"),
            "the merge-mutex query belongs in self_fix_delivery.rs (a protected path), not the shared mod.rs"
        );
        assert!(read("self_fix_delivery.rs").contains("pg_try_advisory_lock"));
        // The administrator re-check that `lock_task` relies on is protected too,
        // and must stay in the file the policy names.
        assert!(blocked(&["rust/crates/api/src/repositories/user/mod.rs"]));
        let user_repository =
            std::fs::read_to_string(format!("{}/src/repositories/user/mod.rs", env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert!(user_repository.contains("fn find_is_admin_by_id_in_tx"));
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

    #[test]
    fn merge_failure_label_checks_not_green() {
        let err = AppError::from(ErrorKind::ValidationWithCode {
            code: "errors.self_fix.checks_not_green",
            message: "CI not green".into(),
        });
        assert_eq!(SelfFixMergeMetricPolicy::failure_label(&err), "checks_red");
    }

    #[test]
    fn merge_failure_label_head_moved() {
        let err = AppError::from(ErrorKind::Conflict("the PR head moved since review".into()));
        assert_eq!(SelfFixMergeMetricPolicy::failure_label(&err), "head_moved");
    }

    #[test]
    fn merge_failure_label_sensitive_blocked() {
        let err = AppError::from(ErrorKind::ForbiddenWithCode {
            code: "errors.self_fix.sensitive_path_blocked",
            message: "sensitive path blocked".into(),
        });
        assert_eq!(SelfFixMergeMetricPolicy::failure_label(&err), "sensitive_blocked");
    }

    #[test]
    fn merge_failure_label_unknown_maps_to_failed() {
        let err = AppError::from(ErrorKind::Unavailable("github I/O timeout".into()));
        assert_eq!(SelfFixMergeMetricPolicy::failure_label(&err), "failed");
    }

    #[test]
    fn merge_failure_label_internal_maps_to_failed() {
        let err = AppError::from(anyhow::anyhow!("unexpected error"));
        assert_eq!(SelfFixMergeMetricPolicy::failure_label(&err), "failed");
    }

    #[test]
    fn merge_failure_label_unrelated_validation_code_maps_to_failed() {
        let err = AppError::from(ErrorKind::ValidationWithCode {
            code: "errors.self_fix.no_pr_to_merge",
            message: "no PR to merge".into(),
        });
        assert_eq!(SelfFixMergeMetricPolicy::failure_label(&err), "failed");
    }

    #[test]
    fn only_definite_merge_refusals_are_classified_definite() {
        for err in [
            SelfFixPolicy::checks_not_green(),
            SelfFixPolicy::head_moved(),
            SelfFixPolicy::sensitive_path_blocked(),
            crate::domain::github_app::not_mergeable(),
        ] {
            assert!(SelfFixMergeMetricPolicy::is_definite_refusal(&err));
        }
        // A transport failure or an unexpected error may follow a completed merge.
        for err in [
            AppError::from(ErrorKind::Unavailable("github: request failed".into())),
            AppError::from(anyhow::anyhow!("unexpected error")),
        ] {
            assert!(!SelfFixMergeMetricPolicy::is_definite_refusal(&err));
        }
    }

    #[test]
    fn merge_outcome_vocabulary_is_closed() {
        let expected = [
            "merged",
            "already_merged",
            "sensitive_blocked",
            "checks_red",
            "head_moved",
            "exhausted",
            "failed",
            "running",
            "unconfirmed",
        ];
        assert_eq!(SelfFixMergeMetricPolicy::OUTCOMES, expected.as_slice());
        for err in [
            SelfFixPolicy::checks_not_green(),
            SelfFixPolicy::head_moved(),
            SelfFixPolicy::sensitive_path_blocked(),
            SelfFixPolicy::merge_attempt_running(),
            SelfFixPolicy::merge_unconfirmed(),
            AppError::from(ErrorKind::Unavailable("github".into())),
        ] {
            assert!(SelfFixMergeMetricPolicy::OUTCOMES.contains(&SelfFixMergeMetricPolicy::failure_label(&err)));
        }
    }

    #[test]
    fn a_refusal_that_raced_a_merge_ends_merged() {
        use PullRequestState::*;
        assert_eq!(SelfFixPolicy::review_status_after_refusal(Merged), review_status::MERGED);
        assert_eq!(SelfFixPolicy::review_status_after_refusal(ClosedUnmerged), review_status::CHANGES_REQUESTED);
        assert_eq!(SelfFixPolicy::review_status_after_refusal(Open), review_status::IN_REVIEW);
    }

    #[test]
    fn a_decision_older_than_its_budget_is_refused() {
        let budget = MERGE_DECISION_BUDGET_MS as i64;
        assert!(!SelfFixPolicy::decision_over_budget(0));
        assert!(!SelfFixPolicy::decision_over_budget(budget));
        assert!(SelfFixPolicy::decision_over_budget(budget + 1));
    }

    #[test]
    fn merge_entry_action_covers_every_combination() {
        use MergeEntry::*;
        use MergeEntryAction::*;
        use PullRequestState::*;
        let fresh = MERGE_SETTLE_WINDOW_SECS - 1;
        let settled = MERGE_SETTLE_WINDOW_SECS;
        // (entry, pull request, seconds since last write, at cap) -> action
        let table = [
            // `in_review` never consults the provider; only the cap matters.
            (InReview, None, fresh, false, Proceed),
            (InReview, None, fresh, true, Exhausted),
            (InReview, None, settled, false, Proceed),
            (InReview, None, settled, true, Exhausted),
            (InReview, Some(Open), fresh, false, Proceed),
            (InReview, Some(Open), settled, true, Exhausted),
            (InReview, Some(Merged), fresh, false, Proceed),
            (InReview, Some(Merged), settled, true, Exhausted),
            (InReview, Some(ClosedUnmerged), fresh, false, Proceed),
            (InReview, Some(ClosedUnmerged), settled, true, Exhausted),
            // `approved`: the provider's actual state decides first.
            (Approved, Some(Merged), fresh, false, MarkMerged),
            (Approved, Some(Merged), fresh, true, MarkMerged),
            (Approved, Some(Merged), settled, false, MarkMerged),
            (Approved, Some(Merged), settled, true, MarkMerged),
            (Approved, Some(ClosedUnmerged), fresh, false, MarkClosed),
            (Approved, Some(ClosedUnmerged), fresh, true, MarkClosed),
            (Approved, Some(ClosedUnmerged), settled, false, MarkClosed),
            (Approved, Some(ClosedUnmerged), settled, true, MarkClosed),
            (Approved, Some(Open), fresh, false, WaitUnconfirmed),
            (Approved, Some(Open), fresh, true, WaitUnconfirmed),
            (Approved, Some(Open), settled, false, Proceed),
            (Approved, Some(Open), settled, true, ExhaustedUnconfirmed),
            (Approved, None, fresh, false, WaitUnconfirmed),
            (Approved, None, fresh, true, WaitUnconfirmed),
            (Approved, None, settled, false, WaitUnconfirmed),
            (Approved, None, settled, true, WaitUnconfirmed),
        ];
        for (entry, pr, age, cap, expected) in table {
            assert_eq!(
                SelfFixPolicy::merge_entry_action(entry, pr, age, cap),
                expected,
                "{entry:?} {pr:?} {age} {cap}"
            );
        }
    }

    #[test]
    fn open_pr_permanence_classification() {
        // Permanent — retrying can never succeed.
        for kind in [
            ErrorKind::NotFound("x".into()),
            ErrorKind::Validation("x".into()),
            ErrorKind::ValidationWithCode { code: "errors.self_fix.github_not_configured", message: "x".into() },
            ErrorKind::Unprocessable("x".into()),
            ErrorKind::Unauthorized,
            ErrorKind::Forbidden("x".into()),
            ErrorKind::ForbiddenWithCode { code: "errors.self_fix.sensitive_path_blocked", message: "x".into() },
            ErrorKind::Conflict("head moved".into()),
        ] {
            let err = AppError::from(kind);
            assert!(SelfFixPolicy::is_open_pr_error_permanent(&err), "{:?} should be permanent", err.kind);
        }

        // Transient — worth a retry (network/GitHub incident, or unexpected).
        assert!(!SelfFixPolicy::is_open_pr_error_permanent(&AppError::from(ErrorKind::Unavailable("down".into()))));
        assert!(!SelfFixPolicy::is_open_pr_error_permanent(&AppError::from(anyhow::anyhow!("boom"))));
    }
}
