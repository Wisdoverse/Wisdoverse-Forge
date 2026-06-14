//! PR Bridge wiring for the self-fix loop.
//!
//! Ties together the pieces already committed (the local `rebuild_branch` core,
//! the sensitive-path policy, the GitHub App client, and the orchestration task
//! repository) into one flow: `open_pr`.
//!
//! Trust boundary (non-negotiable): the server NEVER runs `git` against the
//! agent's `/workspace/.git`. The Bridge reads `/workspace` as plain files
//! (`rebuild_branch` does the vetted filesystem walk) and runs `git` ONLY in a
//! fresh, server-owned clone directory whose `origin` carries a short-lived
//! installation token. The token-bearing remote URL is NEVER logged and NEVER
//! written under `/workspace`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use agentforge_core::{AppError, AppResult};
use tokio::process::Command;
use uuid::Uuid;

use crate::domain::self_fix::review_status::{IN_REVIEW, SENSITIVE_BLOCKED};
use crate::domain::self_fix::{SelfFixPolicy, SensitivePathPolicy};
use crate::services::self_fix::import::{ImportLimits, ImportReject};
use crate::services::self_fix::rebuild::{rebuild_branch, RebuildError};

/// Default ephemeral work-dir root for server-owned clones. One subdir per task.
const DEFAULT_WORK_DIR: &str = "/tmp/agentforge-selffix";

/// Per-`git` invocation wall-clock budget for the network clone/push. Generous
/// vs. the local rebuild budget because the clone is a network operation.
const GIT_TIMEOUT: Duration = Duration::from_secs(300);

/// Server-authored commit identity. Independent of any global git config.
const AUTHOR_NAME: &str = "Wisdoverse Self-Fix";
const AUTHOR_EMAIL: &str = "self-fix@users.noreply.github.com";

/// Base branch self-fix PRs target.
const BASE_BRANCH: &str = "main";

/// What `open_pr` returns on success: the opened draft PR and the review status
/// the Bridge selected (`in_review` or `sensitive_blocked`).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SelfFixPrOutcome {
    pub pr_number: i32,
    pub pr_url: String,
    pub review_status: &'static str,
}

/// The draft PR a [`GitProvider`] opened. Owned (not the wire `PullRequest`) so
/// fakes can construct it without depending on the GitHub deserialize shape.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct OpenedDraftPr {
    pub number: i32,
    pub html_url: String,
    pub head_sha: String,
}

/// The GitHub-dependent operations the Bridge AND the Merge Executor need.
/// Abstracted as a trait so integration tests can inject a fake backed by a
/// local `file://` origin (PR Bridge) or an in-memory state machine (Merge
/// Executor) while production uses the real
/// [`crate::services::github_app::GithubAppClient`].
#[allow(dead_code)]
#[async_trait::async_trait]
pub trait GitProvider: Send + Sync {
    /// `origin/main` SHA — the base pin the agent's change is rebuilt onto.
    async fn default_branch_sha(&self) -> AppResult<String>;
    /// Token-bearing clone/push remote for the server-owned clone's `origin`.
    /// NEVER logged.
    async fn authed_remote_url(&self) -> AppResult<String>;
    /// Open a draft PR from `head_branch` into `base`.
    async fn create_draft_pr(
        &self,
        head_branch: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> AppResult<OpenedDraftPr>;

    // --- Merge Executor (milestone 7) operations ---

    /// True IFF every check run on `head_sha` completed with `success`
    /// (and at least one ran). This is the merge safety gate.
    async fn all_checks_green(&self, head_sha: &str) -> AppResult<bool>;
    /// Current head SHA of the PR (re-read just before the atomic merge).
    async fn pr_head_sha(&self, pr_number: i32) -> AppResult<String>;
    /// `true` if the PR is already merged (idempotency check).
    async fn pr_is_merged(&self, pr_number: i32) -> AppResult<bool>;
    /// Flip a draft PR to ready-for-review.
    async fn mark_ready_for_review(&self, pr_number: i32) -> AppResult<()>;
    /// Squash-merge the PR ONLY if its head still equals `expected_head`.
    /// GitHub's 409 (head moved) is the atomic guard.
    async fn merge_with_expected_head(&self, pr_number: i32, expected_head: &str) -> AppResult<()>;
    /// Post an audit comment on the PR.
    async fn comment(&self, pr_number: i32, body: &str) -> AppResult<()>;
}

#[async_trait::async_trait]
impl GitProvider for crate::services::github_app::GithubAppClient {
    async fn default_branch_sha(&self) -> AppResult<String> {
        crate::services::github_app::GithubAppClient::default_branch_sha(self).await
    }

