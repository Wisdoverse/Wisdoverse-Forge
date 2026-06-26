//! Bounded sweeper that reclaims materialized instruction-image directories.
//!
//! The API materializes a task's uploaded images into the agent's workspace at
//! `<projects_root>/.task-images/<task_id>/` at dispatch. Nothing deletes them
//! when the task ends, so without this sweeper stale instruction images would be
//! readable by future agents in the same (reused) workspace and disk would grow
//! outside the attachment/object-storage lifecycle.
//!
//! Both terminal paths (agent results via the jobs result consumer, operator
//! completes via the api service) write the DB directly across a crate boundary,
//! so a periodic sweeper is the single place that covers both. The projects-root
//! join is shared with the api writer via `agentforge_core::workspace` so the
//! remove side targets exactly what the write side created.
//!
//! Race safety: removal of each candidate runs in its own transaction that takes a
//! `FOR UPDATE` lock on the task row, removes the directory, and sets the permanent
//! done marker (`task_images_cleaned_at`) — all before commit. That row lock is the
//! whole concurrency story: the dispatch path holds the SAME lock from
//! `assign_agent_in_tx` through commit and only materializes images afterward, so the
//! sweeper and a (re-)dispatch are mutually exclusive. A task being dispatched right
//! now is skipped via `SKIP LOCKED` (cleaned a later tick); a task retried after the
//! unlocked candidate scan fails the under-lock age re-check (its `updated_at` is
//! fresh, courtesy the `orchestration_tasks_updated_at` trigger) and is left alone;
//! and a sweeper that wins the lock first forces the dispatch to block at
//! `assign_agent_in_tx` — before it writes any files — so freshly materialized images
//! are never unlinked. Because the done marker is written only after removal and in
//! the same transaction, a crash rolls back cleanly and the task is retried on the
//! next tick — no stranded directory, no TTL-long recovery delay. The marker bounds
//! work: finished rows drop out, so the capped scan makes forward progress instead of
//! re-returning already-removed rows. A directory the sweeper REFUSES to fully remove
//! (symlinked component / planted sub-dir) is not marked done but gets a
//! `task_images_retry_after` backoff, so an old obstructed prefix cannot pin the
//! oldest-first scan and starve newer tasks. All three cleanup columns are written
//! via migration 084's trigger guard, which suppresses the `updated_at` bump for
//! cleanup-only writes — so sweeping a task does not surface as fresh activity in the
//! API/UI, and the age the TTL gate reads is preserved. Removal is symlink-safe
//! (`openat(O_NOFOLLOW)` walk from the canonicalized root), mirroring the writer's
//! escape protections.

use std::path::Path;
use std::time::Duration;

use rustix::fs::{AtFlags, CWD, Dir, Mode, OFlags, openat, unlinkat};
use sqlx::PgPool;
use tokio::sync::watch;
use uuid::Uuid;

const TASK_IMAGES_DIR: &str = ".task-images";

/// Default sweep interval. Cleanup is not time-critical, so a slow cadence keeps
/// the FS/DB load negligible.
const DEFAULT_INTERVAL: Duration = Duration::from_secs(600);

/// Candidate not-yet-done image tasks past the TTL whose refused-backoff has expired
/// (cheap, unlocked scan). Eligible statuses are the terminal ones AND an unassigned
/// `backlog`/`blocked` task: when a terminal image task is retried it returns to
/// backlog (or blocks waiting for a manually-chosen vision agent) with its agent
/// cleared, leaving the old `.task-images/<id>` from the dead assignment — that
/// directory is regenerated on the next dispatch, so reclaiming it is safe. A
/// `working`/`queued` (or still-assigned) task is excluded because its directory is
/// live or about to be. Uses the dispatch-persisted `task_images_workspace_id` (NOT a
/// join on the possibly hard-deleted assigned agent), the `task_images_cleaned_at`
/// done marker, and the `task_images_retry_after` backoff so finished and
/// recently-refused rows drop out and the capped, oldest-first scan makes progress
/// instead of being pinned by an old obstructed prefix. Each candidate is re-checked
/// under a row lock before removal (see `LOCK_SWEEPABLE_SQL`). `$1` = ttl secs.
pub(crate) const SELECT_SWEEPABLE_SQL: &str = "
    SELECT id AS task_id
    FROM orchestration_tasks
    WHERE (status IN ('completed', 'failed', 'canceled')
           OR (status IN ('backlog', 'blocked') AND assigned_agent_id IS NULL))
      AND task_images_workspace_id IS NOT NULL
      AND task_images_cleaned_at IS NULL
      AND updated_at < NOW() - make_interval(secs => $1)
      AND (task_images_retry_after IS NULL OR task_images_retry_after < NOW())
    ORDER BY updated_at
    LIMIT 500
";

