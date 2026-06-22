//! Write-ahead log for buffering events when NATS is unreachable.
//!
//! Each event is persisted as a separate JSON file named by nanosecond timestamp.
//! On replay, files are read in sorted order and deleted after successful delivery.
//!
//! ## Backpressure
//!
//! `append` maintains an O(1) in-process counter (`pending`) so the cap check is
//! free. The counter is seeded from disk at startup via [`Wal::init_pending`] and
//! then kept in sync incrementally: +1 on every successful write, -1 on every
//! successful [`acknowledge`]. Events that arrive when `pending >= max_pending` are
//! dropped (not written to disk), the `dropped` counter is incremented, a metrics
//! counter is emitted, and a warning is logged. Callers receive `Ok(None)` on a
//! drop — they must not treat this as an error, but must skip the acknowledge path.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Default ceiling on the number of WAL files that may be outstanding at once.
/// At ~1 KiB per hook event this is roughly 10 MiB of on-disk headroom.
pub const DEFAULT_WAL_MAX_PENDING: usize = 10_000;

/// File-based WAL for offline event resilience with a bounded admission policy.
pub struct Wal {
    path: PathBuf,
    /// Cached count of files currently on disk (seeded by [`init_pending`] at
    /// startup, then maintained incrementally).
    pending: Arc<AtomicUsize>,
    /// Monotonically increasing count of events dropped because the WAL was full.
    dropped: Arc<AtomicU64>,
    /// Maximum number of pending WAL files before new events are dropped.
    max_pending: usize,
}

impl Wal {
    /// Create a new WAL rooted at the given path (defaults to `/tmp/agentforge-wal`).
    /// Uses [`DEFAULT_WAL_MAX_PENDING`] as the ceiling.
    pub fn new(path: Option<&str>) -> Self {
        Self::with_max_pending(path, DEFAULT_WAL_MAX_PENDING)
    }

    /// Create a WAL with an explicit pending-entry ceiling.
    ///
    /// Primarily used in tests to exercise the backpressure path with a small cap
    /// without needing 10 000 files on disk.
    pub fn with_max_pending(path: Option<&str>, max_pending: usize) -> Self {
        Self {
            path: PathBuf::from(path.unwrap_or("/tmp/agentforge-wal")),
            pending: Arc::new(AtomicUsize::new(0)),
            dropped: Arc::new(AtomicU64::new(0)),
            max_pending,
        }
    }

    /// Seed the in-process `pending` counter from the actual on-disk file count.
    ///
    /// Call once at startup, **before** the relay-socket listener starts accepting
    /// connections, so that crash-recovered files are reflected in the cap check.
    /// After this point the counter is kept in sync incrementally by `append` and
    /// `acknowledge`.
    pub async fn init_pending(&self) {
        let count = self.pending_count().await.unwrap_or(0);
        self.pending.store(count, Ordering::Relaxed);
    }

    /// Append an event to the WAL directory.
    ///
    /// Returns:
    /// - `Ok(Some(path))` — the event was written; pass `path` to [`acknowledge`]
    ///   after a confirmed publish.
    /// - `Ok(None)` — the WAL is full (`pending >= max_pending`); the event was
    ///   **not** written to disk. Callers must not call `acknowledge` for a `None`
    ///   return and must not treat it as a hard error.
    /// - `Err(_)` — an I/O error prevented the write.
    ///
    /// Called by the relay-socket listener *before* a publish attempt (WAL-first
    /// durability), buffering it in a replay-compatible record. The returned path
    /// is passed to [`acknowledge`] once the publish is confirmed (flushed) so
    /// the durable copy is removed only after the NATS server has the event.
    /// Also used by tests and the periodic drain.
    pub async fn append(&self, data: &[u8]) -> std::io::Result<Option<PathBuf>> {
        if self.pending.load(Ordering::Relaxed) >= self.max_pending {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            metrics::counter!("agentforge_sidecar_wal_dropped_total").increment(1);
            tracing::warn!(max_pending = self.max_pending, "WAL full; dropping relay event");
            return Ok(None);
        }

        fs::create_dir_all(&self.path).await?;
        let filename = format!("{}.json", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
        let filepath = self.path.join(filename);
        let mut file = fs::File::create(&filepath).await?;
        file.write_all(data).await?;
        file.flush().await?;

        self.pending.fetch_add(1, Ordering::Relaxed);
        Ok(Some(filepath))
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
    ///
    /// Decrements the `pending` counter (saturating — it will never go below 0).
    pub async fn acknowledge(&self, path: &std::path::Path) -> std::io::Result<()> {
        fs::remove_file(path).await?;
        // Saturating decrement: if the counter ever drifts (e.g. a file was deleted
        // out of band) we should not wrap around to usize::MAX.
        self.pending.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_sub(1))).ok();
        Ok(())
    }

