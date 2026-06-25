//! Symlink-safe materialization of instruction images into the agent's
//! `/workspace` mount.
//!
//! The projects root is bind-mounted read-WRITE into the agent container, so a
//! prior agent run could pre-plant `.task-images` (or any path component) as a
//! symlink pointing outside the workspace. A naive `create_dir_all` + write
//! would follow it and escape (to another workspace, or to host paths). Instead
//! we walk component-by-component with `openat(O_NOFOLLOW | O_DIRECTORY)` from
//! the trusted, canonicalized projects root: a symlinked component yields
//! `ELOOP` and is rejected. Because each `openat` is relative to a verified
//! directory file descriptor (not a re-resolvable path string), the TOCTOU race
//! is defeated — an attacker swapping a component to a symlink after a check
//! cannot redirect a write that goes through the already-opened fd chain.
//!
//! The per-task subdirectory is named with the server-generated task UUID, which
//! the agent cannot predict, so it cannot pre-plant files inside it; combined
//! with `O_NOFOLLOW` on the file create, the materialized file can never be an
//! attacker-planted symlink. We use `O_TRUNC` (not `O_EXCL`) so a retry always
//! overwrites with our re-encoded bytes rather than trusting whatever is present.

use std::io::Write;
use std::os::fd::OwnedFd;
use std::path::Path;

use agentforge_core::{AppError, AppResult, ErrorKind};
use rustix::fs::{CWD, Mode, OFlags, mkdirat, openat};
use uuid::Uuid;

const TASK_IMAGES_DIR: &str = ".task-images";

fn internal(context: &str, err: impl std::fmt::Display) -> AppError {
    ErrorKind::Internal(anyhow::anyhow!("{context}: {err}")).into()
}

/// Open `name` under `parent` as a directory without following symlinks,
/// creating it first if `create` is set. A symlinked component is rejected as a
/// validation error rather than followed.
fn open_dir_nofollow(parent: &OwnedFd, name: &str, create: bool) -> AppResult<OwnedFd> {
    if create {
        match mkdirat(parent, name, Mode::RWXU) {
            Ok(()) => {}
            Err(rustix::io::Errno::EXIST) => {}
            Err(err) => return Err(internal(&format!("mkdir {name}"), err)),
        }
    }
    openat(parent, name, OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC, Mode::empty()).map_err(|err| {
        if err == rustix::io::Errno::LOOP {
            ErrorKind::Validation(format!("workspace image path component '{name}' is a symlink")).into()
        } else {
            internal(&format!("openat {name}"), err)
        }
    })
}