/// Re-check a candidate under a `FOR UPDATE` row lock, immediately before removing
/// its directory, and return the workspace to remove from. The lock is the
/// concurrency boundary: the dispatch path holds this same row lock from
/// `assign_agent_in_tx` through commit (before it materializes any images), so this
/// lock makes the sweeper's remove+mark and a (re-)dispatch mutually exclusive.
/// `SKIP LOCKED` means a task currently being dispatched is left for a later tick
/// rather than blocking the sweep, and the re-checked predicate (same eligible-status
/// set as the scan) drops a task that was (re-)dispatched after the unlocked scan —
/// it is now `working`/assigned, or its `updated_at` is fresh — or that was backed
/// off. Held inside the same transaction as the removal + `MARK_CLEANED_SQL`. `$1` =
/// task id, `$2` = ttl secs. Returns the workspace id, or no row if locked / no longer
/// eligible.
pub(crate) const LOCK_SWEEPABLE_SQL: &str = "
    SELECT organization_id, task_images_workspace_id
    FROM orchestration_tasks
    WHERE id = $1
      AND (status IN ('completed', 'failed', 'canceled')
           OR (status IN ('backlog', 'blocked') AND assigned_agent_id IS NULL))
      AND task_images_workspace_id IS NOT NULL
      AND task_images_cleaned_at IS NULL
      AND updated_at < NOW() - make_interval(secs => $2)
      AND (task_images_retry_after IS NULL OR task_images_retry_after < NOW())
    FOR UPDATE SKIP LOCKED
";

/// Set the PERMANENT done marker, only AFTER removal is confirmed and inside the
/// row-locked transaction, so a crash before commit rolls back cleanly and the task
/// is retried on the next tick. Migration 084's trigger guard keeps this
/// cleanup-only write from bumping `updated_at`, so a swept task does not surface as
/// freshly updated in the API/UI. `$1` = task id.
pub(crate) const MARK_CLEANED_SQL: &str = "UPDATE orchestration_tasks SET task_images_cleaned_at = NOW() WHERE id = $1";

/// Back off a directory the sweeper refused to fully remove (symlinked component or
/// planted sub-directory) so it stops re-filling the oldest-first scan every tick;
/// it becomes a candidate again after the backoff. A cleanup-only write, so it does
/// not bump `updated_at`. `$1` = task id, `$2` = backoff secs.
pub(crate) const BACKOFF_REFUSED_SQL: &str =
    "UPDATE orchestration_tasks SET task_images_retry_after = NOW() + make_interval(secs => $2) WHERE id = $1";

/// Backoff for a refused directory. Long enough that an obstructed prefix cannot
/// starve the scan, short enough that a transiently-obstructed dir is retried the
/// same day. `ponytail:` fixed constant, lift to config only if a deployment needs a
/// different cadence.
const REFUSED_BACKOFF_SECS: i64 = 3600;

pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_task_images_cleanup_removed_total",
        "Total .task-images/<task_id> directories reclaimed by the sweeper"
    );
    metrics::describe_counter!(
        "agentforge_task_images_cleanup_errors_total",
        "Total per-task errors while removing a .task-images directory"
    );
    metrics::describe_counter!(
        "agentforge_task_images_cleanup_tick_errors_total",
        "Total task-images cleanup tick (query) errors"
    );
    metrics::counter!("agentforge_task_images_cleanup_removed_total").increment(0);
    metrics::counter!("agentforge_task_images_cleanup_errors_total").increment(0);
    metrics::counter!("agentforge_task_images_cleanup_tick_errors_total").increment(0);
}

pub struct TaskImagesCleanupWorker {
    pool: PgPool,
    workspace_root: String,
    ttl_secs: u64,
    interval: Duration,
}

