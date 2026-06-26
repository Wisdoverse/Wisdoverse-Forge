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

use agentforge_core::{AppError, AppResult};
use rustix::fs::{AtFlags, CWD, Dir, Mode, OFlags, fchmod, mkdirat, openat, unlinkat};
use uuid::Uuid;

use crate::domain::instruction_image;

const TASK_IMAGES_DIR: &str = ".task-images";

// The agent container runs under a DIFFERENT UID than the API process that
// writes here (default images: API `agentforge`, agent UID 1011), and the bind
// mount shares these inodes verbatim. So the materialized dirs must be
// other-traversable and the files other-readable, or the CLI hits "permission
// denied". These live inside the per-workspace projects root, already shared by
// every agent in that workspace, so other-read does not widen the trust
// boundary. Applied via `fchmod` after create to bypass a restrictive umask;
// the `O_NOFOLLOW`/`openat` symlink-escape protections are unaffected.
const DIR_MODE: u32 = 0o755;
const FILE_MODE: u32 = 0o644;

fn internal(context: &str, err: impl std::fmt::Display) -> AppError {
    instruction_image::workspace_write_internal(context, err)
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
    let fd = openat(parent, name, OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC, Mode::empty()).map_err(
        |err| -> AppError {
            if err == rustix::io::Errno::LOOP {
                instruction_image::symlinked_path_component(name)
            } else {
                internal(&format!("openat {name}"), err)
            }
        },
    )?;
    if create {
        // Make the dir traversable by the agent UID (mkdirat's mode is umask-masked).
        fchmod(&fd, Mode::from_bits_truncate(DIR_MODE)).map_err(|err| internal(&format!("chmod {name}"), err))?;
    }
    Ok(fd)
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
                instruction_image::symlinked_image_file(filename)
            } else {
                internal(&format!("create image {filename}"), err)
            }
        })?;
        // Make the image other-readable so the agent UID can read it (open's mode
        // is umask-masked; fchmod the owned fd to set it deterministically).
        fchmod(&fd, Mode::from_bits_truncate(FILE_MODE)).map_err(|err| internal(&format!("chmod {filename}"), err))?;
        let mut file = std::fs::File::from(fd);
        file.write_all(bytes).map_err(|err| internal(&format!("write image {filename}"), err))?;
        file.sync_all().map_err(|err| internal(&format!("sync image {filename}"), err))?;
        paths.push(format!("/workspace/{TASK_IMAGES_DIR}/{task_dir}/{filename}"));
    }
    Ok(paths)
}

/// Remove `<projects_root>/.task-images/<task_id>/` symlink-safely — the compensation
/// for a dispatch that materialized images but then rolled back, so the orphaned
/// directory is not left readable in a reused workspace. Best-effort and idempotent:
/// an already-absent directory is success, and a symlinked/non-directory component or
/// a planted sub-directory is refused (left in place) rather than followed, mirroring
/// the write path's escape guards and the background sweeper. The same removal also
/// runs in the jobs-crate sweeper; this copy keeps the api compensation self-contained
/// (the two crates do not share a filesystem helper).
pub fn remove_task_images(projects_root: &Path, task_id: Uuid) -> AppResult<()> {
    use rustix::io::Errno;

    let root = match std::fs::canonicalize(projects_root) {
        Ok(root) => root,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(internal("canonicalize projects root", err)),
    };
    let root_fd =
        openat(CWD, &root, OFlags::DIRECTORY | OFlags::CLOEXEC, Mode::empty()).map_err(|e| internal("open root", e))?;

    let images_fd = match openat(
        &root_fd,
        TASK_IMAGES_DIR,
        OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(Errno::NOENT) | Err(Errno::LOOP) | Err(Errno::NOTDIR) => return Ok(()),
        Err(err) => return Err(internal("openat .task-images", err)),
    };
    let task_dir = task_id.to_string();
    let task_fd = match openat(
        &images_fd,
        task_dir.as_str(),
        OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(Errno::NOENT) | Err(Errno::LOOP) | Err(Errno::NOTDIR) => return Ok(()),
        Err(err) => return Err(internal("openat task dir", err)),
    };

    let mut names: Vec<std::ffi::CString> = Vec::new();
    for entry in Dir::read_from(&task_fd).map_err(|e| internal("read task dir", e))? {
        let name = entry.map_err(|e| internal("read entry", e))?.file_name().to_owned();
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        names.push(name);
    }
    for name in &names {
        // unlinkat removes a symlink entry itself (not its target) and fails on a
        // sub-directory, so it can never escape; best-effort per entry.
        let _ = unlinkat(&task_fd, name.as_c_str(), AtFlags::empty());
    }
    match unlinkat(&images_fd, task_dir.as_str(), AtFlags::REMOVEDIR) {
        Ok(()) | Err(Errno::NOENT) | Err(Errno::NOTEMPTY) | Err(Errno::EXIST) => Ok(()),
        Err(err) => Err(internal("rmdir task dir", err)),
    }
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
    fn materialized_paths_are_readable_by_other_uids() {
        use std::os::unix::fs::PermissionsExt;
        let root = temp_root();
        let task_id = Uuid::new_v4();
        materialize_task_images(&root, task_id, &[("a.png".to_string(), b"PNG".to_vec())]).expect("materialize");

        // The agent container runs as a DIFFERENT UID than the API process, so it
        // can only reach the bind-mounted image if every dir is other-traversable
        // and the file is other-readable. (Defends against a restrictive umask.)
        let images_dir = root.join(".task-images");
        let task_dir = images_dir.join(task_id.to_string());
        let file = task_dir.join("a.png");
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&images_dir) & 0o005, 0o005, ".task-images must be other-readable+traversable");
        assert_eq!(mode(&task_dir) & 0o005, 0o005, "task dir must be other-readable+traversable");
        assert_eq!(mode(&file) & 0o004, 0o004, "image file must be other-readable");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn empty_images_is_noop() {
        let root = temp_root();
        assert!(materialize_task_images(&root, Uuid::new_v4(), &[]).expect("noop").is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn remove_reclaims_materialized_dir_and_is_idempotent() {
        let root = temp_root();
        let task_id = Uuid::new_v4();
        materialize_task_images(&root, task_id, &[("a.png".to_string(), b"x".to_vec())]).expect("materialize");
        let task_dir = root.join(".task-images").join(task_id.to_string());
        assert!(task_dir.exists());

        remove_task_images(&root, task_id).expect("remove");
        assert!(!task_dir.exists(), "compensation removes the materialized dir");
        // Idempotent: removing again (or an absent root) is success, not an error.
        remove_task_images(&root, task_id).expect("remove missing is ok");
        remove_task_images(&root.join("nope"), Uuid::new_v4()).expect("absent root is ok");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn remove_refuses_to_follow_a_symlinked_task_images() {
        let root = temp_root();
        let escape = std::env::temp_dir().join(format!("af-rm-escape-{}", Uuid::new_v4()));
        let task_id = Uuid::new_v4();
        std::fs::create_dir_all(escape.join(task_id.to_string())).expect("escape dir");
        std::os::unix::fs::symlink(&escape, root.join(".task-images")).expect("plant symlink");

        // A symlinked `.task-images` is refused (Ok, no-op); the escape target stays.
        remove_task_images(&root, task_id).expect("refuse via no-op");
        assert!(escape.join(task_id.to_string()).exists(), "must not delete through the symlink");

        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&escape).ok();
    }
}