/// Materialize `(filename, bytes)` images into
/// `<projects_root>/.task-images/<task_id>/` symlink-safely and return the
/// container-relative paths (`/workspace/.task-images/<task_id>/<file>`).
///
/// `filename`s MUST already be sanitized by the caller (e.g. attachment-UUID
/// prefixed, no path separators). A retry that re-materializes the same task
/// overwrites any already-present file (`O_TRUNC`) with our bytes.
pub fn materialize_task_images(
    projects_root: &Path,
    task_id: Uuid,
    images: &[(String, Vec<u8>)],
) -> AppResult<Vec<String>> {
    if images.is_empty() {
        return Ok(Vec::new());
    }

    // The projects root is a trusted, server-controlled path. Canonicalize so the
    // trusted prefix has no symlinks, then open it as the walk anchor.
    let root = std::fs::canonicalize(projects_root).map_err(|err| internal("canonicalize projects root", err))?;
    let root_fd = openat(CWD, &root, OFlags::DIRECTORY | OFlags::CLOEXEC, Mode::empty())
        .map_err(|err| internal("open projects root", err))?;

    let images_fd = open_dir_nofollow(&root_fd, TASK_IMAGES_DIR, true)?;
    let task_dir = task_id.to_string();
    let task_fd = open_dir_nofollow(&images_fd, &task_dir, true)?;

    let mut paths = Vec::with_capacity(images.len());
    for (filename, bytes) in images {
        // O_TRUNC (not O_EXCL): the workspace is agent-writable, so on a retry the
        // file may already exist and could have been modified by the agent — always
        // overwrite with our re-encoded bytes. O_NOFOLLOW still rejects a symlink
        // swapped in for the file (ELOOP), preserving the escape guarantee.
        let fd = openat(
            &task_fd,
            filename.as_str(),
            OFlags::CREATE | OFlags::TRUNC | OFlags::WRONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )
        .map_err(|err| {
            if err == rustix::io::Errno::LOOP {
                ErrorKind::Validation(format!("workspace image file '{filename}' is a symlink")).into()
            } else {
                internal(&format!("create image {filename}"), err)
            }
        })?;
        let mut file = std::fs::File::from(fd);
        file.write_all(bytes).map_err(|err| internal(&format!("write image {filename}"), err))?;
        file.sync_all().map_err(|err| internal(&format!("sync image {filename}"), err))?;
        paths.push(format!("/workspace/{TASK_IMAGES_DIR}/{task_dir}/{filename}"));
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("af-img-writer-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp projects root");
        dir
    }

    #[test]
    fn materializes_images_under_task_dir() {
        let root = temp_root();
        let task_id = Uuid::new_v4();
        let images = vec![("a.png".to_string(), b"PNGDATA".to_vec()), ("b.png".to_string(), b"OTHER".to_vec())];

        let paths = materialize_task_images(&root, task_id, &images).expect("materialize");

        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], format!("/workspace/.task-images/{task_id}/a.png"));
        let on_disk = root.join(".task-images").join(task_id.to_string()).join("a.png");
        assert_eq!(std::fs::read(&on_disk).expect("read written image"), b"PNGDATA");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_symlinked_task_images_component() {
        let root = temp_root();
        // Pre-plant `.task-images` as a symlink to an outside escape target, as a
        // malicious prior agent run could.
        let escape = std::env::temp_dir().join(format!("af-escape-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&escape).expect("create escape dir");
        std::os::unix::fs::symlink(&escape, root.join(".task-images")).expect("plant symlink");

        let task_id = Uuid::new_v4();
        let result = materialize_task_images(&root, task_id, &[("evil.png".to_string(), b"x".to_vec())]);

        assert!(result.is_err(), "must reject a symlinked .task-images component");
        // Nothing was written through the symlink into the escape target.
        assert!(!escape.join(task_id.to_string()).exists(), "must not create files in the symlink escape target");

        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&escape).ok();
    }

    #[test]
    fn retry_overwrites_existing_file_with_new_bytes() {
        let root = temp_root();
        let task_id = Uuid::new_v4();
        // First materialization writes the original bytes.
        materialize_task_images(&root, task_id, &[("a.png".to_string(), b"FIRST".to_vec())]).expect("first");
        let on_disk = root.join(".task-images").join(task_id.to_string()).join("a.png");
        assert_eq!(std::fs::read(&on_disk).expect("read first"), b"FIRST");

        // A retry (e.g. after a transient dispatch failure) must overwrite with
        // our re-encoded bytes, not trust whatever is on disk.
        materialize_task_images(&root, task_id, &[("a.png".to_string(), b"SECOND".to_vec())]).expect("retry");
        assert_eq!(std::fs::read(&on_disk).expect("read retry"), b"SECOND");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_symlinked_image_file() {
        let root = temp_root();
        let task_id = Uuid::new_v4();
        // Pre-create the task dir and plant the target file as a symlink, as a
        // malicious agent could between a retry's dir creation and file write.
        let task_dir = root.join(".task-images").join(task_id.to_string());
        std::fs::create_dir_all(&task_dir).expect("mk task dir");
        let escape = std::env::temp_dir().join(format!("af-escape-file-{}", Uuid::new_v4()));
        std::os::unix::fs::symlink(&escape, task_dir.join("a.png")).expect("plant file symlink");

        let result = materialize_task_images(&root, task_id, &[("a.png".to_string(), b"x".to_vec())]);

        assert!(result.is_err(), "must reject a symlinked image file");
        assert!(!escape.exists(), "must not write through the symlink to the escape target");

        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_file(&escape).ok();
    }

    #[test]
    fn empty_images_is_noop() {
        let root = temp_root();
        assert!(materialize_task_images(&root, Uuid::new_v4(), &[]).expect("noop").is_empty());
        std::fs::remove_dir_all(&root).ok();
    }
}
