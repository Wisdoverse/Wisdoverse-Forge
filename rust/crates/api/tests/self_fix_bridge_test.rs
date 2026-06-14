//! Integration tests for the self-fix PR Bridge core (`run_pr_bridge`).
//!
//! These drive the full clone → rebuild → sensitive-check → push → draft-PR
//! flow against a REAL local `git` and a LOCAL `file://` origin (no network, no
//! real GitHub). A fake `GitProvider` injects the base SHA, the local origin URL
//! (in place of a token-bearing GitHub URL), and a synthetic draft PR.
//!
//! They prove, without a database:
//!   - Happy path: a benign change rebuilds onto base, the branch is PUSHED to
//!     the local origin with the rebuilt commit, and review_status == in_review.
//!   - Sensitive path: a change touching `rust/crates/auth/...` yields
//!     review_status == sensitive_blocked (still pushed; it's a CODEOWNERS route).
//!   - Empty change: a no-op change fails with a visible error and NOTHING is
//!     pushed (no agent/<id> ref on origin).
//!
//! The DB-touching wrapper (`SelfFixService::open_pr`) is exercised at the route
//! milestone; here we cover the trust-boundary-critical git half end-to-end.
//!
//! Gated on `SELF_FIX_IT=1` (and `git` present) so it does not run in the
//! default `cargo test` pass.

use std::path::{Path, PathBuf};
use std::process::Command;

use agentforge_api::testing::self_fix_bridge::{run_pr_bridge, GitProvider, ImportLimits, OpenedDraftPr};
use agentforge_core::AppResult;
use uuid::Uuid;

/// `SELF_FIX_WORK_DIR` is process-global, but the default `cargo test` harness
/// runs every `#[tokio::test]` in this file concurrently on its own thread. Each
/// test sets that var to its own temp work dir and clears it on the way out, so
/// without serialization one test's `remove_var`/`set_var` clobbers another's
/// mid-run — the bridge then resolves the wrong clone dir and the run fails
/// non-deterministically. Hold this lock for the whole body of any test that
/// mutates `SELF_FIX_WORK_DIR` so those tests run one at a time.
///
/// This is a `tokio::sync::Mutex` (not `std::sync::Mutex`) on purpose: each test
/// holds the guard across the `run_pr_bridge` `.await`s, and an async-aware mutex
/// keeps that clippy-clean (`await_holding_lock`) and sound. The mutex is never
/// poisoned by a panic, so there is nothing to recover.
static SELF_FIX_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Acquire the env-serialization lock for the duration of a test body.
async fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
    SELF_FIX_ENV_LOCK.lock().await
}

fn it_enabled() -> bool {
    std::env::var("SELF_FIX_IT").as_deref() == Ok("1")
}

fn git_available() -> bool {
    Command::new("git").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// Self-cleaning temp dir under the system temp dir.
struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("af-selffix-bridge-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create temp root");
        Self { path }
    }
    fn join(&self, p: &str) -> PathBuf {
        self.path.join(p)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// `git` that may fail; returns (success, trimmed stdout).
fn git_try(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir parent");
    }
    std::fs::write(path, contents).expect("write file");
}

/// Build a NON-bare `origin` repo that can receive pushes to its current branch.
/// Returns `(origin_dir, base_sha)`. The origin tracks a few base files and a
/// nested path `rust/crates/auth/src/jwt.rs` so a "sensitive" change is possible.
fn setup_origin(root: &TempRoot) -> (PathBuf, String) {
    let origin = root.join("origin");
    std::fs::create_dir_all(&origin).expect("mkdir origin");
    git(&origin, &["init", "-q", "-b", "main"]);
    // Allow pushes into the checked-out branch of this non-bare origin.
    git(&origin, &["config", "receive.denyCurrentBranch", "updateInstead"]);
    write_file(&origin.join("README.md"), "base readme\n");
    write_file(&origin.join("rust/crates/auth/src/jwt.rs"), "// base jwt\n");
    write_file(&origin.join("src/app/feature.ts"), "export const x = 1;\n");
    write_file(&origin.join(".gitignore"), "ignored/\n");
    git(&origin, &["add", "-A"]);
    git(&origin, &["-c", "user.name=Origin", "-c", "user.email=origin@example.com", "commit", "-q", "-m", "base"]);
    let base_sha = git(&origin, &["rev-parse", "HEAD"]);
    (origin, base_sha)
}

