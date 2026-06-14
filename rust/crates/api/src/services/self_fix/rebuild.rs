//! Local rebuild core of the self-fix PR Bridge.
//!
//! The untrusted agent edited files in `/workspace`. The server can read those
//! files as plain bytes, but it must NEVER run `git` against the workspace's
//! `.git` — a planted local git config (filters, hooks) would execute attacker
//! code under our process. So instead of trusting the agent's git, we REBUILD:
//!
//! 1. Start from a clean, server-owned clone already checked out at a trusted
//!    base commit (objects for `base_sha` already present).
//! 2. Mirror ONLY vetted file content from `/workspace` onto it, classifying
//!    every entry through [`classify_entry`] (symlinks, gitlinks, `.git`,
//!    oversize, path escapes are hard rejects — no partial import).
//! 3. Produce a single server-authored commit on a fresh branch, then re-check
//!    the staged tree's object modes as defense in depth (no `120000`/`160000`
//!    mode may reach the tree).
//!
//! This module does only the local-git part: no GitHub, no network. The next
//! milestone clones from GitHub and pushes the produced branch.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use tokio::process::Command;

use crate::services::self_fix::import::{check_caps, classify_entry, ImportLimits, ImportReject};

/// Directory names we never descend into while mirroring the agent's workspace.
/// These are build artifacts / dependency trees / VCS metadata that are either
/// huge or never part of a source change, and `.git` in particular must never
/// be read as content.
const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "dist", "build", ".next", ".venv", ".turbo"];

/// Per-`git` invocation wall-clock budget. The clone is local and small, so any
/// invocation that runs longer than this is almost certainly wedged.
const GIT_TIMEOUT: Duration = Duration::from_secs(120);

/// Result of a successful rebuild: the new server-authored head commit and the
/// repo-root-relative paths that changed (added/modified/deleted) in it.
#[allow(dead_code)]
#[derive(Debug)]
pub struct RebuildOutcome {
    pub head_sha: String,
    /// Repo-root-relative, as reported by `git diff --cached --name-only`.
    pub changed_files: Vec<String>,
}

/// Why a rebuild failed. `Rejected` carries the trust-boundary rejection from
/// the import validator; the others are operational failures.
#[allow(dead_code)]
#[derive(Debug)]
pub enum RebuildError {
    /// A vetted-content rejection (symlink, gitlink, `.git`, oversize, escape,
    /// or a churn/deletion cap). Hard fail — nothing is committed.
    Rejected(ImportReject),
    /// The agent's change is identical to the base tree; there is nothing to PR.
    EmptyChange,
    /// A `git` command exited non-zero (stderr summarized). There are no tokens
    /// in this code path, so the summary is safe to surface.
    Git(String),
    /// A filesystem operation failed (read, copy, mkdir, remove).
    Io(String),
}

impl From<ImportReject> for RebuildError {
    fn from(reject: ImportReject) -> Self {
        RebuildError::Rejected(reject)
    }
}