impl TaskImagesCleanupWorker {
    pub fn new(pool: PgPool, workspace_root: String, ttl_secs: u64) -> Self {
        Self { pool, workspace_root, ttl_secs, interval: DEFAULT_INTERVAL }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        tracing::info!(
            ttl_secs = self.ttl_secs,
            interval_secs = self.interval.as_secs(),
            workspace_root = %self.workspace_root,
            "task images cleanup sweeper started"
        );
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() { break; }
                }
                _ = ticker.tick() => {
                    if let Err(err) = self.tick().await {
                        metrics::counter!("agentforge_task_images_cleanup_tick_errors_total").increment(1);
                        tracing::warn!(error = %err, "task images cleanup tick failed");
                    }
                }
            }
        }
        tracing::info!("task images cleanup sweeper stopped");
    }

    async fn tick(&self) -> sqlx::Result<u64> {
        let ttl = self.ttl_secs as i64;
        let candidates: Vec<(Uuid,)> = sqlx::query_as(SELECT_SWEEPABLE_SQL).bind(ttl).fetch_all(&self.pool).await?;

        let mut removed = 0u64;
        for (task_id,) in candidates {
            match self.sweep_one(task_id, ttl).await {
                Ok(true) => {
                    removed += 1;
                    metrics::counter!("agentforge_task_images_cleanup_removed_total").increment(1);
                }
                Ok(false) => {} // skipped (locked / retried / already gone) or removed-nothing
                Err(err) => {
                    metrics::counter!("agentforge_task_images_cleanup_errors_total").increment(1);
                    tracing::warn!(%task_id, error = %err, "failed to remove task image dir");
                }
            }
        }
        Ok(removed)
    }

    /// Remove one candidate's image dir under a row lock, then mark it done in the
    /// same transaction. Returns `Ok(true)` only when a directory was actually
    /// reclaimed. The row lock (`FOR UPDATE SKIP LOCKED`) serialises against the
    /// dispatch path so a concurrent (re-)dispatch can neither have its fresh images
    /// deleted nor be cleaned mid-flight; a crash before commit rolls back, leaving
    /// the row eligible for the next tick.
    async fn sweep_one(&self, task_id: Uuid, ttl: i64) -> sqlx::Result<bool> {
        let mut tx = self.pool.begin().await?;
        let locked: Option<(Uuid, Uuid)> =
            sqlx::query_as(LOCK_SWEEPABLE_SQL).bind(task_id).bind(ttl).fetch_optional(&mut *tx).await?;
        let Some((org_id, workspace_id)) = locked else {
            return Ok(false); // locked by a live dispatch, or retried/cleaned since the scan
        };

        let projects_root =
            agentforge_core::workspace::workspace_projects_root(&self.workspace_root, org_id, workspace_id);
        // Removal runs while the row lock is held, so the dispatch path (which takes
        // the same lock before it materializes) cannot interleave with the unlink.
        // On error, return (dropping `tx` rolls back, releasing the lock) so the row
        // stays eligible for a later tick instead of being marked done. `tick` logs
        // and counts the failure.
        match remove_task_image_dir(&projects_root, task_id)? {
            Removal::Done { reclaimed } => {
                // The directory is gone — only now mark the task done so it drops out
                // of the scan.
                sqlx::query(MARK_CLEANED_SQL).bind(task_id).execute(&mut *tx).await?;
                tx.commit().await?;
                Ok(reclaimed)
            }
            Removal::Refused => {
                // A symlinked component or a planted sub-dir left bytes in place. Do
                // NOT mark done — but stamp a backoff so this old, obstructed row
                // stops re-filling the oldest-first scan every tick (which would
                // starve newer tasks); it is retried once the backoff expires, in
                // case the obstruction clears.
                sqlx::query(BACKOFF_REFUSED_SQL).bind(task_id).bind(REFUSED_BACKOFF_SECS).execute(&mut *tx).await?;
                tx.commit().await?;
                tracing::debug!(%org_id, %workspace_id, %task_id, "task image dir refused (symlink/non-empty); backing off");
                Ok(false)
            }
        }
    }
}

/// Outcome of attempting to remove a task's image directory.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Removal {
    /// The directory is gone: reclaimed just now (`reclaimed = true`) or already
    /// absent (`reclaimed = false`). Either way there is nothing left, so the task
    /// is safe to mark done.
    Done { reclaimed: bool },
    /// A symlinked / non-directory component, or a planted sub-directory we refuse
    /// to recurse into, left bytes in place. The task must NOT be marked done — a
    /// later tick retries once the obstruction is gone.
    Refused,
}