/// A fake `GitProvider` backed by a local `file://` origin. `default_branch_sha`
/// returns the pinned base; `authed_remote_url` returns the local origin path
/// (stands in for the token-bearing GitHub URL); `create_draft_pr` records the
/// call and returns a synthetic PR whose head_sha is the just-pushed branch tip.
struct FakeGitProvider {
    origin: PathBuf,
    base_sha: String,
    pr_calls: std::sync::Mutex<Vec<(String, String, String)>>, // (head, base, title)
}

impl FakeGitProvider {
    fn new(origin: PathBuf, base_sha: String) -> Self {
        Self { origin, base_sha, pr_calls: std::sync::Mutex::new(Vec::new()) }
    }
}

#[async_trait::async_trait]
impl GitProvider for FakeGitProvider {
    async fn default_branch_sha(&self) -> AppResult<String> {
        Ok(self.base_sha.clone())
    }

    async fn authed_remote_url(&self) -> AppResult<String> {
        // Local origin path stands in for the token-bearing GitHub remote.
        Ok(self.origin.to_string_lossy().to_string())
    }

    async fn create_draft_pr(
        &self,
        head_branch: &str,
        base: &str,
        title: &str,
        _body: &str,
    ) -> AppResult<OpenedDraftPr> {
        // By the time the PR is opened the branch has been pushed, so we can read
        // its tip from the origin to populate head_sha.
        let head_sha = git(&self.origin, &["rev-parse", head_branch]);
        self.pr_calls.lock().unwrap().push((head_branch.to_string(), base.to_string(), title.to_string()));
        Ok(OpenedDraftPr { number: 4242, html_url: format!("file://pr/{head_branch}"), head_sha })
    }
}

#[tokio::test]
async fn happy_path_pushes_rebuilt_branch_and_selects_in_review() {
    if !it_enabled() {
        eprintln!("SKIP happy_path_pushes_rebuilt_branch_and_selects_in_review: set SELF_FIX_IT=1 to run");
        return;
    }
    if !git_available() {
        eprintln!("SKIP: git not available");
        return;
    }
    // Serialize against sibling tests: `SELF_FIX_WORK_DIR` is process-global.
    let _env = env_lock().await;

    let root = TempRoot::new();
    let (origin, base_sha) = setup_origin(&root);

    // Point the clone work-dir at our temp root so we don't litter /tmp.
    unsafe {
        std::env::set_var("SELF_FIX_WORK_DIR", root.join("work").to_string_lossy().to_string());
    }

    // Fake /workspace project dir: a BENIGN change (README edit + new file).
    let ws = root.join("workspace");
    std::fs::create_dir_all(&ws).expect("mkdir workspace");
    write_file(&ws.join("README.md"), "changed readme\n");
    write_file(&ws.join("rust/crates/auth/src/jwt.rs"), "// base jwt\n"); // unchanged
    write_file(&ws.join("src/app/feature.ts"), "export const x = 2;\n"); // benign change
    write_file(&ws.join(".gitignore"), "ignored/\n");

    let task_id = Uuid::new_v4();
    let provider = FakeGitProvider::new(origin.clone(), base_sha.clone());

    let result = run_pr_bridge(
        &provider,
        task_id,
        &base_sha,
        &ws,
        "self-fix: benign change",
        "[self-fix] benign change",
        "body",
        &ImportLimits::default(),
    )
    .await
    .expect("happy path should succeed");

    assert_eq!(result.review_status, "in_review", "benign change must be in_review");
    assert_eq!(result.pr.number, 4242);

    // The branch must exist on origin with the rebuilt commit (advanced past base).
    let branch = format!("agent/{task_id}");
    let (ok, pushed_sha) = git_try(&origin, &["rev-parse", &branch]);
    assert!(ok, "agent branch must have been pushed to origin");
    assert_ne!(pushed_sha, base_sha, "pushed head must advance past base");
    assert_eq!(pushed_sha, result.pr.head_sha, "PR head_sha must match the pushed branch tip");

    // The pushed commit's diff must contain the benign change, NOT a symlink/gitlink.
    let raw = git(&origin, &["diff-tree", "-r", "--raw", &base_sha, &branch]);
    assert!(raw.contains("README.md"), "expected README.md in the pushed diff:\n{raw}");
    assert!(!raw.contains("120000"), "no symlink mode may reach the tree");

    // The token-bearing clone dir (${SELF_FIX_WORK_DIR}/<task_id>) must be gone
    // after a SUCCESSFUL run: the RAII guard wipes it on the way out so the
    // installation token in .git/config does not linger on disk.
    let clone_dir = root.join("work").join(task_id.to_string());
    assert!(!clone_dir.exists(), "clone dir must be removed after a successful run: {}", clone_dir.display());

    unsafe {
        std::env::remove_var("SELF_FIX_WORK_DIR");
    }
}