    /// Count the number of pending WAL entries by scanning the directory.
    ///
    /// This is an O(n) directory scan. Prefer [`pending_cached`] for the hot path;
    /// call this only at startup (via [`init_pending`]) or in tests.
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

    /// Return the cached pending-entry count (O(1)).
    ///
    /// Accurate after [`init_pending`] is called at startup; maintained
    /// incrementally by `append` and `acknowledge`. Exposed for the heartbeat
    /// health signal (see issue #808).
    pub fn pending_cached(&self) -> usize {
        self.pending.load(Ordering::Relaxed)
    }

    /// Return the total number of events dropped because the WAL was full.
    ///
    /// Monotonically increasing. Exposed for the heartbeat health signal (#808).
    // Consumed by tests and reserved for the heartbeat health signal in #808.
    #[allow(dead_code)]
    pub fn dropped_total(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
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
        let p1 = wal.append(b"{\"event\":1}").await.unwrap();
        assert!(p1.is_some(), "first append should succeed");
        // Small delay so filenames differ.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let p2 = wal.append(b"{\"event\":2}").await.unwrap();
        assert!(p2.is_some(), "second append should succeed");

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

    // -------------------------------------------------------------------------
    // Backpressure tests
    // -------------------------------------------------------------------------

    /// When the WAL is at capacity, new appends return `Ok(None)`, the dropped
    /// counter increments, and nothing is written to disk.
    #[tokio::test]
    async fn append_drops_when_at_capacity() {
        let tmp = tempfile::tempdir().unwrap();
        let wal = Wal::with_max_pending(Some(tmp.path().to_str().unwrap()), 2);

        // First two appends succeed.
        let r1 = wal.append(b"{\"n\":1}").await.unwrap();
        assert!(r1.is_some(), "first append within cap must succeed");
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let r2 = wal.append(b"{\"n\":2}").await.unwrap();
        assert!(r2.is_some(), "second append within cap must succeed");

        assert_eq!(wal.pending_cached(), 2, "cached counter should be 2");
        assert_eq!(wal.pending_count().await.unwrap(), 2, "disk count should be 2");

        // Third append is dropped.
        let r3 = wal.append(b"{\"n\":3}").await.unwrap();
        assert!(r3.is_none(), "append at cap must return Ok(None)");

        assert_eq!(wal.dropped_total(), 1, "dropped counter must be 1 after one drop");
        assert_eq!(wal.pending_cached(), 2, "cached counter must not change on a drop");
        assert_eq!(wal.pending_count().await.unwrap(), 2, "on-disk count must stay 2");
    }

    /// Acknowledging a WAL entry decrements the pending counter, allowing a
    /// subsequent append to succeed.
    #[tokio::test]
    async fn acknowledge_decrements_pending() {
        let tmp = tempfile::tempdir().unwrap();
        let wal = Wal::with_max_pending(Some(tmp.path().to_str().unwrap()), 1);

        let path = wal.append(b"{}").await.unwrap().expect("first append succeeds");
        assert_eq!(wal.pending_cached(), 1);

        wal.acknowledge(&path).await.unwrap();
        assert_eq!(wal.pending_cached(), 0, "pending must be 0 after acknowledge");

        // Now there is room again.
        let r2 = wal.append(b"{}").await.unwrap();
        assert!(r2.is_some(), "append after ack must succeed");
        assert_eq!(wal.pending_cached(), 1);
    }

    /// `pending_cached` never underflows below 0 even if `acknowledge` is called
    /// more times than `append` (e.g. files deleted out-of-band).
    #[tokio::test]
    async fn acknowledge_saturates_at_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let wal = Wal::with_max_pending(Some(tmp.path().to_str().unwrap()), 10);

        let path = wal.append(b"{}").await.unwrap().expect("append succeeds");
        assert_eq!(wal.pending_cached(), 1);

        wal.acknowledge(&path).await.unwrap();
        assert_eq!(wal.pending_cached(), 0);

        // Manually create a stale file and acknowledge it even though the counter
        // is already 0 — must not wrap to usize::MAX.
        let stale = tmp.path().join("stale.json");
        tokio::fs::write(&stale, b"{}").await.unwrap();
        wal.acknowledge(&stale).await.unwrap();
        assert_eq!(wal.pending_cached(), 0, "saturating decrement: must not underflow");
    }
}
