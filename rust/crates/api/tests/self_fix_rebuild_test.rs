//! Integration tests for the self-fix local rebuild core (`rebuild_branch`).
//!
//! These tests drive the function against REAL local `git` in throwaway temp
//! dirs (no network, no GitHub). They prove the trust-boundary guarantees:
//!
//! - Test A: a symlink in the agent workspace is a HARD reject BEFORE any commit.
//! - Test B: a clean change captures modify/add/delete, respects `.gitignore`,
//!   and no symlink/gitlink mode reaches the produced tree.
//! - Test C: a no-op change yields `EmptyChange`.
//!
//! The tests gate themselves on `git` being present and skip gracefully if not.

use std::path::{Path, PathBuf};
use std::process::Command;

use agentforge_api::testing::self_fix_rebuild::{
    ImportLimits, ImportReject, RebuildError, RebuildOutcome, rebuild_branch,
};
use uuid::Uuid;

/// True if `git` is on PATH. The tests early-return (skip) otherwise.
fn git_available() -> bool {
    Command::new("git").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// A self-cleaning temp directory rooted under the system temp dir.
struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("af-selffix-rebuild-{}", Uuid::new_v4()));
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

/// Run `git` in `dir`, asserting success, returning trimmed stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        // Keep the test hermetic: do not touch global/system config or hooks.
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir parent");
    }
    std::fs::write(path, contents).expect("write file");
}

/// Build an `origin` git repo with a base commit, then a local `clone` of it.
/// Returns `(origin_dir, clone_dir, base_sha)`. The origin tracks:
///   a.txt = "base", keep.txt = "keep", .gitignore = "ignored/\n".
fn setup_origin_and_clone(root: &TempRoot) -> (PathBuf, PathBuf, String) {
    let origin = root.join("origin");
    std::fs::create_dir_all(&origin).expect("mkdir origin");
    git(&origin, &["init", "-q"]);
    // Identity per-invocation only — the host may have no global git identity.
    write_file(&origin.join("a.txt"), "base");
    write_file(&origin.join("keep.txt"), "keep");
    write_file(&origin.join(".gitignore"), "ignored/\n");
    git(&origin, &["add", "-A"]);
    git(&origin, &["-c", "user.name=Origin", "-c", "user.email=origin@example.com", "commit", "-q", "-m", "base"]);
    let base_sha = git(&origin, &["rev-parse", "HEAD"]);

    let clone = root.join("clone");
    git(&root.path, &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);

    (origin, clone, base_sha)
}