/// Rebuild the agent's change onto `base_sha` inside an existing server-owned clone.
///
/// - `clone_dir`: a git working clone whose `origin`/objects already contain
///   `base_sha`. All git runs against this server-owned tree, never the workspace.
/// - `workspace_dir`: the untrusted agent project dir, read as plain files. Its
///   `.git` is ignored entirely.
///
/// Produces branch `branch_name` with ONE server-authored commit and returns the
/// new head sha. Any trust-boundary violation is a hard reject before commit.
#[allow(dead_code)]
pub async fn rebuild_branch(
    clone_dir: &Path,
    base_sha: &str,
    workspace_dir: &Path,
    branch_name: &str,
    commit_message: &str,
    author_name: &str,
    author_email: &str,
    limits: &ImportLimits,
) -> Result<RebuildOutcome, RebuildError> {
    // 1. Check out a fresh branch at the trusted base commit.
    run_git(clone_dir, &["-c", "core.hooksPath=/dev/null", "checkout", "-B", branch_name, base_sha]).await?;

    // 2. Snapshot the set of files tracked at base so we can detect deletions.
    // `-c core.quotePath=false -z`: disable C-style quoting for non-ASCII paths and
    // emit NUL-delimited output so the verbatim bytes match the filesystem walk keys.
    let ls_files =
        run_git(clone_dir, &["-c", "core.hooksPath=/dev/null", "-c", "core.quotePath=false", "ls-files", "-z"]).await?;
    let base_files: HashSet<String> = ls_files
        .stdout
        .split(|&b| b == b'\0')
        .filter(|s| !s.is_empty())
        .filter_map(|s| String::from_utf8(s.to_vec()).ok())
        .collect();

    // 3. Walk the untrusted workspace, validate every entry, copy vetted files.
    let mut present: HashSet<String> = HashSet::new();
    mirror_workspace(workspace_dir, clone_dir, limits, &mut present)?;

    // 4. Deletions: a base-tracked file the agent removed from the workspace.
    for f in &base_files {
        if !present.contains(f) && !workspace_dir.join(f).exists() {
            let target = clone_dir.join(f);
            match std::fs::remove_file(&target) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(RebuildError::Io(format!("remove {}: {e}", target.display()))),
            }
        }
    }

    // 5. Stage everything. `add -A` respects the base tree's `.gitignore`, so any
    //    ignored files we may have copied are not staged.
    run_git(clone_dir, &["-c", "core.hooksPath=/dev/null", "add", "-A"]).await?;

    // 6. Empty check on the staged name list.
    let staged = run_git(clone_dir, &["-c", "core.hooksPath=/dev/null", "diff", "--cached", "--name-only"]).await?;
    let changed_files: Vec<String> =
        String::from_utf8_lossy(&staged.stdout).lines().filter(|l| !l.is_empty()).map(|l| l.to_string()).collect();
    if changed_files.is_empty() {
        return Err(RebuildError::EmptyChange);
    }

    // 7. Aggregate caps.
    let deleted = run_git(
        clone_dir,
        &["-c", "core.hooksPath=/dev/null", "diff", "--cached", "--diff-filter=D", "--name-only"],
    )
    .await?;
    let deletions = String::from_utf8_lossy(&deleted.stdout).lines().filter(|l| !l.is_empty()).count();
    check_caps(changed_files.len(), deletions, limits)?;

    // 8. Object/mode re-check (defense in depth). The dst mode is the second
    //    octal field on each `--raw` line; reject any symlink (120000) or
    //    gitlink (160000) that slipped through.
    let raw = run_git(clone_dir, &["-c", "core.hooksPath=/dev/null", "diff", "--cached", "--raw"]).await?;
    reject_unsafe_modes(&String::from_utf8_lossy(&raw.stdout))?;

    // 9. Server-authored commit. Identity is supplied per-invocation (`-c`) so we
    //    never depend on or mutate any global git config; `--no-verify` skips any
    //    hooks even though `core.hooksPath=/dev/null` already neutered them.
    run_git(
        clone_dir,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            &format!("user.name={author_name}"),
            "-c",
            &format!("user.email={author_email}"),
            "commit",
            "--no-verify",
            "-m",
            commit_message,
        ],
    )
    .await?;

    // 10. Resolve the new head.
    let head = run_git(clone_dir, &["-c", "core.hooksPath=/dev/null", "rev-parse", "HEAD"]).await?;
    let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();

    Ok(RebuildOutcome { head_sha, changed_files })
}