#[tokio::test]
async fn sensitive_change_selects_sensitive_blocked() {
    if !it_enabled() {
        eprintln!("SKIP sensitive_change_selects_sensitive_blocked: set SELF_FIX_IT=1 to run");
        return;
    }
    if !git_available() {
        eprintln!("SKIP: git not available");
        return;
    }
    // Serialize against sibling tests: `SELF_FIX_WORK_DIR` is process-global.
    let _env = env_lock().await;

    let root = TempRoot::new();
    let (origin, base_sha) = setup_origin(&root);
    unsafe {
        std::env::set_var("SELF_FIX_WORK_DIR", root.join("work").to_string_lossy().to_string());
    }

    // Workspace: edit a SENSITIVE file (rust/crates/auth/...).
    let ws = root.join("workspace");
    std::fs::create_dir_all(&ws).expect("mkdir workspace");
    write_file(&ws.join("README.md"), "base readme\n");
    write_file(&ws.join("rust/crates/auth/src/jwt.rs"), "// TAMPERED jwt\n"); // sensitive change
    write_file(&ws.join("src/app/feature.ts"), "export const x = 1;\n");
    write_file(&ws.join(".gitignore"), "ignored/\n");

    let task_id = Uuid::new_v4();
    let provider = FakeGitProvider::new(origin.clone(), base_sha.clone());

    let result = run_pr_bridge(
        &provider,
        task_id,
        &base_sha,
        &ws,
        "self-fix: touches auth",
        "[self-fix] touches auth",
        "body",
        &ImportLimits::default(),
    )
    .await
    .expect("sensitive change still opens a (blocked) PR");

    assert_eq!(result.review_status, "sensitive_blocked", "auth change must be sensitive_blocked");

    // It is still pushed (routed to CODEOWNERS / manual merge).
    let branch = format!("agent/{task_id}");
    let (ok, _sha) = git_try(&origin, &["rev-parse", &branch]);
    assert!(ok, "sensitive change branch is still pushed for manual review");

    unsafe {
        std::env::remove_var("SELF_FIX_WORK_DIR");
    }
}

#[tokio::test]
async fn empty_change_fails_visibly_and_pushes_nothing() {
    if !it_enabled() {
        eprintln!("SKIP empty_change_fails_visibly_and_pushes_nothing: set SELF_FIX_IT=1 to run");
        return;
    }
    if !git_available() {
        eprintln!("SKIP: git not available");
        return;
    }
    // Serialize against sibling tests: `SELF_FIX_WORK_DIR` is process-global.
    let _env = env_lock().await;

    let root = TempRoot::new();
    let (origin, base_sha) = setup_origin(&root);
    unsafe {
        std::env::set_var("SELF_FIX_WORK_DIR", root.join("work").to_string_lossy().to_string());
    }

    // Workspace identical to base → empty change.
    let ws = root.join("workspace");
    std::fs::create_dir_all(&ws).expect("mkdir workspace");
    write_file(&ws.join("README.md"), "base readme\n");
    write_file(&ws.join("rust/crates/auth/src/jwt.rs"), "// base jwt\n");
    write_file(&ws.join("src/app/feature.ts"), "export const x = 1;\n");
    write_file(&ws.join(".gitignore"), "ignored/\n");

    let task_id = Uuid::new_v4();
    let provider = FakeGitProvider::new(origin.clone(), base_sha.clone());

    let result = run_pr_bridge(
        &provider,
        task_id,
        &base_sha,
        &ws,
        "self-fix: noop",
        "[self-fix] noop",
        "body",
        &ImportLimits::default(),
    )
    .await;

    assert!(result.is_err(), "an empty change must fail visibly");

    // NOTHING was pushed: no agent/<id> ref on origin, and the PR was never opened.
    let branch = format!("agent/{task_id}");
    let (ok, _sha) = git_try(&origin, &["rev-parse", &branch]);
    assert!(!ok, "no branch must be pushed on an empty change");
    assert!(provider.pr_calls.lock().unwrap().is_empty(), "no PR must be opened on an empty change");

    // The token-bearing clone dir must ALSO be gone after a FAILING run: the RAII
    // guard fires on the early `?` return (rebuild → EmptyChange), not only on
    // success.
    let clone_dir = root.join("work").join(task_id.to_string());
    assert!(!clone_dir.exists(), "clone dir must be removed after a failing run: {}", clone_dir.display());

    unsafe {
        std::env::remove_var("SELF_FIX_WORK_DIR");
    }
}