    async fn authed_remote_url(&self) -> AppResult<String> {
        crate::services::github_app::GithubAppClient::authed_remote_url(self).await
    }

    async fn create_draft_pr(
        &self,
        head_branch: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> AppResult<OpenedDraftPr> {
        let pr =
            crate::services::github_app::GithubAppClient::create_draft_pr(self, head_branch, base, title, body).await?;
        Ok(OpenedDraftPr { number: pr.number, html_url: pr.html_url, head_sha: pr.head.sha })
    }

    async fn all_checks_green(&self, head_sha: &str) -> AppResult<bool> {
        crate::services::github_app::GithubAppClient::all_checks_green(self, head_sha).await
    }

    async fn pr_head_sha(&self, pr_number: i32) -> AppResult<String> {
        crate::services::github_app::GithubAppClient::pr_head_sha(self, pr_number).await
    }

    async fn pr_is_merged(&self, pr_number: i32) -> AppResult<bool> {
        crate::services::github_app::GithubAppClient::pr_is_merged(self, pr_number).await
    }

    async fn mark_ready_for_review(&self, pr_number: i32) -> AppResult<()> {
        crate::services::github_app::GithubAppClient::mark_ready_for_review(self, pr_number).await
    }

    async fn merge_with_expected_head(&self, pr_number: i32, expected_head: &str) -> AppResult<()> {
        crate::services::github_app::GithubAppClient::merge_with_expected_head(self, pr_number, expected_head).await
    }

    async fn comment(&self, pr_number: i32, body: &str) -> AppResult<()> {
        crate::services::github_app::GithubAppClient::comment(self, pr_number, body).await
    }
}

/// Deterministic per-task branch name. The PR Bridge owns this branch
/// exclusively for this task; retries force-push a new sibling commit onto it.
#[allow(dead_code)]
pub fn branch_name(task_id: Uuid) -> String {
    format!("agent/{task_id}")
}

/// Ephemeral clone dir for a task: `${SELF_FIX_WORK_DIR:-/tmp/agentforge-selffix}/<task_id>`.
#[allow(dead_code)]
pub fn clone_dir_for(task_id: Uuid) -> PathBuf {
    let root = std::env::var("SELF_FIX_WORK_DIR").unwrap_or_else(|_| DEFAULT_WORK_DIR.to_string());
    Path::new(&root).join(task_id.to_string())
}

/// The successful result of the git-heavy bridge core (no DB involved). The
/// caller persists `base_sha` + the PR metadata + `review_status`.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct BridgeResult {
    pub pr: OpenedDraftPr,
    pub review_status: &'static str,
}

/// RAII guard that wipes the ephemeral server-owned clone dir on EVERY exit path
/// of `run_pr_bridge` — success, error, or early `?` return.
///
/// Why this matters: `git clone <authed_url>` stores the short-lived installation
/// token inside the clone's `.git/config` as `remote.origin.url`. Without this
/// guard the token-bearing config would linger on disk until the START of the
/// next run for this task (the start-of-run wipe). Removing the whole dir in
/// `Drop` deletes that config the moment the bridge call returns, so the token
/// is on disk only for the duration of the call.
struct CloneDirGuard {
    dir: PathBuf,
}

impl Drop for CloneDirGuard {
    fn drop(&mut self) {
        // Best-effort: a failed cleanup must not panic out of a Drop. The
        // start-of-next-run wipe is the backstop if this ever fails.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Map a per-`git` invocation rejection to a safe, attacker-independent summary.
fn reject_reason(reject: &ImportReject) -> String {
    match reject {
        ImportReject::Symlink(p) => format!("a symlink is not allowed ({p})"),
        ImportReject::Gitlink(p) => format!("a nested git repo / submodule is not allowed ({p})"),
        ImportReject::SpecialFile(p) => format!("a special file (pipe/socket/device) is not allowed ({p})"),
        ImportReject::EscapesRoot(p) => format!("a path escapes the project root ({p})"),
        ImportReject::DotGit(p) => format!("changes under .git are not allowed ({p})"),
        ImportReject::OversizeFile(p) => format!("a file exceeds the size limit ({p})"),
        ImportReject::ChurnCapExceeded { changed, cap } => {
            format!("too many changed files ({changed} > {cap})")
        }
        ImportReject::DeletionCapExceeded { deleted, cap } => {
            format!("too many deletions ({deleted} > {cap})")
        }
    }
}

/// Map a `RebuildError` to a visible domain error. `Rejected`/`EmptyChange` are
/// task-failing policy errors (NO PR is opened); `Git`/`Io` are operational.
fn rebuild_error_to_app(err: RebuildError) -> AppError {
    match err {
        RebuildError::Rejected(reject) => SelfFixPolicy::rebuild_rejected(reject_reason(&reject)),
        RebuildError::EmptyChange => SelfFixPolicy::empty_change(),
        RebuildError::Git(_) => SelfFixPolicy::git_step_failed("rebuild"),
        RebuildError::Io(_) => SelfFixPolicy::git_step_failed("rebuild"),
    }
}

/// Run the git-heavy half of the PR Bridge with NO database access:
///
/// 1. Prepare a fresh server-owned clone dir (remove any stale dir first).
/// 2. `git clone` the token-bearing origin into it.
/// 3. `rebuild_branch` the vetted `/workspace` content onto `base_sha`.
/// 4. Sensitive-path check → choose `review_status`.
/// 5. `git push --force` the rebuilt branch back to origin.
/// 6. Open a draft PR.
///
/// The token-bearing remote URL is fetched from `provider.authed_remote_url()`
/// and passed straight to `git` as a clone argument; it is NEVER logged.
/// The token is visible in `ps` for the duration of the clone invocation and
/// is accepted because it is a short-lived installation token and the server
/// process is trusted. Returns the opened PR and the selected review status;
/// the caller persists them.
///
/// The push uses `--force` because `agent/<task-id>` is exclusively owned and
/// written by this bridge for this task: on a retry the re-clone yields a new
/// sibling commit (different SHA, same base) and a plain push would be rejected
/// non-fast-forward. Force-pushing the owned branch is the correct and intended
/// behaviour.
#[allow(dead_code)]
pub async fn run_pr_bridge<G: GitProvider + ?Sized>(
    provider: &G,
    task_id: Uuid,
    base_sha: &str,
    workspace_project_dir: &Path,
    commit_message: &str,
    pr_title: &str,
    pr_body: &str,
    limits: &ImportLimits,
) -> AppResult<BridgeResult> {
    let branch = branch_name(task_id);
    let clone_dir = clone_dir_for(task_id);

    // 1. Ephemeral per-task clone dir: remove any stale dir, recreate parent.
    if clone_dir.exists() {
        tokio::fs::remove_dir_all(&clone_dir).await.map_err(|_| SelfFixPolicy::git_step_failed("prepare_clone_dir"))?;
    }
    if let Some(parent) = clone_dir.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|_| SelfFixPolicy::git_step_failed("prepare_clone_dir"))?;
    }