/// Remove `<projects_root>/.task-images/<task_id>/` symlink-safely. `Err` only on an
/// unexpected filesystem error; otherwise a `Removal` distinguishing "gone" (safe to
/// mark done) from "refused" (bytes may remain, retry later).
fn remove_task_image_dir(projects_root: &Path, task_id: Uuid) -> std::io::Result<Removal> {
    use rustix::io::Errno;

    // Canonicalize the trusted, server-controlled root so the walk anchor has no
    // symlinks; if the workspace itself is gone, there is nothing to clean.
    let root = match std::fs::canonicalize(projects_root) {
        Ok(root) => root,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Removal::Done { reclaimed: false }),
        Err(err) => return Err(err),
    };
    let root_fd = openat(CWD, &root, OFlags::DIRECTORY | OFlags::CLOEXEC, Mode::empty())?;

    // `.task-images` and `<task_id>` must each be a real directory, never a
    // symlink (an agent could have swapped one in after materialization).
    let images_fd = match openat(
        &root_fd,
        TASK_IMAGES_DIR,
        OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(Errno::NOENT) => return Ok(Removal::Done { reclaimed: false }),
        // A symlink/non-directory component: `O_NOFOLLOW` refused to traverse it
        // (surfacing as ELOOP or, with `O_DIRECTORY`, ENOTDIR depending on the
        // kernel) — nothing was opened, and bytes may remain behind it, so refuse.
        Err(Errno::LOOP) | Err(Errno::NOTDIR) => return Ok(Removal::Refused),
        Err(err) => return Err(err.into()),
    };
    let task_dir = task_id.to_string();
    let task_fd = match openat(
        &images_fd,
        task_dir.as_str(),
        OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(Errno::NOENT) => return Ok(Removal::Done { reclaimed: false }), // already gone
        Err(Errno::LOOP) | Err(Errno::NOTDIR) => return Ok(Removal::Refused), // symlinked/non-dir — refuse
        Err(err) => return Err(err.into()),
    };

    // Collect the flat entry names (the writer only puts files here), then unlink
    // them. `unlinkat` removes a symlink entry itself (not its target) and fails
    // on a planted sub-directory (left in place), so it can never escape.
    let mut names: Vec<std::ffi::CString> = Vec::new();
    for entry in Dir::read_from(&task_fd)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        names.push(name.to_owned());
    }
    for name in &names {
        let _ = unlinkat(&task_fd, name.as_c_str(), AtFlags::empty());
    }

    // Remove the now-empty task dir via its parent fd. A planted sub-dir leaves it
    // non-empty (ENOTEMPTY/EEXIST) — refuse and retry later rather than mark done
    // with bytes still inside; ENOENT (raced) means it is already gone.
    match unlinkat(&images_fd, task_dir.as_str(), AtFlags::REMOVEDIR) {
        Ok(()) => Ok(Removal::Done { reclaimed: true }),
        Err(Errno::NOENT) => Ok(Removal::Done { reclaimed: false }),
        Err(Errno::NOTEMPTY) | Err(Errno::EXIST) => Ok(Removal::Refused),
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BACKOFF_REFUSED_SQL, LOCK_SWEEPABLE_SQL, MARK_CLEANED_SQL, Removal, SELECT_SWEEPABLE_SQL, remove_task_image_dir,
    };
    use std::fs;
    use uuid::Uuid;

    fn temp_root() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("af-img-cleanup-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("temp projects root");
        dir
    }

    #[test]
    fn sweep_sql_scopes_to_uncleaned_terminal_image_tasks() {
        // Unlocked candidate scan: not-done image tasks past the TTL — terminal, or an
        // unassigned backlog/blocked (retried) task whose stale dir is regenerated on
        // re-dispatch.
        assert!(SELECT_SWEEPABLE_SQL.contains("status IN ('completed', 'failed', 'canceled')"));
        assert!(SELECT_SWEEPABLE_SQL.contains("status IN ('backlog', 'blocked') AND assigned_agent_id IS NULL"));
        assert!(SELECT_SWEEPABLE_SQL.contains("task_images_workspace_id IS NOT NULL"));
        assert!(SELECT_SWEEPABLE_SQL.contains("task_images_cleaned_at IS NULL"));
        assert!(SELECT_SWEEPABLE_SQL.contains("updated_at < NOW() - make_interval(secs => $1)"));
        // Refused rows are skipped until their backoff expires (no oldest-first pin).
        assert!(SELECT_SWEEPABLE_SQL.contains("task_images_retry_after IS NULL OR task_images_retry_after < NOW()"));
        // Per-row re-check holds the row lock and skips a row a live dispatch holds,
        // re-checking the same eligible-status set + age + backoff.
        assert!(LOCK_SWEEPABLE_SQL.contains("FOR UPDATE SKIP LOCKED"));
        assert!(LOCK_SWEEPABLE_SQL.contains("status IN ('completed', 'failed', 'canceled')"));
        assert!(LOCK_SWEEPABLE_SQL.contains("status IN ('backlog', 'blocked') AND assigned_agent_id IS NULL"));
        assert!(LOCK_SWEEPABLE_SQL.contains("task_images_cleaned_at IS NULL"));
        assert!(LOCK_SWEEPABLE_SQL.contains("updated_at < NOW() - make_interval(secs => $2)"));
        assert!(LOCK_SWEEPABLE_SQL.contains("task_images_retry_after IS NULL OR task_images_retry_after < NOW()"));
        // The permanent done marker is set only after removal, in the locked tx.
        assert!(MARK_CLEANED_SQL.contains("SET task_images_cleaned_at = NOW()"));
        // A refused dir is backed off via the retry timestamp, never marked done.
        assert!(BACKOFF_REFUSED_SQL.contains("SET task_images_retry_after = NOW() + make_interval(secs => $2)"));
    }

    #[test]
    fn removes_the_task_dir_and_its_files() {
        let root = temp_root();
        let task_id = Uuid::new_v4();
        let task_dir = root.join(".task-images").join(task_id.to_string());
        fs::create_dir_all(&task_dir).unwrap();
        fs::write(task_dir.join("a.png"), b"x").unwrap();
        fs::write(task_dir.join("b.png"), b"y").unwrap();

        assert_eq!(remove_task_image_dir(&root, task_id).unwrap(), Removal::Done { reclaimed: true });
        assert!(!task_dir.exists(), "task dir must be gone");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn missing_dir_is_a_noop() {
        let root = temp_root();
        assert_eq!(remove_task_image_dir(&root, Uuid::new_v4()).unwrap(), Removal::Done { reclaimed: false });
        // also when the whole projects root is absent
        assert_eq!(
            remove_task_image_dir(&root.join("nope"), Uuid::new_v4()).unwrap(),
            Removal::Done { reclaimed: false }
        );
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn refuses_a_task_dir_that_still_has_a_planted_subdir() {
        // An agent could leave a sub-directory inside `.task-images/<id>`; we only
        // unlink files, never recurse, so REMOVEDIR fails and we must NOT claim the
        // dir was cleaned (bytes remain).
        let root = temp_root();
        let task_id = Uuid::new_v4();
        let task_dir = root.join(".task-images").join(task_id.to_string());
        fs::create_dir_all(task_dir.join("planted")).unwrap();
        fs::write(task_dir.join("a.png"), b"x").unwrap();

        assert_eq!(remove_task_image_dir(&root, task_id).unwrap(), Removal::Refused);
        assert!(task_dir.join("planted").exists(), "planted sub-dir left in place");

        fs::remove_dir_all(&root).ok();
    }

    // Seed an org + user + an image task at a chosen age/done-state and materialize
    // its `.task-images/<id>` dir. No agent row is seeded — proving cleanup keys off
    // the dispatch-persisted workspace, not an agent join (survives agent deletion).
    async fn seed_image_task(
        pool: &sqlx::PgPool,
        org_id: Uuid,
        user_id: Uuid,
        ws_id: Option<Uuid>,
        cleaned: Option<chrono::DateTime<chrono::Utc>>,
        age_hours: i32,
    ) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO orchestration_tasks
               (id, organization_id, title, status, created_by,
                task_images_workspace_id, task_images_cleaned_at, updated_at)
             VALUES ($1,$2,'t','completed',$3,$4,$5, NOW() - make_interval(hours => $6))",
        )
        .bind(id)
        .bind(org_id)
        .bind(user_id)
        .bind(ws_id)
        .bind(cleaned)
        .bind(age_hours)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    async fn cleaned_at(pool: &sqlx::PgPool, id: Uuid) -> Option<chrono::DateTime<chrono::Utc>> {
        sqlx::query_scalar("SELECT task_images_cleaned_at FROM orchestration_tasks WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn retry_after(pool: &sqlx::PgPool, id: Uuid) -> Option<chrono::DateTime<chrono::Utc>> {
        sqlx::query_scalar("SELECT task_images_retry_after FROM orchestration_tasks WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn updated_at(pool: &sqlx::PgPool, id: Uuid) -> chrono::DateTime<chrono::Utc> {
        sqlx::query_scalar("SELECT updated_at FROM orchestration_tasks WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    // End-to-end: tick() scans not-done terminal image tasks past the TTL, removes
    // each dir under a row lock, marks it done, and leaves recent / already-done /
    // non-image tasks alone.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn tick_sweeps_only_old_uncleaned_image_tasks(pool: sqlx::PgPool) {
        use super::TaskImagesCleanupWorker;

        let workspace_root = temp_root();
        let root_str = workspace_root.to_str().unwrap().to_string();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let ws_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1,$2,$3)")
            .bind(org_id)
            .bind("Img Org")
            .bind(format!("img-{org_id}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users (id, email) VALUES ($1,$2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(&pool)
            .await
            .unwrap();

        let now = chrono::Utc::now();
        let old_image = seed_image_task(&pool, org_id, user_id, Some(ws_id), None, 2).await; // → swept
        let recent_image = seed_image_task(&pool, org_id, user_id, Some(ws_id), None, 0).await; // kept (age)
        let cleaned_image = seed_image_task(&pool, org_id, user_id, Some(ws_id), Some(now), 2).await; // kept (done)
        let no_workspace = seed_image_task(&pool, org_id, user_id, None, None, 2).await; // kept (text-only)
        let _ = no_workspace;

        let projects = agentforge_core::workspace::workspace_projects_root(&root_str, org_id, ws_id);
        for t in [old_image, recent_image, cleaned_image] {
            let d = projects.join(".task-images").join(t.to_string());
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("x.png"), b"x").unwrap();
        }

        let worker = TaskImagesCleanupWorker::new(pool.clone(), root_str, 3600); // ttl = 1h
        let removed = worker.tick().await.unwrap();

        assert_eq!(removed, 1, "only the old, un-cleaned image task is swept");
        let dir = |t: Uuid| projects.join(".task-images").join(t.to_string()).exists();
        assert!(!dir(old_image), "old image dir removed");
        assert!(dir(recent_image), "recent image dir kept (age)");
        assert!(dir(cleaned_image), "already-done image dir kept (done marker)");
        assert!(cleaned_at(&pool, old_image).await.is_some(), "swept task marked done");
        assert_eq!(worker.tick().await.unwrap(), 0, "second tick removes nothing");

        fs::remove_dir_all(&workspace_root).ok();
    }

    // The row lock is the dispatch/sweeper concurrency boundary: a candidate whose
    // task row is locked by an open transaction (standing in for an in-flight
    // (re-)dispatch) is SKIPPED — its dir is left intact and it is not marked done —
    // and is reclaimed only once the lock is released. This is what prevents the
    // sweeper from unlinking a re-dispatch's freshly materialized images.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn tick_skips_a_row_locked_by_an_in_flight_dispatch(pool: sqlx::PgPool) {
        use super::TaskImagesCleanupWorker;

        let workspace_root = temp_root();
        let root_str = workspace_root.to_str().unwrap().to_string();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let ws_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1,$2,$3)")
            .bind(org_id)
            .bind("Img Org")
            .bind(format!("img-{org_id}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users (id, email) VALUES ($1,$2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(&pool)
            .await
            .unwrap();

        let locked_task = seed_image_task(&pool, org_id, user_id, Some(ws_id), None, 2).await;
        let projects = agentforge_core::workspace::workspace_projects_root(&root_str, org_id, ws_id);
        let dir = projects.join(".task-images").join(locked_task.to_string());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("x.png"), b"x").unwrap();

        let worker = TaskImagesCleanupWorker::new(pool.clone(), root_str, 3600);

        // Hold the task row lock in a separate transaction, as a live dispatch would.
        let mut holder = pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM orchestration_tasks WHERE id = $1 FOR UPDATE")
            .bind(locked_task)
            .fetch_one(&mut *holder)
            .await
            .unwrap();

        assert_eq!(worker.tick().await.unwrap(), 0, "a row-locked task is skipped");
        assert!(dir.exists(), "locked task's image dir is left intact");
        assert!(cleaned_at(&pool, locked_task).await.is_none(), "locked task is not marked done");

        // Release the lock; now the sweeper reclaims it.
        holder.rollback().await.unwrap();
        assert_eq!(worker.tick().await.unwrap(), 1, "reclaimed once the lock is released");
        assert!(!dir.exists(), "image dir removed after the lock cleared");
        assert!(cleaned_at(&pool, locked_task).await.is_some(), "marked done after removal");

        fs::remove_dir_all(&workspace_root).ok();
    }

    // A dir we refuse to fully remove (here: a planted sub-directory we will not
    // recurse into) must NOT be marked done (its bytes are still there), but MUST be
    // backed off (`task_images_retry_after`) so it stops pinning the oldest-first scan
    // every tick; the next tick within the backoff window then skips it.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn tick_backs_off_a_refused_dir_without_marking_done(pool: sqlx::PgPool) {
        use super::TaskImagesCleanupWorker;

        let workspace_root = temp_root();
        let root_str = workspace_root.to_str().unwrap().to_string();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let ws_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1,$2,$3)")
            .bind(org_id)
            .bind("Img Org")
            .bind(format!("img-{org_id}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users (id, email) VALUES ($1,$2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(&pool)
            .await
            .unwrap();

        let task_id = seed_image_task(&pool, org_id, user_id, Some(ws_id), None, 2).await;
        let projects = agentforge_core::workspace::workspace_projects_root(&root_str, org_id, ws_id);
        let dir = projects.join(".task-images").join(task_id.to_string());
        fs::create_dir_all(dir.join("planted")).unwrap(); // survives the file-only unlink
        fs::write(dir.join("x.png"), b"x").unwrap();

        let worker = TaskImagesCleanupWorker::new(pool.clone(), root_str, 3600);
        assert_eq!(worker.tick().await.unwrap(), 0, "a refused dir is not counted as reclaimed");
        assert!(dir.join("planted").exists(), "planted sub-dir left in place");
        assert!(cleaned_at(&pool, task_id).await.is_none(), "refused dir must NOT be marked done");
        assert!(retry_after(&pool, task_id).await.is_some(), "refused dir must be backed off");
        // Backed off → the very next tick (within the backoff window) skips it.
        assert_eq!(worker.tick().await.unwrap(), 0, "backed-off refused dir is skipped next tick");

        fs::remove_dir_all(&workspace_root).ok();
    }

    // Sweeping a task is internal bookkeeping: marking it done must NOT bump the
    // task's `updated_at` (migration 084's trigger guard), or the API/UI would show
    // the task as freshly updated even though the operator did nothing.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn marking_cleaned_does_not_bump_updated_at(pool: sqlx::PgPool) {
        use super::TaskImagesCleanupWorker;

        let workspace_root = temp_root();
        let root_str = workspace_root.to_str().unwrap().to_string();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let ws_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1,$2,$3)")
            .bind(org_id)
            .bind("Img Org")
            .bind(format!("img-{org_id}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users (id, email) VALUES ($1,$2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(&pool)
            .await
            .unwrap();

        let task_id = seed_image_task(&pool, org_id, user_id, Some(ws_id), None, 2).await;
        let projects = agentforge_core::workspace::workspace_projects_root(&root_str, org_id, ws_id);
        let d = projects.join(".task-images").join(task_id.to_string());
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("x.png"), b"x").unwrap();

        let before = updated_at(&pool, task_id).await;
        let worker = TaskImagesCleanupWorker::new(pool.clone(), root_str, 3600);
        assert_eq!(worker.tick().await.unwrap(), 1, "old image task is swept");
        assert!(cleaned_at(&pool, task_id).await.is_some(), "swept task marked done");
        assert_eq!(updated_at(&pool, task_id).await, before, "cleanup must not bump updated_at");

        fs::remove_dir_all(&workspace_root).ok();
    }

    // Migration 084's one-time backfill: an image task materialized BEFORE this
    // feature shipped has `task_images_workspace_id = NULL` (the dispatch writer only
    // stamps it going forward), so without backfill the sweeper would never reclaim
    // its directory. Backfilling from the still-present assigned agent's workspace
    // makes it sweepable. This pins the SAME `UPDATE` migration 084 runs.
    #[sqlx::test(migrations = "../db/migrations")]
    async fn backfill_makes_a_pre_migration_image_task_sweepable(pool: sqlx::PgPool) {
        use super::TaskImagesCleanupWorker;

        let workspace_root = temp_root();
        let root_str = workspace_root.to_str().unwrap().to_string();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let ws_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1,$2,$3)")
            .bind(org_id)
            .bind("Img Org")
            .bind(format!("img-{org_id}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1,$2,'WS')")
            .bind(ws_id)
            .bind(org_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users (id, email) VALUES ($1,$2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status, cli_tool, runtime_kind)
             VALUES ($1,$2,$3,$4,'a','idle','claude','container')",
        )
        .bind(agent_id)
        .bind(org_id)
        .bind(ws_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

        // A pre-migration terminal image task: assigned to the agent, carries image
        // attachments, but its workspace column is NULL.
        let task_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO orchestration_tasks
               (id, organization_id, title, status, assigned_agent_id, created_by, params, updated_at)
             VALUES ($1,$2,'t','completed',$3,$4,$5, NOW() - make_interval(hours => 2))",
        )
        .bind(task_id)
        .bind(org_id)
        .bind(agent_id)
        .bind(user_id)
        .bind(serde_json::json!({ "imageAttachmentIds": ["11111111-1111-1111-1111-111111111111"] }))
        .execute(&pool)
        .await
        .unwrap();

        let projects = agentforge_core::workspace::workspace_projects_root(&root_str, org_id, ws_id);
        let dir = projects.join(".task-images").join(task_id.to_string());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("x.png"), b"x").unwrap();

        // ttl = 1h: the task's real `updated_at` is 2h old. Because migration 084's
        // trigger guard suppresses the `updated_at` bump for cleanup-column writes,
        // the backfill keeps that 2h-old timestamp, so the backfilled task is eligible
        // on the very first post-deploy tick (no extra TTL wait).
        let worker = TaskImagesCleanupWorker::new(pool.clone(), root_str, 3600);
        // Before backfill the column is NULL, so the candidate scan ignores the task.
        assert_eq!(worker.tick().await.unwrap(), 0, "un-backfilled task is invisible to the sweeper");
        assert!(dir.exists(), "dir untouched before backfill");

        let before_backfill = updated_at(&pool, task_id).await;
        // The exact backfill migration 084 runs: any-status assigned image task with
        // a NULL workspace, guarded by a CASE so a malformed `imageAttachmentIds`
        // can't raise.
        sqlx::query(
            "UPDATE orchestration_tasks t
                SET task_images_workspace_id = a.workspace_id
               FROM agents a
              WHERE t.assigned_agent_id = a.id
                AND a.workspace_id IS NOT NULL
                AND t.task_images_workspace_id IS NULL
                AND CASE
                      WHEN jsonb_typeof(t.params -> 'imageAttachmentIds') = 'array'
                        THEN jsonb_array_length(t.params -> 'imageAttachmentIds')
                      ELSE 0
                    END > 0",
        )
        .execute(&pool)
        .await
        .unwrap();

        let backfilled: Option<Uuid> =
            sqlx::query_scalar("SELECT task_images_workspace_id FROM orchestration_tasks WHERE id = $1")
                .bind(task_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(backfilled, Some(ws_id), "workspace backfilled from the assigned agent");
        assert_eq!(updated_at(&pool, task_id).await, before_backfill, "backfill must not bump updated_at");

        assert_eq!(worker.tick().await.unwrap(), 1, "backfilled task is now reclaimed");
        assert!(!dir.exists(), "pre-migration dir removed after backfill");

        fs::remove_dir_all(&workspace_root).ok();
    }

    // A terminal image task that is RETRIED returns to backlog (or blocks) with its
    // agent cleared, stranding the old `.task-images/<id>` dir. The sweeper reclaims
    // such an unassigned backlog/blocked task (its dir is regenerated on re-dispatch)
    // but must leave a still-assigned or still-`working` task alone (its dir is live).
    #[sqlx::test(migrations = "../db/migrations")]
    async fn tick_sweeps_retried_unassigned_image_task_not_assigned_or_working(pool: sqlx::PgPool) {
        use super::TaskImagesCleanupWorker;

        let workspace_root = temp_root();
        let root_str = workspace_root.to_str().unwrap().to_string();
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let ws_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1,$2,$3)")
            .bind(org_id)
            .bind("Img Org")
            .bind(format!("img-{org_id}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1,$2,'WS')")
            .bind(ws_id)
            .bind(org_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users (id, email) VALUES ($1,$2)")
            .bind(user_id)
            .bind(format!("u-{user_id}@example.com"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, user_id, name, status, cli_tool, runtime_kind)
             VALUES ($1,$2,$3,$4,'a','idle','claude','container')",
        )
        .bind(agent_id)
        .bind(org_id)
        .bind(ws_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

        const INSERT: &str = "INSERT INTO orchestration_tasks
               (id, organization_id, title, status, assigned_agent_id, created_by,
                task_images_workspace_id, updated_at)
             VALUES ($1,$2,'t',$3,$4,$5,$6, NOW() - make_interval(hours => 2))";
        let retried_backlog = Uuid::new_v4(); // backlog + unassigned → swept
        let retried_blocked = Uuid::new_v4(); // blocked + unassigned → swept
        let still_working = Uuid::new_v4(); // working → kept (live dir)
        let backlog_assigned = Uuid::new_v4(); // backlog but still assigned → kept
        for (id, status, agent) in [
            (retried_backlog, "backlog", None),
            (retried_blocked, "blocked", None),
            (still_working, "working", None),
            (backlog_assigned, "backlog", Some(agent_id)),
        ] {
            sqlx::query(INSERT)
                .bind(id)
                .bind(org_id)
                .bind(status)
                .bind(agent)
                .bind(user_id)
                .bind(ws_id)
                .execute(&pool)
                .await
                .unwrap();
        }

        let projects = agentforge_core::workspace::workspace_projects_root(&root_str, org_id, ws_id);
        for t in [retried_backlog, retried_blocked, still_working, backlog_assigned] {
            let d = projects.join(".task-images").join(t.to_string());
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("x.png"), b"x").unwrap();
        }

        let worker = TaskImagesCleanupWorker::new(pool.clone(), root_str, 3600);
        let removed = worker.tick().await.unwrap();

        assert_eq!(removed, 2, "both unassigned retried (backlog + blocked) tasks are swept");
        let dir = |t: Uuid| projects.join(".task-images").join(t.to_string()).exists();
        assert!(!dir(retried_backlog), "unassigned backlog (retried) dir removed");
        assert!(!dir(retried_blocked), "unassigned blocked (retried) dir removed");
        assert!(dir(still_working), "working task dir kept (live)");
        assert!(dir(backlog_assigned), "still-assigned backlog dir kept");

        fs::remove_dir_all(&workspace_root).ok();
    }

    #[test]
    fn refuses_to_remove_through_a_symlinked_task_images() {
        let root = temp_root();
        let escape = std::env::temp_dir().join(format!("af-img-escape-{}", Uuid::new_v4()));
        let task_id = Uuid::new_v4();
        fs::create_dir_all(escape.join(task_id.to_string())).unwrap();
        std::os::unix::fs::symlink(&escape, root.join(".task-images")).unwrap();

        // A symlinked .task-images is refused, and the escape target is intact.
        assert_eq!(remove_task_image_dir(&root, task_id).unwrap(), Removal::Refused);
        assert!(escape.join(task_id.to_string()).exists(), "must not delete through the symlink");

        fs::remove_dir_all(&root).ok();
        fs::remove_dir_all(&escape).ok();
    }
}