#[tokio::test]
async fn test_a_symlink_is_hard_rejected_before_commit() {
    if !git_available() {
        eprintln!("SKIP test_a_symlink_is_hard_rejected_before_commit: git not available");
        return;
    }
    let root = TempRoot::new();
    let (_origin, clone, base_sha) = setup_origin_and_clone(&root);

    // Fake agent workspace: modify a.txt, add new.txt, and plant a symlink.
    let ws = root.join("workspace");
    std::fs::create_dir_all(&ws).expect("mkdir workspace");
    write_file(&ws.join("a.txt"), "changed");
    write_file(&ws.join("new.txt"), "new");
    std::os::unix::fs::symlink("/etc/passwd", ws.join("evil")).expect("create symlink");

    let result = rebuild_branch(
        &clone,
        &base_sha,
        &ws,
        "agent/x",
        "msg",
        "Self-Fix Bot",
        "bot@example.com",
        &ImportLimits::default(),
    )
    .await;

    match result {
        Err(RebuildError::Rejected(ImportReject::Symlink(_))) => {}
        other => panic!("expected Rejected(Symlink), got {other:?}"),
    }

    // The symlink must never have been committed: the branch head, if it exists,
    // must still equal base (no server-authored commit was produced).
    // `checkout -B agent/x base_sha` moves the branch to base before the walk,
    // so a present `agent/x` head equals base; the rejection happened before commit.
    let head = Command::new("git")
        .current_dir(&clone)
        .args(["rev-parse", "agent/x"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git rev-parse");
    if head.status.success() {
        let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
        assert_eq!(head_sha, base_sha, "no commit must exist on agent/x beyond base after a symlink reject");
    }
}

#[tokio::test]
async fn test_b_clean_rebuild_diff_no_symlink_or_gitlink() {
    if !git_available() {
        eprintln!("SKIP test_b_clean_rebuild_diff_no_symlink_or_gitlink: git not available");
        return;
    }
    let root = TempRoot::new();
    let (_origin, clone, base_sha) = setup_origin_and_clone(&root);

    // Workspace: a.txt modified, new.txt added, keep.txt DELETED (absent),
    // ignored/junk.bin present (must be excluded by .gitignore). No symlink.
    let ws = root.join("workspace");
    std::fs::create_dir_all(&ws).expect("mkdir workspace");
    write_file(&ws.join("a.txt"), "changed");
    write_file(&ws.join("new.txt"), "new");
    write_file(&ws.join(".gitignore"), "ignored/\n"); // carry the gitignore forward
    write_file(&ws.join("ignored/junk.bin"), "junk");
    // keep.txt intentionally absent -> deletion.

    let outcome: RebuildOutcome = rebuild_branch(
        &clone,
        &base_sha,
        &ws,
        "agent/x",
        "self-fix change",
        "Self-Fix Bot",
        "bot@example.com",
        &ImportLimits::default(),
    )
    .await
    .expect("clean rebuild should succeed");

    // head_sha is real and advances past base.
    let head = git(&clone, &["rev-parse", "HEAD"]);
    assert_eq!(outcome.head_sha, head, "outcome head must match git HEAD");
    assert_ne!(outcome.head_sha, base_sha, "head must advance past base");

    // The committed change set: a.txt modified, new.txt added, keep.txt deleted.
    let raw = git(&clone, &["diff-tree", "-r", "--raw", &base_sha, "HEAD"]);
    let mut saw_a = false;
    let mut saw_new = false;
    let mut saw_keep_del = false;
    for line in raw.lines() {
        // ":<srcmode> <dstmode> <srcsha> <dstsha> <status>\t<path>"
        let fields: Vec<&str> = line.split_whitespace().collect();
        assert!(fields.len() >= 5, "unexpected --raw line: {line}");
        let dst_mode = fields[1];
        // No symlink/gitlink mode may reach the tree.
        assert_ne!(dst_mode, "120000", "symlink mode leaked into tree: {line}");
        assert_ne!(dst_mode, "160000", "gitlink mode leaked into tree: {line}");
        let path = line.rsplit('\t').next().unwrap_or("");
        let status = fields[4];
        match path {
            "a.txt" => {
                assert!(status.starts_with('M'), "a.txt should be modified: {line}");
                saw_a = true;
            }
            "new.txt" => {
                assert!(status.starts_with('A'), "new.txt should be added: {line}");
                saw_new = true;
            }
            "keep.txt" => {
                assert!(status.starts_with('D'), "keep.txt should be deleted: {line}");
                saw_keep_del = true;
            }
            "ignored/junk.bin" => panic!("ignored file leaked into the commit: {line}"),
            _ => {}
        }
    }
    assert!(saw_a, "expected a.txt modification in {raw}");
    assert!(saw_new, "expected new.txt addition in {raw}");
    assert!(saw_keep_del, "expected keep.txt deletion in {raw}");

    // The ignored file must not be tracked at HEAD at all.
    let tracked = git(&clone, &["ls-tree", "-r", "--name-only", "HEAD"]);
    assert!(!tracked.lines().any(|l| l == "ignored/junk.bin"), "ignored/junk.bin must not be tracked, got:\n{tracked}");

    // changed_files (from --cached --name-only) covers the three changed paths.
    let mut cf = outcome.changed_files.clone();
    cf.sort();
    assert!(cf.contains(&"a.txt".to_string()), "changed_files missing a.txt: {cf:?}");
    assert!(cf.contains(&"new.txt".to_string()), "changed_files missing new.txt: {cf:?}");
    assert!(cf.contains(&"keep.txt".to_string()), "changed_files missing keep.txt: {cf:?}");
    assert!(!cf.contains(&"ignored/junk.bin".to_string()), "changed_files must not list ignored file: {cf:?}");
}

#[tokio::test]
async fn test_c_empty_change_returns_empty_change() {
    if !git_available() {
        eprintln!("SKIP test_c_empty_change_returns_empty_change: git not available");
        return;
    }
    let root = TempRoot::new();
    let (_origin, clone, base_sha) = setup_origin_and_clone(&root);

    // Workspace identical to base.
    let ws = root.join("workspace");
    std::fs::create_dir_all(&ws).expect("mkdir workspace");
    write_file(&ws.join("a.txt"), "base");
    write_file(&ws.join("keep.txt"), "keep");
    write_file(&ws.join(".gitignore"), "ignored/\n");

    let result = rebuild_branch(
        &clone,
        &base_sha,
        &ws,
        "agent/x",
        "noop",
        "Self-Fix Bot",
        "bot@example.com",
        &ImportLimits::default(),
    )
    .await;

    match result {
        Err(RebuildError::EmptyChange) => {}
        other => panic!("expected EmptyChange, got {other:?}"),
    }
}

/// True if `mkfifo` is on PATH (used to plant a FIFO in the fake workspace).
fn mkfifo_available() -> bool {
    Command::new("mkfifo").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

#[tokio::test]
async fn test_e_fifo_is_hard_rejected_and_does_not_hang() {
    if !git_available() {
        eprintln!("SKIP test_e_fifo_is_hard_rejected_and_does_not_hang: git not available");
        return;
    }
    if !mkfifo_available() {
        eprintln!("SKIP test_e_fifo_is_hard_rejected_and_does_not_hang: mkfifo not available");
        return;
    }
    let root = TempRoot::new();
    let (_origin, clone, base_sha) = setup_origin_and_clone(&root);

    // Fake agent workspace: a benign edit plus a FIFO. Without the special-file
    // gate, `std::fs::copy` on the FIFO blocks forever waiting for a writer.
    let ws = root.join("workspace");
    std::fs::create_dir_all(&ws).expect("mkdir workspace");
    write_file(&ws.join("a.txt"), "changed");
    let fifo = ws.join("pipe");
    let status = Command::new("mkfifo").arg(&fifo).status().expect("spawn mkfifo");
    assert!(status.success(), "mkfifo must create the FIFO");

    // Bound the whole call: a regression (a hang) trips the timeout and FAILS
    // the test loudly rather than wedging the suite forever.
    let limits = ImportLimits::default();
    let call = rebuild_branch(&clone, &base_sha, &ws, "agent/fifo", "msg", "Self-Fix Bot", "bot@example.com", &limits);
    let result = tokio::time::timeout(std::time::Duration::from_secs(20), call)
        .await
        .expect("rebuild_branch must NOT hang on a FIFO — it must reject quickly");

    match result {
        Err(RebuildError::Rejected(ImportReject::SpecialFile(_))) => {}
        other => panic!("expected Rejected(SpecialFile), got {other:?}"),
    }
}

#[tokio::test]
async fn test_d_non_ascii_filename_deletion_is_captured() {
    if !git_available() {
        eprintln!("SKIP test_d_non_ascii_filename_deletion_is_captured: git not available");
        return;
    }
    let root = TempRoot::new();

    // Build an origin that also tracks a non-ASCII file `café.txt` (UTF-8).
    let origin = root.join("origin");
    std::fs::create_dir_all(&origin).expect("mkdir origin");
    git(&origin, &["init", "-q"]);
    write_file(&origin.join("a.txt"), "base");
    write_file(&origin.join("keep.txt"), "keep");
    write_file(&origin.join(".gitignore"), "ignored/\n");
    write_file(&origin.join("café.txt"), "café content");
    git(&origin, &["add", "-A"]);
    git(&origin, &["-c", "user.name=Origin", "-c", "user.email=origin@example.com", "commit", "-q", "-m", "base"]);
    let base_sha = git(&origin, &["rev-parse", "HEAD"]);

    let clone = root.join("clone");
    git(&root.path, &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);

    // Agent workspace: modify a.txt, omit `café.txt` (agent "deleted" it), keep keep.txt.
    // Without the fix, the quoted key `"caf\303\251.txt"` never matches the walk's
    // `café.txt` key → the deletion is silently dropped from the commit.
    let ws = root.join("workspace");
    std::fs::create_dir_all(&ws).expect("mkdir workspace");
    write_file(&ws.join("a.txt"), "changed by agent");
    write_file(&ws.join("keep.txt"), "keep");
    write_file(&ws.join(".gitignore"), "ignored/\n");
    // `café.txt` intentionally absent — the agent deleted it.

    let outcome: RebuildOutcome = rebuild_branch(
        &clone,
        &base_sha,
        &ws,
        "agent/d",
        "delete café.txt",
        "Self-Fix Bot",
        "bot@example.com",
        &ImportLimits::default(),
    )
    .await
    .expect("clean rebuild should succeed");

    // Verify `café.txt` appears as DELETED in the produced commit.
    // `-c core.quotePath=false` must come before the subcommand in git's arg order.
    let raw = git(&clone, &["-c", "core.quotePath=false", "diff-tree", "-r", "--raw", &base_sha, "HEAD"]);
    let mut saw_cafe_deleted = false;
    let mut saw_a_modified = false;
    for line in raw.lines() {
        // ":<srcmode> <dstmode> <srcsha> <dstsha> <status>\t<path>"
        let tab_pos = line.find('\t').unwrap_or(line.len());
        let path = &line[tab_pos.saturating_add(1)..];
        let fields: Vec<&str> = line[..tab_pos].split_whitespace().collect();
        if fields.len() < 5 {
            continue;
        }
        let status = fields[4];
        if path == "café.txt" {
            assert!(status.starts_with('D'), "café.txt should be deleted but got status {status} in: {line}");
            saw_cafe_deleted = true;
        }
        if path == "a.txt" {
            assert!(status.starts_with('M'), "a.txt should be modified: {line}");
            saw_a_modified = true;
        }
    }
    assert!(saw_cafe_deleted, "café.txt deletion missing from commit diff-tree output:\n{raw}");
    assert!(saw_a_modified, "a.txt modification missing from commit diff-tree output:\n{raw}");

    // The outcome head must advance past base (a real commit was produced).
    let head = git(&clone, &["rev-parse", "HEAD"]);
    assert_eq!(outcome.head_sha, head, "outcome head must match git HEAD");
    assert_ne!(outcome.head_sha, base_sha, "head must advance past base");
}