/// Regression test: a second `run_pr_bridge` call for the same task (same branch
/// name) with a DIFFERENT workspace change must succeed (force-push) and leave the
/// origin branch pointing at the new commit.
///
/// Without `--force` the second push is rejected non-fast-forward because rebuild
/// yields a new sibling commit (different SHA, same base); the task gets stuck.
/// With `--force` it succeeds. This test verifies the fix.
#[tokio::test]
async fn retry_with_different_commit_succeeds_via_force_push() {
    if !it_enabled() {
        eprintln!("SKIP retry_with_different_commit_succeeds_via_force_push: set SELF_FIX_IT=1 to run");
        return;
    }
    if !git_available() {
        eprintln!("SKIP: git not available");
        return;
    }
    // Serialize against sibling tests: `SELF_FIX_WORK_DIR` is process-global.
    let _env = env_lock().await;

    let root = TempRoot::new();
    let (origin, base_sha) = setup_origin(&root);
    unsafe {
        std::env::set_var("SELF_FIX_WORK_DIR", root.join("work").to_string_lossy().to_string());
    }

    // The task_id is fixed; both runs use the same branch name (the idempotency key).
    let task_id = Uuid::new_v4();
    let branch = format!("agent/{task_id}");

    // --- First run: workspace change C1 ---
    let ws = root.join("workspace");
    std::fs::create_dir_all(&ws).expect("mkdir workspace");
    write_file(&ws.join("README.md"), "base readme\n");
    write_file(&ws.join("rust/crates/auth/src/jwt.rs"), "// base jwt\n");
    write_file(&ws.join("src/app/feature.ts"), "export const x = 1;\n");
    write_file(&ws.join(".gitignore"), "ignored/\n");
    // C1: add a new file "change_c1.txt"
    write_file(&ws.join("change_c1.txt"), "first run change\n");

    let provider = FakeGitProvider::new(origin.clone(), base_sha.clone());
    let result1 = run_pr_bridge(
        &provider,
        task_id,
        &base_sha,
        &ws,
        "self-fix: first run",
        "[self-fix] first run",
        "body",
        &ImportLimits::default(),
    )
    .await
    .expect("first run must succeed");

    let (ok, sha_c1) = git_try(&origin, &["rev-parse", &branch]);
    assert!(ok, "first run must have pushed the branch");
    assert_ne!(sha_c1, base_sha, "C1 must advance past base");
    assert_eq!(sha_c1, result1.pr.head_sha, "first PR head_sha must match pushed tip");

    // --- Second run: a DIFFERENT workspace change C2 (simulates a retry after a
    //     partial failure where `create_draft_pr` failed). The clone dir is
    //     removed by `run_pr_bridge` at the start of each run. We mutate the
    //     workspace so rebuild yields a sibling commit (same base, different tree).
    // C2: remove "change_c1.txt", add "change_c2.txt" instead.
    std::fs::remove_file(ws.join("change_c1.txt")).expect("remove c1 marker");
    write_file(&ws.join("change_c2.txt"), "second run change\n");

    let provider2 = FakeGitProvider::new(origin.clone(), base_sha.clone());
    let result2 = run_pr_bridge(
        &provider2,
        task_id,       // same task → same branch name
        &base_sha,
        &ws,
        "self-fix: second run (retry)",
        "[self-fix] second run (retry)",
        "body",
        &ImportLimits::default(),
    )
    .await
    .expect("second run (force-push retry) must succeed — would fail non-fast-forward without --force");

    let (ok2, sha_c2) = git_try(&origin, &["rev-parse", &branch]);
    assert!(ok2, "second run must have pushed the branch");
    // The origin branch must now point at C2, not C1.
    assert_ne!(sha_c2, sha_c1, "second push must have replaced C1 with a new sibling C2");
    assert_ne!(sha_c2, base_sha, "C2 must still advance past base");
    assert_eq!(sha_c2, result2.pr.head_sha, "second PR head_sha must match the new pushed tip");

    unsafe {
        std::env::remove_var("SELF_FIX_WORK_DIR");
    }
}
