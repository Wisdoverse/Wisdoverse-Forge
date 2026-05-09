//! Write-ahead log for buffering events when NATS is unreachable.
//!
//! Each event is persisted as a separate JSON file named by nanosecond timestamp.
//! On replay, files are read in sorted order and deleted after successful delivery.

use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// File-based WAL for offline event resilience.
pub struct Wal {
    path: PathBuf,
}

impl Wal {
    /// Create a new WAL rooted at the given path (defaults to `/tmp/agentforge-wal`).
    pub fn new(path: Option<&str>) -> Self {
        Self { path: PathBuf::from(path.unwrap_or("/tmp/agentforge-wal")) }
    }

    /// Append an event to the WAL directory.
    #[allow(dead_code)] // Used by publisher (wired in main), and tests
    pub async fn append(&self, data: &[u8]) -> std::io::Result<()> {
        fs::create_dir_all(&self.path).await?;
        let filename = format!("{}.json", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
        let filepath = self.path.join(filename);
        let mut file = fs::File::create(&filepath).await?;
        file.write_all(data).await?;
        file.flush().await?;
        Ok(())
    }

    /// Replay all buffered events in timestamp order.
    ///
    /// Returns entries paired with their file paths. Files are NOT deleted;
    /// call [`acknowledge`] after each successful publish to remove them.
    pub async fn replay(&self) -> std::io::Result<Vec<(PathBuf, Vec<u8>)>> {
        let mut entries = Vec::new();

        if !self.path.exists() {
            return Ok(entries);
        }

        let mut dir = fs::read_dir(&self.path).await?;
        let mut files = Vec::new();
        while let Some(entry) = dir.next_entry().await? {
            if entry.path().extension().is_some_and(|e| e == "json") {
                files.push(entry.path());
            }
        }
        files.sort();

        for file_path in files {
            let data = fs::read(&file_path).await?;
            entries.push((file_path, data));
        }

        Ok(entries)
    }

    /// Acknowledge (delete) a single WAL file after successful publish.
    pub async fn acknowledge(&self, path: &std::path::Path) -> std::io::Result<()> {
        fs::remove_file(path).await
    }

    /// Count the number of pending WAL entries.
    pub async fn pending_count(&self) -> std::io::Result<usize> {
        if !self.path.exists() {
            return Ok(0);
        }
        let mut count = 0;
        let mut dir = fs::read_dir(&self.path).await?;
        while let Some(entry) = dir.next_entry().await? {
            if entry.path().extension().is_some_and(|e| e == "json") {
                count += 1;
            }
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_append_and_replay() {
        let tmp = tempfile::tempdir().unwrap();
        let wal = Wal::new(Some(tmp.path().to_str().unwrap()));

        // Append two entries.
        wal.append(b"{\"event\":1}").await.unwrap();
        // Small delay so filenames differ.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        wal.append(b"{\"event\":2}").await.unwrap();

        assert_eq!(wal.pending_count().await.unwrap(), 2);

        let entries = wal.replay().await.unwrap();
        assert_eq!(entries.len(), 2);
        // First entry should be event 1 (sorted by timestamp filename).
        assert!(String::from_utf8_lossy(&entries[0].1).contains("\"event\":1"));
        assert!(String::from_utf8_lossy(&entries[1].1).contains("\"event\":2"));

        // After replay, files should still exist (not deleted).
        assert_eq!(wal.pending_count().await.unwrap(), 2);

        // Acknowledge each entry to delete the files.
        for (path, _) in &entries {
            wal.acknowledge(path).await.unwrap();
        }
        assert_eq!(wal.pending_count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_replay_empty_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let wal = Wal::new(Some(tmp.path().to_str().unwrap()));
        let entries = wal.replay().await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_replay_nonexistent_directory() {
        let wal = Wal::new(Some("/tmp/agentforge-wal-nonexistent-test-dir"));
        let entries = wal.replay().await.unwrap();
        assert!(entries.is_empty());
        assert_eq!(wal.pending_count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_pending_count_ignores_non_json() {
        let tmp = tempfile::tempdir().unwrap();
        let wal = Wal::new(Some(tmp.path().to_str().unwrap()));

        // Write a .json file and a .txt file.
        wal.append(b"{}").await.unwrap();
        tokio::fs::write(tmp.path().join("note.txt"), b"hello").await.unwrap();

        assert_eq!(wal.pending_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_default_path() {
        let wal = Wal::new(None);
        assert_eq!(wal.path, PathBuf::from("/tmp/agentforge-wal"));
    }
}
