//! Vetted file-content import from the untrusted /workspace into a server-owned worktree.
//! No git is ever run against /workspace; the Bridge does a filesystem walk and asks this
//! module to classify each entry. Rejections are hard (the task fails), never silent skips.

use std::path::{Component, Path};

#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
pub enum ImportReject {
    Symlink(String),
    Gitlink(String),
    /// A non-regular, non-dir, non-symlink entry: FIFO/socket/block/char device.
    /// Copying one (e.g. `std::fs::copy` on a FIFO) would block forever waiting
    /// for a peer, so these are a hard reject.
    SpecialFile(String),
    EscapesRoot(String),
    DotGit(String),
    OversizeFile(String),
    ChurnCapExceeded { changed: usize, cap: usize },
    DeletionCapExceeded { deleted: usize, cap: usize },
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ImportLimits {
    pub max_file_bytes: u64,
    pub max_changed_files: usize,
    pub max_deletions: usize,
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self { max_file_bytes: 2 * 1024 * 1024, max_changed_files: 400, max_deletions: 200 }
    }
}

/// Classify ONE relative path for import. `is_symlink`/`is_gitlink`/`size` are passed in so
/// this stays pure (the Bridge supplies them from its filesystem walk). `rel_path` is
/// repo-root-relative, forward-slashed.
#[allow(dead_code)]
pub(crate) fn classify_entry(
    rel_path: &str,
    is_symlink: bool,
    is_gitlink: bool,
    size: u64,
    limits: &ImportLimits,
) -> Result<(), ImportReject> {
    if is_symlink {
        return Err(ImportReject::Symlink(rel_path.to_string()));
    }
    if is_gitlink {
        return Err(ImportReject::Gitlink(rel_path.to_string()));
    }
    if path_escapes(rel_path) {
        return Err(ImportReject::EscapesRoot(rel_path.to_string()));
    }
    if rel_path == ".git" || rel_path.starts_with(".git/") {
        return Err(ImportReject::DotGit(rel_path.to_string()));
    }
    if size > limits.max_file_bytes {
        return Err(ImportReject::OversizeFile(rel_path.to_string()));
    }
    Ok(())
}

/// Reject absolute paths, or any `..` / root component after normalization.
#[allow(dead_code)]
fn path_escapes(rel: &str) -> bool {
    if rel.is_empty() || rel.starts_with('/') {
        return true;
    }
    Path::new(rel)
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
}

/// Check aggregate caps over the full change set (call after per-entry classification).
#[allow(dead_code)]
pub(crate) fn check_caps(changed: usize, deleted: usize, limits: &ImportLimits) -> Result<(), ImportReject> {
    if changed > limits.max_changed_files {
        return Err(ImportReject::ChurnCapExceeded { changed, cap: limits.max_changed_files });
    }
    if deleted > limits.max_deletions {
        return Err(ImportReject::DeletionCapExceeded { deleted, cap: limits.max_deletions });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lim() -> ImportLimits { ImportLimits::default() }

    #[test] fn rejects_symlink() {
        assert_eq!(classify_entry("a.rs", true, false, 10, &lim()), Err(ImportReject::Symlink("a.rs".into())));
    }
    #[test] fn rejects_gitlink() {
        assert_eq!(classify_entry("sub", false, true, 10, &lim()), Err(ImportReject::Gitlink("sub".into())));
    }
    #[test] fn rejects_parent_escape() {
        assert!(matches!(classify_entry("../etc/passwd", false, false, 1, &lim()), Err(ImportReject::EscapesRoot(_))));
    }
    #[test] fn rejects_absolute() {
        assert!(matches!(classify_entry("/etc/passwd", false, false, 1, &lim()), Err(ImportReject::EscapesRoot(_))));
    }
    #[test] fn rejects_empty() {
        assert!(matches!(classify_entry("", false, false, 1, &lim()), Err(ImportReject::EscapesRoot(_))));
    }
    #[test] fn rejects_dotgit_dir() {
        assert!(matches!(classify_entry(".git/config", false, false, 1, &lim()), Err(ImportReject::DotGit(_))));
        assert!(matches!(classify_entry(".git", false, false, 1, &lim()), Err(ImportReject::DotGit(_))));
    }
    #[test] fn rejects_oversize() {
        assert!(matches!(classify_entry("big.bin", false, false, 9_000_000, &lim()), Err(ImportReject::OversizeFile(_))));
    }
    #[test] fn accepts_regular_nested_file() {
        assert_eq!(classify_entry("rust/crates/api/src/x.rs", false, false, 100, &lim()), Ok(()));
    }
    #[test] fn nested_dotgit_path_component_is_allowed_if_not_top_level() {
        // e.g. a file literally named .gitignore is fine; only the .git dir is blocked.
        assert_eq!(classify_entry(".gitignore", false, false, 100, &lim()), Ok(()));
        assert_eq!(classify_entry("docs/.gitkeep", false, false, 100, &lim()), Ok(()));
    }
    #[test] fn churn_cap() {
        assert!(matches!(check_caps(401, 0, &lim()), Err(ImportReject::ChurnCapExceeded { .. })));
        assert!(check_caps(400, 200, &lim()).is_ok());
    }
    #[test] fn deletion_cap() {
        assert!(matches!(check_caps(1, 201, &lim()), Err(ImportReject::DeletionCapExceeded { .. })));
    }
}
