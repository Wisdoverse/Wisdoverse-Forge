//! Local state publication: flush contents first, then publish without a partial file.

use std::path::Path;

pub fn create_file(path: &Path) -> std::io::Result<std::fs::File> {
    #[cfg(windows)]
    {
        crate::windows_security::create_private_file(path)
    }
    #[cfg(not(windows))]
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path)
    }
}

pub async fn create_file_async(path: std::path::PathBuf) -> std::io::Result<tokio::fs::File> {
    tokio::task::spawn_blocking(move || create_file(&path))
        .await
        .map_err(std::io::Error::other)?
        .map(tokio::fs::File::from_std)
}

pub async fn create_dirs(path: std::path::PathBuf) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        tokio::task::spawn_blocking(move || crate::windows_security::create_private_dirs(&path))
            .await
            .map_err(std::io::Error::other)?
    }
    #[cfg(not(windows))]
    {
        tokio::fs::create_dir_all(path).await
    }
}

pub fn move_file(source: &Path, target: &Path, replace: bool) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW};
        let source = source.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        let target = target.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        let flags = MOVEFILE_WRITE_THROUGH | if replace { MOVEFILE_REPLACE_EXISTING } else { 0 };
        // SAFETY: both paths are live, NUL-terminated UTF-16 arrays. Do not allow
        // cross-volume copy/delete: state publication must remain a local move.
        if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), flags) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        if replace {
            std::fs::rename(source, target)?;
        } else {
            // hard_link is an atomic no-clobber publication. Persist the new
            // name before dropping the temporary/pending name; recovery may see
            // both and must honor the completed tombstone first.
            std::fs::hard_link(source, target)?;
            sync_dir(target.parent().unwrap_or_else(|| Path::new(".")))?;
            std::fs::remove_file(source)?;
        }
        sync_dir(target.parent().unwrap_or_else(|| Path::new(".")))?;
        if source.parent() != target.parent() {
            sync_dir(source.parent().unwrap_or_else(|| Path::new(".")))?;
        }
        Ok(())
    }
}

pub fn sync_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(path)?.sync_all()
    }
    #[cfg(windows)]
    {
        // Windows publication is committed by MoveFileExW above. Directory
        // handles do not support Unix fsync. Deletion here is cleanup only:
        // completed tombstones precede deletion of pending/results, and event
        // identities survive WAL replay. Do not promise power-loss immunity.
        std::fs::metadata(path).map(|_| ())
    }
    #[cfg(not(any(unix, windows)))]
    {
        std::fs::File::open(path)?.sync_all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_does_not_clobber_and_snapshot_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("state.json");
        let source = dir.path().join("state.tmp");
        std::fs::write(&source, b"first").unwrap();
        move_file(&source, &target, false).unwrap();
        std::fs::write(&source, b"second").unwrap();
        assert!(move_file(&source, &target, false).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"first");
        move_file(&source, &target, true).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"second");
        assert!(!source.exists());
    }
}