/// Recursively mirror `src_root` (untrusted) into `dst_root` (server-owned),
/// validating every entry. Hard-rejects on the first unsafe entry — no partial
/// import is left behind beyond files copied before the reject, which the caller
/// discards by never committing.
fn mirror_workspace(
    src_root: &Path,
    dst_root: &Path,
    limits: &ImportLimits,
    present: &mut HashSet<String>,
) -> Result<(), RebuildError> {
    // Iterative DFS so we control descent (skip dirs) precisely.
    let mut stack: Vec<PathBuf> = vec![src_root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| RebuildError::Io(format!("read_dir {}: {e}", dir.display())))?;

        for entry in entries {
            let entry = entry.map_err(|e| RebuildError::Io(format!("dir entry under {}: {e}", dir.display())))?;
            let path = entry.path();

            // symlink_metadata so we classify the LINK, not its target.
            let meta = std::fs::symlink_metadata(&path)
                .map_err(|e| RebuildError::Io(format!("symlink_metadata {}: {e}", path.display())))?;
            let file_type = meta.file_type();
            let is_symlink = file_type.is_symlink();

            let rel = match path.strip_prefix(src_root) {
                Ok(r) => to_forward_slashed(r),
                Err(_) => {
                    return Err(RebuildError::Io(format!("path {} escaped workspace root", path.display())));
                }
            };

            // A directory (not a symlink) we may descend into or treat as a gitlink.
            if file_type.is_dir() && !is_symlink {
                if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && SKIP_DIRS.contains(&name)
                {
                    continue; // do not descend
                }
                // A directory that itself contains a `.git` entry is a nested
                // repo / submodule (gitlink); that's a hard reject.
                if path.join(".git").exists() {
                    classify_entry(&rel, false, true, 0, limits)?;
                    // classify_entry always errs for gitlink; unreachable past here.
                    continue;
                }
                stack.push(path);
                continue;
            }

            // Files and symlinks. Symlinks are rejected inside classify_entry
            // before any copy is attempted.
            let size = meta.len();
            classify_entry(&rel, is_symlink, false, size, limits)?;

            // Past classify_entry, `path` is not a dir and not a symlink. It is
            // STILL possibly a special file (FIFO / socket / block or char
            // device): those have size 0 and aren't dirs/symlinks, so they slip
            // through every check above, yet `std::fs::copy` on a FIFO BLOCKS
            // FOREVER waiting for a writer — hanging the bridge. Require a plain
            // regular file before any copy; reject everything else.
            if !file_type.is_file() {
                return Err(RebuildError::Rejected(ImportReject::SpecialFile(rel)));
            }

            // Regular file: copy bytes onto the clone.
            let target = dst_root.join(&rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| RebuildError::Io(format!("create_dir_all {}: {e}", parent.display())))?;
            }
            std::fs::copy(&path, &target)
                .map_err(|e| RebuildError::Io(format!("copy {} -> {}: {e}", path.display(), target.display())))?;
            present.insert(rel);
        }
    }

    Ok(())
}

/// Reject if any staged entry's destination mode is a symlink (`120000`) or a
/// gitlink (`160000`). Each `git diff --cached --raw` line looks like:
///
/// ```text
/// :100644 100755 <src-sha> <dst-sha> M\tpath
/// ```
///
/// The leading token starts with `:`; the second whitespace-delimited field is
/// the destination mode. Parsed by whitespace split so we tolerate the rename
/// (`R100`) status forms too.
fn reject_unsafe_modes(raw: &str) -> Result<(), RebuildError> {
    for line in raw.lines() {
        if line.is_empty() || !line.starts_with(':') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let _src_mode = fields.next(); // ":100644"
        let dst_mode = match fields.next() {
            Some(m) => m,
            None => continue,
        };
        // The path is everything after the status field; recover it for the error.
        let path = line.rsplit('\t').next().unwrap_or(line).to_string();
        match dst_mode {
            "120000" => return Err(RebuildError::Rejected(ImportReject::Symlink(path))),
            "160000" => return Err(RebuildError::Rejected(ImportReject::Gitlink(path))),
            _ => {}
        }
    }
    Ok(())
}

/// Forward-slash a relative path for stable, platform-independent rel keys.
fn to_forward_slashed(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/")
}

/// Run `git` in `dir` with strict environment hardening and a wall-clock timeout.
///
/// Hardening (defense in depth — the clone is server-owned, but be strict):
/// - `GIT_CONFIG_GLOBAL=/dev/null` + `GIT_CONFIG_SYSTEM=/dev/null` neutralize any
///   user/system config (filters, aliases, hooks indirection).
/// - `GIT_CONFIG_NOSYSTEM=1` belt-and-suspenders on the system config.
/// - Callers also pass `-c core.hooksPath=/dev/null` as the first args.
/// - `kill_on_drop(true)` so a timed-out child is reaped.
async fn run_git(dir: &Path, args: &[&str]) -> Result<Output, RebuildError> {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .kill_on_drop(true);

    let output = match tokio::time::timeout(GIT_TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(RebuildError::Io(format!("spawn git {:?}: {e}", args))),
        Err(_) => return Err(RebuildError::Git(format!("git {:?} timed out after {}s", args, GIT_TIMEOUT.as_secs()))),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let summary = stderr.lines().take(5).collect::<Vec<_>>().join("; ");
        let summary = if summary.is_empty() { format!("git {:?} exited non-zero", args) } else { summary };
        return Err(RebuildError::Git(summary));
    }

    Ok(output)
}