    // Arm the cleanup guard BEFORE the clone so that any subsequent `?` early
    // return (clone, rebuild, push, PR) also wipes the token-bearing clone dir.
    // Held until the end of the function; its `Drop` removes the dir on success
    // and on every error path.
    let _clone_dir_guard = CloneDirGuard { dir: clone_dir.clone() };

    // 2. Clone the token-bearing origin into the server-owned clone dir.
    let authed_url = provider.authed_remote_url().await?;
    run_git_secret(
        None,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "clone",
            "--no-tags",
            "--depth",
            "50",
            authed_url.as_str(),
            clone_dir.to_str().ok_or_else(|| SelfFixPolicy::git_step_failed("clone"))?,
        ],
        "clone",
    )
    .await?;
    // The token must not linger in the clone's git config / packed-refs origin
    // beyond what we need. We drop `authed_url` here; push reuses the configured
    // origin (which git stored), so we never re-log or re-handle the token.
    drop(authed_url);

    // 3. Rebuild the vetted /workspace change onto base_sha in the clone.
    let outcome = rebuild_branch(
        &clone_dir,
        base_sha,
        workspace_project_dir,
        &branch,
        commit_message,
        AUTHOR_NAME,
        AUTHOR_EMAIL,
        limits,
    )
    .await
    .map_err(rebuild_error_to_app)?;

    // 4. Sensitive-path circuit breaker → review status.
    let review_status =
        if SensitivePathPolicy::touches_sensitive(&outcome.changed_files) { SENSITIVE_BLOCKED } else { IN_REVIEW };

    // 5. Force-push the rebuilt branch back to origin. --force is required because
    //    each rebuild produces a new sibling commit (different SHA, same base); a
    //    plain push after a partial-success retry would be rejected non-fast-forward.
    //    `agent/<task-id>` is exclusively owned by this bridge for this task, so
    //    replacing it is correct and intended.
    run_git_secret(
        Some(&clone_dir),
        &["-c", "core.hooksPath=/dev/null", "push", "--force", "origin", &branch],
        "push",
    )
    .await?;

    // 6. Open the draft PR.
    let pr = provider.create_draft_pr(&branch, BASE_BRANCH, pr_title, pr_body).await?;

    Ok(BridgeResult { pr, review_status })
}

/// Run `git` for a step whose args MAY contain a token-bearing URL (`clone`),
/// or run in the clone whose `origin` carries a token (`push`). On failure we
/// surface ONLY the static `stage` label — never the args, the URL, or stderr,
/// any of which could leak the embedded installation token.
///
/// Hardening mirrors `rebuild::run_git`: neutralize global/system git config so
/// no planted filter/hook/alias can run, and reap a timed-out child.
async fn run_git_secret(dir: Option<&Path>, args: &[&str], stage: &'static str) -> AppResult<()> {
    let mut cmd = Command::new("git");
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    cmd.args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0") // never block on an interactive auth prompt
        .kill_on_drop(true);

    let output = match tokio::time::timeout(GIT_TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => out,
        // Do NOT include the error / args here: the clone args carry the token.
        Ok(Err(_)) => return Err(SelfFixPolicy::git_step_failed(stage)),
        Err(_) => return Err(SelfFixPolicy::git_step_failed(stage)),
    };
    if !output.status.success() {
        // stderr from a failed clone/push can echo the remote URL (token); drop it.
        return Err(SelfFixPolicy::git_step_failed(stage));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lim() -> ImportLimits {
        ImportLimits::default()
    }

    #[test]
    fn branch_name_is_deterministic_per_task() {
        let id = Uuid::nil();
        assert_eq!(branch_name(id), "agent/00000000-0000-0000-0000-000000000000");
        // Same id → same branch (idempotency key).
        assert_eq!(branch_name(id), branch_name(id));
    }

    #[test]
    fn clone_dir_honors_env_override_and_default() {
        let id = Uuid::nil();
        // Default root.
        unsafe {
            std::env::remove_var("SELF_FIX_WORK_DIR");
        }
        assert_eq!(clone_dir_for(id), Path::new("/tmp/agentforge-selffix").join(id.to_string()));
        // Overridden root.
        unsafe {
            std::env::set_var("SELF_FIX_WORK_DIR", "/var/tmp/sf");
        }
        assert_eq!(clone_dir_for(id), Path::new("/var/tmp/sf").join(id.to_string()));
        unsafe {
            std::env::remove_var("SELF_FIX_WORK_DIR");
        }
    }

    #[test]
    fn rejected_rebuild_maps_to_visible_policy_error_not_pr() {
        // A symlink rejection becomes the `rebuild_rejected` policy error.
        let err = rebuild_error_to_app(RebuildError::Rejected(ImportReject::Symlink("evil".into())));
        let body = err.to_string();
        // It must be a task-failing validation, not a transport error.
        assert!(
            matches!(err.kind, agentforge_core::ErrorKind::ValidationWithCode { .. }),
            "rejection must map to ValidationWithCode, got {body}"
        );
    }

    #[test]
    fn empty_change_maps_to_visible_empty_change_error() {
        let err = rebuild_error_to_app(RebuildError::EmptyChange);
        assert!(
            matches!(err.kind, agentforge_core::ErrorKind::ValidationWithCode { code, .. } if code == "errors.self_fix.empty_change"),
            "empty change must map to the empty_change code"
        );
    }

    #[test]
    fn operational_git_io_errors_map_to_unavailable_not_validation() {
        // A git/io failure during rebuild is operational (retryable), not a
        // policy rejection — it must NOT look like a validation error.
        let git_err = rebuild_error_to_app(RebuildError::Git("boom".into()));
        assert!(matches!(git_err.kind, agentforge_core::ErrorKind::Unavailable(_)));
        let io_err = rebuild_error_to_app(RebuildError::Io("boom".into()));
        assert!(matches!(io_err.kind, agentforge_core::ErrorKind::Unavailable(_)));
    }

    #[test]
    fn reject_reason_never_panics_for_any_variant() {
        // Cheap exhaustiveness guard: every variant yields a non-empty summary.
        let variants = [
            ImportReject::Symlink("a".into()),
            ImportReject::Gitlink("b".into()),
            ImportReject::SpecialFile("sp".into()),
            ImportReject::EscapesRoot("c".into()),
            ImportReject::DotGit("d".into()),
            ImportReject::OversizeFile("e".into()),
            ImportReject::ChurnCapExceeded { changed: 9, cap: 1 },
            ImportReject::DeletionCapExceeded { deleted: 9, cap: 1 },
        ];
        for v in &variants {
            assert!(!reject_reason(v).is_empty());
        }
        let _ = lim(); // keep helper referenced
    }
}
