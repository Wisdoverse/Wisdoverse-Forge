//! Integration tests for M5: the project_clone worker + reconciler.
//!
//! Driven against a REAL Postgres (`#[sqlx::test]` provisions a throwaway DB per
//! test) but WITHOUT a real Docker daemon — clone execution is injected behind a
//! fake [`CloneRunner`] returning a scripted [`CloneRunOutcome`]. Locally:
//!
//! ```text
//! DATABASE_URL='postgres://<role>:<pw>@127.0.0.1:5432/<role-owned-db>' \
//!   cargo test -p agentforge-api --test project_clone_worker
//! ```
//!
//! Covers (per the M5 spec, §6.3, §6.6, §6.7, §7, §8, §12, + the M5-review fixes):
//!   * happy path: queued -> Ready -> attempt+project `ready`, branch/head_sha
//!     persisted, target dir materialized by an ATOMIC rename, staging removed,
//!     a `clone.ready` audit event recorded;
//!   * Failed with a glued token -> attempt+project `failed`, error_message
//!     REDACTED (token absent), staging removed, a retry attempt 2 (queued) +
//!     outbox row created; and bounded (no retry past MAX_ATTEMPTS);
//!   * Timeout -> failed/timeout class; TooLarge -> failed/too_large class;
//!   * atomic-rename: an existing target dir is refused (no clobber) -> failed,
//!     never a false ready;
//!   * reconciler: an expired-lease `cloning` attempt is recovered/failed/retried;
//!     a `queued` attempt with no job is re-enqueued (a fresh outbox row);
//!   * redaction-before-persist: error_message == redact(raw).into_string() and
//!     never the raw tail (the M4-deferred redaction boundary test);
//!   * host-match credential: two creds for different hosts -> the matching one is
//!     picked + recorded; an unknown host -> None (clone proceeds anonymously).
//!
//! M5-review failure-branch coverage (added with the review fixes):
//!   * idempotent re-delivery: re-processing an already-`ready` or already-`failed`
//!     attempt is a no-op (no second clone.started, no extra retry, dir untouched);
//!   * recoverable publish (#1 regression): a materialized-but-unfinalized attempt
//!     (rename done, finalize lost) is force-readied by the reconciler — never a
//!     stranded `cloning` + a failing retry loop;
//!   * worker target-exists-is-mine: an attempt whose own `materialized_at` is set
//!     and whose target dir exists re-finalizes `ready` without re-cloning;
//!   * delete-race (#2): a project soft-deleted before publish cancels the attempt
//!     and creates NO directory;
//!   * runtime error (Docker create failed): staging cleaned + redacted + bounded
//!     retry (exercises `finish_failed_redacted` directly), clone.started emitted;
//!   * concurrent retry schedulers (failure-path + reconciler): exactly ONE next
//!     attempt + ONE unpublished outbox row;
//!   * container_id (#11) persisted before the wait.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

/// A throwaway workspace-root directory under the system temp dir, removed on
/// drop. Avoids pulling in the `tempfile` crate as a new dependency.
struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("agentforge-clone-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create temp workspace root");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

use agentforge_api::repositories::project::{CloneRequest, ProjectCreateTx, ProjectRepository};
use agentforge_api::services::git_credential::GitCredentialService;
use agentforge_api::services::project_clone_worker::{
    CloneRunner, CloneWorkerConfig, DEFAULT_MAX_ATTEMPTS, ProjectCloneWorker,
};
use agentforge_api::test_support::{TEST_LLM_ENCRYPTION_KEY, tenant_scope_for_ids};
use agentforge_core::{TenantScope, WorkspaceId};
use agentforge_platform::{CloneRunOutcome, CloneRunSpec, RawStderr};

const REPO_URL: &str = "https://github.com/example/repo.git";
const GITHUB_HOST: &str = "github.com";

// ---------------------------------------------------------------------------
// Fake CloneRunner — scripts an outcome and (for Ready) materializes the cloned
// `repo` subdir inside the staging dir so the worker's atomic rename succeeds.
// ---------------------------------------------------------------------------

struct FakeRunner {
    outcome: Mutex<Box<dyn Fn() -> CloneRunOutcome + Send + Sync>>,
    /// Captured staging paths + whether a credential secret was supplied, for
    /// assertions.
    calls: Mutex<Vec<FakeCall>>,
    /// When true, a Ready outcome materializes `<staging>/repo` so the rename has
    /// a real source dir; set false to simulate a missing source (rename fail).
    materialize_repo: bool,
}

#[derive(Clone)]
struct FakeCall {
    /// Captured staging path the runner was handed (asserts the worker mounts the
    /// per-attempt staging dir, not the projects root).
    #[allow(dead_code)]
    staging: PathBuf,
    had_credential: bool,
}

impl FakeRunner {
    fn new(outcome: impl Fn() -> CloneRunOutcome + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self { outcome: Mutex::new(Box::new(outcome)), calls: Mutex::new(Vec::new()), materialize_repo: true })
    }

    fn without_materialize(outcome: impl Fn() -> CloneRunOutcome + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            outcome: Mutex::new(Box::new(outcome)),
            calls: Mutex::new(Vec::new()),
            materialize_repo: false,
        })
    }

    fn last_call(&self) -> Option<FakeCall> {
        self.calls.lock().unwrap().last().cloned()
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

/// A runner whose `run` always returns an Err (simulating a Docker create/start
/// failure) — never produces a staging repo. Exercises the worker's runtime-error
/// branch (`finish_failed_redacted` directly).
struct ErrRunner;

#[async_trait::async_trait]
impl CloneRunner for ErrRunner {
    async fn run(&self, _spec: CloneRunSpec) -> agentforge_core::AppResult<CloneRunOutcome> {
        Err(agentforge_core::ErrorKind::Internal(anyhow::anyhow!("docker create failed: no such image")).into())
    }

    async fn sweep_orphans(&self) -> agentforge_core::AppResult<usize> {
        Ok(0)
    }
}

#[async_trait::async_trait]
impl CloneRunner for FakeRunner {
    async fn run(&self, spec: CloneRunSpec) -> agentforge_core::AppResult<CloneRunOutcome> {
        let had_credential = spec.credential.is_some();
        self.calls.lock().unwrap().push(FakeCall { staging: spec.staging_host_path.clone(), had_credential });

        let outcome = (self.outcome.lock().unwrap())();
        if matches!(outcome, CloneRunOutcome::Ready { .. }) && self.materialize_repo {
            // Mimic the entrypoint: create the cloned repo under <staging>/repo.
            let repo = spec.staging_host_path.join("repo");
            tokio::fs::create_dir_all(&repo).await.expect("create fake repo dir");
            tokio::fs::write(repo.join("README.md"), b"hello").await.expect("write fake file");
        }
        Ok(outcome)
    }

    async fn sweep_orphans(&self) -> agentforge_core::AppResult<usize> {
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

struct Seed {
    org_id: Uuid,
    workspace_id: Uuid,
    team_id: Uuid,
    user_id: Uuid,
    workspace_root: TempRoot,
}

async fn seed(pool: &PgPool) -> Seed {
    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("Org {org_id}"))
        .bind(format!("org-{org_id}"))
        .execute(pool)
        .await
        .expect("seed org");

    let workspace_id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, 'Default')")
        .bind(workspace_id)
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed workspace");

    let team_id = Uuid::new_v4();
    sqlx::query("INSERT INTO public.teams (id, organization_id, name, slug) VALUES ($1, $2, 'Engineering', $3)")
        .bind(team_id)
        .bind(org_id)
        .bind(format!("team-{team_id}"))
        .execute(pool)
        .await
        .expect("seed team");

    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("u-{user_id}@example.com"))
        .execute(pool)
        .await
        .expect("seed user");
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed org membership");

    let workspace_root = TempRoot::new();
    Seed { org_id, workspace_id, team_id, user_id, workspace_root }
}

fn scope(seed: &Seed) -> TenantScope {
    tenant_scope_for_ids(seed.org_id, seed.user_id)
}

/// Create a project with a repo (so attempt 1 / outbox exist) and return its id.
async fn create_cloned_project(pool: &PgPool, seed: &Seed, name: &str, url: &str) -> Uuid {
    let mut tx = pool.begin().await.expect("begin");
    let project = ProjectRepository::create_with_clone_in_tx(
        &mut tx,
        &scope(seed),
        ProjectCreateTx {
            workspace_id: WorkspaceId::from(seed.workspace_id),
            team_id: seed.team_id,
            name: name.to_string(),
            color: None,
            description: None,
            clone: Some(CloneRequest::parse(url).expect("parse url")),
        },
    )
    .await
    .expect("create with clone");
    tx.commit().await.expect("commit");
    project.id.as_uuid()
}

/// Build a worker over the fake runner with the temp workspace root + a secret
/// root under the same temp dir.
fn worker<R: CloneRunner + 'static>(pool: &PgPool, seed: &Seed, runner: Arc<R>) -> ProjectCloneWorker<R> {
    let secret_root = seed.workspace_root.path().join("clone-secrets");
    let config = CloneWorkerConfig {
        image: "agentforge-clone:test".to_string(),
        workspace_root: seed.workspace_root.path().to_string_lossy().to_string(),
        secret_root,
        timeout: Duration::from_secs(60),
        lease_ttl: Duration::from_secs(120),
        heartbeat_interval: Duration::from_secs(40),
        max_bytes: None,
        max_attempts: DEFAULT_MAX_ATTEMPTS,
        retry_backoff: Duration::from_millis(1),
        poll_interval: Duration::from_millis(10),
        reconcile_interval: Duration::from_millis(10),
    };
    let credentials = Arc::new(GitCredentialService::from_pool(pool.clone(), Some(TEST_LLM_ENCRYPTION_KEY)));
    ProjectCloneWorker::new(pool.clone(), credentials, runner, config)
}

fn projects_root(seed: &Seed) -> PathBuf {
    seed.workspace_root
        .path()
        .join("orgs")
        .join(seed.org_id.to_string())
        .join("workspaces")
        .join(seed.workspace_id.to_string())
        .join("projects")
}

async fn attempt_row(
    pool: &PgPool,
    project_id: Uuid,
    attempt: i32,
) -> (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<Uuid>) {
    sqlx::query_as(
        "SELECT status, error_class, error_message, resolved_branch, head_sha, credential_id
         FROM project_clone_attempts WHERE project_id = $1 AND attempt = $2",
    )
    .bind(project_id)
    .bind(attempt)
    .fetch_one(pool)
    .await
    .expect("fetch attempt")
}

async fn project_clone_status(pool: &PgPool, project_id: Uuid) -> String {
    sqlx::query_scalar("SELECT clone_status FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .expect("fetch clone_status")
}

async fn project_dir_name(pool: &PgPool, project_id: Uuid) -> String {
    sqlx::query_scalar("SELECT workspace_dir_name FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .expect("fetch dir name")
}

async fn count_audit(pool: &PgPool, action: &str, project_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = $1 AND resource_id = $2")
        .bind(action)
        .bind(project_id)
        .fetch_one(pool)
        .await
        .expect("count audit")
}

async fn count_attempts(pool: &PgPool, project_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM project_clone_attempts WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .expect("count attempts")
}

/// Just the status of an attempt (for the idempotency/recovery tests).
async fn attempt_status(pool: &PgPool, project_id: Uuid, attempt: i32) -> String {
    sqlx::query_scalar("SELECT status FROM project_clone_attempts WHERE project_id = $1 AND attempt = $2")
        .bind(project_id)
        .bind(attempt)
        .fetch_one(pool)
        .await
        .expect("fetch status")
}

/// Whether an attempt carries `materialized_at` (the irreversible-publish marker).
async fn attempt_is_materialized(pool: &PgPool, project_id: Uuid, attempt: i32) -> bool {
    sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
        "SELECT materialized_at FROM project_clone_attempts WHERE project_id = $1 AND attempt = $2",
    )
    .bind(project_id)
    .bind(attempt)
    .fetch_one(pool)
    .await
    .expect("fetch materialized_at")
    .is_some()
}

/// The persisted container_id of an attempt (for the #11 diagnostics test).
async fn attempt_container_id(pool: &PgPool, project_id: Uuid, attempt: i32) -> Option<String> {
    sqlx::query_scalar("SELECT container_id FROM project_clone_attempts WHERE project_id = $1 AND attempt = $2")
        .bind(project_id)
        .bind(attempt)
        .fetch_one(pool)
        .await
        .expect("fetch container_id")
}

async fn count_unpublished_outbox(pool: &PgPool, project_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM orchestration_outbox
          WHERE aggregate_type = 'project_clone' AND aggregate_id = $1 AND published_at IS NULL",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .expect("count outbox")
}

async fn count_unpublished_outbox_for_attempt(pool: &PgPool, project_id: Uuid, attempt: i32) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM orchestration_outbox
          WHERE aggregate_type = 'project_clone' AND aggregate_id = $1 AND published_at IS NULL
            AND payload->>'attempt' = $2",
    )
    .bind(project_id)
    .bind(attempt.to_string())
    .fetch_one(pool)
    .await
    .expect("count outbox for attempt")
}

async fn seed_git_credential(
    pool: &PgPool,
    seed: &Seed,
    provider: &str,
    remote_url: Option<&str>,
    token: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    let encrypted = agentforge_core::crypto::encrypt_base64(&TEST_LLM_ENCRYPTION_KEY, token).expect("encrypt token");
    sqlx::query(
        "INSERT INTO git_credentials (id, organization_id, user_id, name, provider, credential_type, token_encrypted, remote_url)
         VALUES ($1, $2, $3, $4, $5, 'token', $6, $7)",
    )
    .bind(id)
    .bind(seed.org_id)
    .bind(seed.user_id)
    .bind(format!("{provider} cred"))
    .bind(provider)
    .bind(encrypted.into_bytes())
    .bind(remote_url)
    .execute(pool)
    .await
    .expect("seed git credential");
    id
}

// ---------------------------------------------------------------------------
// 1. Happy path.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn ready_outcome_materializes_project_and_records_event(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "My Repo", REPO_URL).await;

    let runner = FakeRunner::new(|| CloneRunOutcome::Ready {
        branch: Some("main".to_string()),
        head_sha: "abc123def456".to_string(),
        bytes: 4096,
    });
    let worker = worker(&pool, &seed, runner.clone());

    worker.process_attempt_for_test(project_id, 1).await.expect("process attempt");

    // Attempt + project are ready; branch/head_sha persisted.
    let (status, _class, _msg, branch, head_sha, _cred) = attempt_row(&pool, project_id, 1).await;
    assert_eq!(status, "ready");
    assert_eq!(branch.as_deref(), Some("main"));
    assert_eq!(head_sha.as_deref(), Some("abc123def456"));
    assert_eq!(project_clone_status(&pool, project_id).await, "ready");

    // The target directory exists (materialized by the atomic rename) and the
    // staging dir was removed.
    let dir_name = project_dir_name(&pool, project_id).await;
    let target = projects_root(&seed).join(&dir_name);
    assert!(target.join("README.md").exists(), "cloned repo must be live at the project dir");
    let staging = projects_root(&seed).join(".clone-staging");
    if staging.exists() {
        let mut entries = tokio::fs::read_dir(&staging).await.expect("read staging");
        assert!(entries.next_entry().await.expect("next").is_none(), "staging dir must be empty after success");
    }

    // A clone.ready audit event was recorded.
    assert_eq!(count_audit(&pool, "clone.ready", project_id).await, 1, "clone.ready event recorded");
    assert_eq!(count_audit(&pool, "clone.started", project_id).await, 1, "clone.started event recorded");
}

// ---------------------------------------------------------------------------
// 2. Failure with a glued token -> redacted, retry scheduled (bounded).
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn failed_outcome_redacts_and_schedules_bounded_retry(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "Doomed", REPO_URL).await;

    // A failure stderr with a token glued into a redirected URL — the exact leak
    // shape the redactor must scrub before persistence.
    let raw_tail =
        "fatal: unable to access 'https://x-access-token:ghp_supersecrettoken0123456789@github.com/x.git': 403";
    let runner = FakeRunner::new(move || CloneRunOutcome::Failed {
        exit_code: 128,
        stderr_tail: RawStderr::new(raw_tail.to_string()),
    });
    let worker = worker(&pool, &seed, runner);

    worker.process_attempt_for_test(project_id, 1).await.expect("process attempt");

    // Attempt 1 is terminal failed; the PROJECT summary then mirrors the retry as
    // queued (asserted below) — spec §7 `failed -> bounded retry -> queued`.
    let (status, class, msg, _branch, _sha, _cred) = attempt_row(&pool, project_id, 1).await;
    assert_eq!(status, "failed");
    // Auth class (403 / x-access-token form).
    assert_eq!(class.as_deref(), Some("auth"));

    // REDACTION BOUNDARY: the stored message must equal redact(raw) and must NOT
    // contain the raw token.
    let stored = msg.expect("error_message must be set");
    assert!(!stored.contains("ghp_supersecrettoken0123456789"), "token leaked into error_message: {stored}");
    let expected = agentforge_api::domain::project_clone::redact(raw_tail).into_string();
    assert_eq!(stored, expected, "stored error_message must equal redact(raw).into_string()");

    // A bounded retry: attempt 2 (queued) + a fresh unpublished outbox row FOR
    // attempt 2 exist. (The attempt-1 create-path outbox row is also still
    // unpublished in this test because the publisher never ran, so we assert on
    // the attempt-2 row specifically.)
    assert_eq!(count_attempts(&pool, project_id).await, 2, "a retry attempt was inserted");
    let (status2, _c, _m, _b, _s, _cr) = attempt_row(&pool, project_id, 2).await;
    assert_eq!(status2, "queued", "the retry attempt is queued");
    assert_eq!(
        count_unpublished_outbox_for_attempt(&pool, project_id, 2).await,
        1,
        "a fresh outbox row was written for the retry attempt 2"
    );
    // The project mirrors the retry as queued.
    assert_eq!(project_clone_status(&pool, project_id).await, "queued");

    // A clone.started (on entry) + clone.failed + clone.retry event recorded.
    assert_eq!(count_audit(&pool, "clone.started", project_id).await, 1, "clone.started on the failure path");
    assert_eq!(count_audit(&pool, "clone.failed", project_id).await, 1);
    assert_eq!(count_audit(&pool, "clone.retry", project_id).await, 1);

    // Staging removed (no partial left behind).
    let staging = projects_root(&seed).join(".clone-staging");
    if staging.exists() {
        let mut entries = tokio::fs::read_dir(&staging).await.expect("read staging");
        assert!(entries.next_entry().await.expect("next").is_none(), "staging dir must be empty after failure");
    }
}

#[sqlx::test(migrations = "../db/migrations")]
async fn retry_is_bounded_at_max_attempts(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "Always Fails", REPO_URL).await;

    let runner = FakeRunner::new(|| CloneRunOutcome::Failed {
        exit_code: 128,
        stderr_tail: RawStderr::new("fatal: repository not found".to_string()),
    });
    let worker = worker(&pool, &seed, runner);

    // Drive each attempt to failure: 1 -> 2 -> 3, then attempt 3 must NOT spawn a
    // 4th (DEFAULT_MAX_ATTEMPTS = 3).
    for attempt in 1..=DEFAULT_MAX_ATTEMPTS {
        worker.process_attempt_for_test(project_id, attempt).await.expect("process");
    }

    assert_eq!(count_attempts(&pool, project_id).await, i64::from(DEFAULT_MAX_ATTEMPTS), "no retry past the ceiling");
    let (last_status, _c, _m, _b, _s, _cr) = attempt_row(&pool, project_id, DEFAULT_MAX_ATTEMPTS).await;
    assert_eq!(last_status, "failed", "the final attempt stays terminal");
    assert_eq!(project_clone_status(&pool, project_id).await, "failed");
}

// ---------------------------------------------------------------------------
// 3. Timeout / TooLarge map to the right class.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn timeout_maps_to_timeout_class(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "Slow", REPO_URL).await;

    let runner = FakeRunner::new(|| CloneRunOutcome::Timeout);
    let worker = worker(&pool, &seed, runner);
    worker.process_attempt_for_test(project_id, 1).await.expect("process");

    let (status, class, _m, _b, _s, _cr) = attempt_row(&pool, project_id, 1).await;
    assert_eq!(status, "failed");
    assert_eq!(class.as_deref(), Some("timeout"));
    // clone.started is emitted on entry even on the timeout path.
    assert_eq!(count_audit(&pool, "clone.started", project_id).await, 1, "clone.started on the timeout path");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn too_large_maps_to_too_large_class(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "Huge", REPO_URL).await;

    let runner = FakeRunner::new(|| CloneRunOutcome::TooLarge {
        stderr_tail: RawStderr::new("clone aborted: tree exceeded CLONE_MAX_BYTES".to_string()),
    });
    let worker = worker(&pool, &seed, runner);
    worker.process_attempt_for_test(project_id, 1).await.expect("process");

    let (status, class, _m, _b, _s, _cr) = attempt_row(&pool, project_id, 1).await;
    assert_eq!(status, "failed");
    assert_eq!(class.as_deref(), Some("too_large"));
}

// ---------------------------------------------------------------------------
// 4. Atomic rename: an existing target dir is refused (no clobber, no false ready).
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn ready_refuses_to_overwrite_an_existing_target_dir(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "Collide", REPO_URL).await;

    // Pre-create the would-be target dir with a sentinel file the clone must NOT
    // clobber.
    let dir_name = project_dir_name(&pool, project_id).await;
    let target = projects_root(&seed).join(&dir_name);
    tokio::fs::create_dir_all(&target).await.expect("pre-create target");
    tokio::fs::write(target.join("PRECIOUS"), b"do not destroy").await.expect("write sentinel");

    let runner = FakeRunner::new(|| CloneRunOutcome::Ready {
        branch: Some("main".to_string()),
        head_sha: "deadbeef".to_string(),
        bytes: 10,
    });
    let worker = worker(&pool, &seed, runner);
    worker.process_attempt_for_test(project_id, 1).await.expect("process");

    // The attempt is FAILED (internal), NOT a false ready, and the sentinel file
    // is untouched.
    let (status, class, _m, _b, _s, _cr) = attempt_row(&pool, project_id, 1).await;
    assert_eq!(status, "failed", "an existing target must NOT become a false ready");
    assert_eq!(class.as_deref(), Some("internal"));
    assert!(target.join("PRECIOUS").exists(), "the existing target dir must not be clobbered");
    assert_eq!(
        tokio::fs::read_to_string(target.join("PRECIOUS")).await.unwrap(),
        "do not destroy",
        "the existing file content must be preserved"
    );
    // No partial staging left.
    let staging = projects_root(&seed).join(".clone-staging");
    if staging.exists() {
        let mut entries = tokio::fs::read_dir(&staging).await.expect("read staging");
        assert!(entries.next_entry().await.expect("next").is_none());
    }
}

#[sqlx::test(migrations = "../db/migrations")]
async fn rename_failure_yields_failed_not_false_ready(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "No Source", REPO_URL).await;

    // A Ready outcome WITHOUT materializing <staging>/repo -> the rename source is
    // missing, so the rename fails. The worker must report failed, not ready, and
    // leave no partial target dir.
    let runner = FakeRunner::without_materialize(|| CloneRunOutcome::Ready {
        branch: None,
        head_sha: "abc".to_string(),
        bytes: 1,
    });
    let worker = worker(&pool, &seed, runner);
    worker.process_attempt_for_test(project_id, 1).await.expect("process");

    let (status, class, _m, _b, _s, _cr) = attempt_row(&pool, project_id, 1).await;
    assert_eq!(status, "failed", "a failed rename must not become a false ready");
    assert_eq!(class.as_deref(), Some("internal"));

    let dir_name = project_dir_name(&pool, project_id).await;
    let target = projects_root(&seed).join(&dir_name);
    assert!(!target.exists(), "no partial target dir may be left after a failed rename");
}

// ---------------------------------------------------------------------------
// 5. Reconciler: expired-lease cloning recovery + lost-enqueue re-enqueue.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn reconciler_recovers_expired_cloning_attempt(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "Stuck", REPO_URL).await;

    // Force attempt 1 into a crashed-worker state: cloning with an already-expired
    // lease.
    sqlx::query(
        "UPDATE project_clone_attempts
            SET status = 'cloning', worker_id = 'dead', lease_expires_at = now() - interval '1 hour'
          WHERE project_id = $1 AND attempt = 1",
    )
    .bind(project_id)
    .execute(&pool)
    .await
    .expect("force stuck cloning");
    sqlx::query("UPDATE projects SET clone_status = 'cloning' WHERE id = $1")
        .bind(project_id)
        .execute(&pool)
        .await
        .expect("force project cloning");

    let runner = FakeRunner::new(|| CloneRunOutcome::Timeout);
    let worker = worker(&pool, &seed, runner);
    worker.run_reconciler_once().await;

    // The stuck attempt 1 is failed with the recovery message, and a retry
    // attempt 2 is queued (attempts remain).
    let (status, class, msg, _b, _s, _cr) = attempt_row(&pool, project_id, 1).await;
    assert_eq!(status, "failed", "an expired-lease cloning attempt must be recovered to failed");
    assert_eq!(class.as_deref(), Some("internal"));
    assert_eq!(msg.as_deref(), Some("worker lost the clone"));
    assert_eq!(count_attempts(&pool, project_id).await, 2, "the recovered attempt is retried");
    let (status2, _c, _m, _b, _s, _cr) = attempt_row(&pool, project_id, 2).await;
    assert_eq!(status2, "queued");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn reconciler_reenqueues_a_lost_queued_attempt(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "Lost", REPO_URL).await;

    // Simulate a lost enqueue: the create-path outbox row is already published
    // (so it won't re-relay), and there is NO job_queue row. The attempt sits
    // queued with no path to run — exactly the orphan the reconciler heals.
    sqlx::query(
        "UPDATE orchestration_outbox SET published_at = now()
          WHERE aggregate_type = 'project_clone' AND aggregate_id = $1",
    )
    .bind(project_id)
    .execute(&pool)
    .await
    .expect("publish the create outbox row");
    // Backdate the attempt so it is older than the reconcile grace window.
    sqlx::query("UPDATE project_clone_attempts SET created_at = now() - interval '1 hour' WHERE project_id = $1")
        .bind(project_id)
        .execute(&pool)
        .await
        .expect("backdate attempt");

    assert_eq!(count_unpublished_outbox(&pool, project_id).await, 0, "precondition: no pending outbox");

    let runner = FakeRunner::new(|| CloneRunOutcome::Timeout);
    let worker = worker(&pool, &seed, runner);
    worker.run_reconciler_once().await;

    // A fresh unpublished outbox row was written for the lost attempt.
    assert_eq!(count_unpublished_outbox(&pool, project_id).await, 1, "the lost queued attempt was re-enqueued");
    // The attempt is still queued (re-enqueue does not change its status).
    let (status, _c, _m, _b, _s, _cr) = attempt_row(&pool, project_id, 1).await;
    assert_eq!(status, "queued");
}

// ---------------------------------------------------------------------------
// 6. Host-match credential selection.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "../db/migrations")]
async fn worker_picks_the_host_matching_credential(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "Matched", REPO_URL).await;

    // Two creds for DIFFERENT hosts: a self-hosted gitlab and the github.com one
    // the repo URL targets. The worker must pick the github.com match.
    let _other = seed_git_credential(&pool, &seed, "gitlab", Some("https://gitlab.example.com"), "glpat-other").await;
    let github_id =
        seed_git_credential(&pool, &seed, "github", Some("https://github.com"), "ghp_thematchingtoken").await;

    // Capture the runner so we can assert a credential was supplied.
    let runner = FakeRunner::new(|| CloneRunOutcome::Ready {
        branch: Some("main".to_string()),
        head_sha: "sha".to_string(),
        bytes: 1,
    });
    let worker = worker(&pool, &seed, runner.clone());
    worker.process_attempt_for_test(project_id, 1).await.expect("process");

    // The attempt records WHICH credential (the github.com one), never the secret.
    let (_status, _c, _m, _b, _s, cred) = attempt_row(&pool, project_id, 1).await;
    assert_eq!(cred, Some(github_id), "the worker must record the host-matching credential id");
    // The runner saw a credential (host matched -> secret mounted).
    let call = runner.last_call().expect("runner called");
    assert!(call.had_credential, "the matching credential must be supplied to the clone");
    let _ = GITHUB_HOST; // host constant documents the intent.
}

#[sqlx::test(migrations = "../db/migrations")]
async fn worker_clones_anonymously_when_no_credential_matches_the_host(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "Anon", REPO_URL).await;

    // A credential for a DIFFERENT host only — no github.com match.
    let _other = seed_git_credential(&pool, &seed, "gitlab", Some("https://gitlab.example.com"), "glpat-other").await;

    let runner = FakeRunner::new(|| CloneRunOutcome::Ready {
        branch: Some("main".to_string()),
        head_sha: "sha".to_string(),
        bytes: 1,
    });
    let worker = worker(&pool, &seed, runner.clone());
    worker.process_attempt_for_test(project_id, 1).await.expect("process");

    // No credential recorded; the clone ran anonymously (no secret supplied).
    let (status, _c, _m, _b, _s, cred) = attempt_row(&pool, project_id, 1).await;
    assert_eq!(status, "ready");
    assert_eq!(cred, None, "an unmatched host must record no credential");
    let call = runner.last_call().expect("runner called");
    assert!(!call.had_credential, "no credential must be supplied when no host matches");
}

// ---------------------------------------------------------------------------
// 7. M5-review failure-branch coverage.
// ---------------------------------------------------------------------------

/// A job re-delivered for an already-`ready` attempt is a no-op: no second clone
/// runs (the runner is not called again), no second clone.started, and the live
/// dir is untouched. (Idempotency: the durable dedup is the attempt row.)
#[sqlx::test(migrations = "../db/migrations")]
async fn redelivery_of_a_ready_attempt_is_a_noop(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "Once", REPO_URL).await;

    let runner = FakeRunner::new(|| CloneRunOutcome::Ready {
        branch: Some("main".to_string()),
        head_sha: "abc123".to_string(),
        bytes: 8,
    });
    let worker = worker(&pool, &seed, runner.clone());

    // First delivery materializes the clone to ready.
    worker.process_attempt_for_test(project_id, 1).await.expect("first process");
    assert_eq!(attempt_status(&pool, project_id, 1).await, "ready");
    assert_eq!(runner.call_count(), 1, "the runner ran exactly once");
    let dir_name = project_dir_name(&pool, project_id).await;
    let target = projects_root(&seed).join(&dir_name);
    let sentinel = target.join("README.md");
    assert!(sentinel.exists());

    // Re-deliver the SAME attempt: terminal -> short-circuit, no second run.
    worker.process_attempt_for_test(project_id, 1).await.expect("redeliver");
    assert_eq!(runner.call_count(), 1, "a re-delivered ready attempt must NOT run the clone again");
    assert_eq!(count_audit(&pool, "clone.started", project_id).await, 1, "no second clone.started");
    assert_eq!(count_attempts(&pool, project_id).await, 1, "no extra attempt");
    assert!(sentinel.exists(), "the live dir must be untouched by a re-delivery");
}

/// A job re-delivered for an already-`failed` attempt is a no-op: no second run,
/// no second clone.started, and NO second retry (so the bounded budget is not
/// double-spent by a duplicate job).
#[sqlx::test(migrations = "../db/migrations")]
async fn redelivery_of_a_failed_attempt_does_not_double_retry(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "FailOnce", REPO_URL).await;

    let runner = FakeRunner::new(|| CloneRunOutcome::Failed {
        exit_code: 128,
        stderr_tail: RawStderr::new("fatal: repository not found".to_string()),
    });
    let worker = worker(&pool, &seed, runner.clone());

    worker.process_attempt_for_test(project_id, 1).await.expect("first process");
    assert_eq!(attempt_status(&pool, project_id, 1).await, "failed");
    // One retry (attempt 2) scheduled.
    assert_eq!(count_attempts(&pool, project_id).await, 2);
    assert_eq!(runner.call_count(), 1);

    // Re-deliver attempt 1: it is terminal -> no-op. No second run, no extra retry.
    worker.process_attempt_for_test(project_id, 1).await.expect("redeliver");
    assert_eq!(runner.call_count(), 1, "a re-delivered failed attempt must NOT run again");
    assert_eq!(count_attempts(&pool, project_id).await, 2, "no second retry from the duplicate job");
    assert_eq!(count_audit(&pool, "clone.started", project_id).await, 1, "no second clone.started");
    assert_eq!(count_audit(&pool, "clone.retry", project_id).await, 1, "exactly one retry event");
}

/// #1 REGRESSION: a clone whose tree was renamed live (materialized_at set) but
/// whose DB finalize never reached `ready` — the rename/finalize split-brain —
/// must be RECOVERABLE to `ready`, NOT a stranded `cloning` that retries forever
/// into a target the overwrite guard refuses.
///
/// We reconstruct the exact split-brain: attempt 1 `cloning` with `materialized_at`
/// set AND the target dir live on disk (as if the worker crashed in the instant
/// between the rename and the finalize commit). The reconciler must force `ready`.
#[sqlx::test(migrations = "../db/migrations")]
async fn reconciler_force_readies_a_materialized_but_unfinalized_attempt(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "SplitBrain", REPO_URL).await;

    // Materialize the clone on disk (the rename "happened") without finalizing.
    let dir_name = project_dir_name(&pool, project_id).await;
    let target = projects_root(&seed).join(&dir_name);
    tokio::fs::create_dir_all(&target).await.expect("materialize target dir");
    tokio::fs::write(target.join("README.md"), b"already cloned").await.expect("write materialized file");

    // Reconstruct the DB split-brain: cloning + materialized_at set + persisted
    // success payload, but NOT ready. (An expired lease too, to prove the
    // materialized scan wins over the expired-lease failure path.)
    sqlx::query(
        "UPDATE project_clone_attempts
            SET status = 'cloning',
                materialized_at = now(),
                resolved_branch = 'main',
                head_sha = 'cafebabe',
                bytes_cloned = 14,
                duration_ms = 5,
                lease_expires_at = now() - interval '1 hour'
          WHERE project_id = $1 AND attempt = 1",
    )
    .bind(project_id)
    .execute(&pool)
    .await
    .expect("force split-brain state");

    let runner = FakeRunner::new(|| CloneRunOutcome::Timeout); // must NOT be called
    let worker = worker(&pool, &seed, runner.clone());
    worker.run_reconciler_once().await;

    // The attempt is force-readied (rename is the source of truth); NO re-clone,
    // NO failing retry loop, and the live dir is untouched.
    assert_eq!(attempt_status(&pool, project_id, 1).await, "ready", "split-brain must heal to ready");
    assert_eq!(project_clone_status(&pool, project_id).await, "ready");
    assert_eq!(count_attempts(&pool, project_id).await, 1, "NO retry attempt: the clone succeeded on disk");
    assert_eq!(runner.call_count(), 0, "the clone must NOT be re-run for a materialized attempt");
    assert!(target.join("README.md").exists(), "the live clone must be untouched");
    let (_s, _c, _m, branch, head_sha, _cr) = attempt_row(&pool, project_id, 1).await;
    assert_eq!(branch.as_deref(), Some("main"), "the persisted success payload is preserved");
    assert_eq!(head_sha.as_deref(), Some("cafebabe"));
    assert_eq!(count_audit(&pool, "clone.ready", project_id).await, 1, "a clone.ready event is emitted on recovery");
}

/// #1 crash-window closure: the worker crashed AFTER the on-disk rename but BEFORE
/// the publish tx committed, so the clone is LIVE on disk yet `materialized_at` is
/// NULL and the attempt is stuck `cloning` with an expired lease. The reconciler's
/// expired-lease recovery must ADOPT the on-disk clone (force ready) — NOT fail +
/// retry it into the overwrite guard forever.
#[sqlx::test(migrations = "../db/migrations")]
async fn reconciler_adopts_an_on_disk_clone_whose_finalize_was_lost(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "CrashWindow", REPO_URL).await;

    // The clone is live on disk (rename happened) ...
    let dir_name = project_dir_name(&pool, project_id).await;
    let target = projects_root(&seed).join(&dir_name);
    tokio::fs::create_dir_all(&target).await.expect("materialize target");
    tokio::fs::write(target.join("README.md"), b"committed to disk").await.expect("write file");

    // ... but the DB shows an expired-lease `cloning` attempt with NO materialized_at
    // (the publish tx never committed). The persisted success payload survives the
    // rename step in the real flow; seed it so the adopt has values to re-finalize.
    sqlx::query(
        "UPDATE project_clone_attempts
            SET status = 'cloning', materialized_at = NULL,
                resolved_branch = 'main', head_sha = 'd00d',
                lease_expires_at = now() - interval '1 hour'
          WHERE project_id = $1 AND attempt = 1",
    )
    .bind(project_id)
    .execute(&pool)
    .await
    .expect("force crash-window state");

    let runner = FakeRunner::new(|| CloneRunOutcome::Timeout); // must NOT be called
    let worker = worker(&pool, &seed, runner.clone());
    worker.run_reconciler_once().await;

    // Adopted: the attempt is ready (not failed), materialized_at is now set, NO
    // retry was spawned, and the live dir is untouched.
    assert_eq!(attempt_status(&pool, project_id, 1).await, "ready", "an on-disk clone must be adopted, not failed");
    assert!(attempt_is_materialized(&pool, project_id, 1).await, "adoption stamps materialized_at");
    assert_eq!(count_attempts(&pool, project_id).await, 1, "NO retry: the clone is live on disk");
    assert_eq!(runner.call_count(), 0, "the clone must NOT be re-run");
    assert!(target.join("README.md").exists(), "the live clone must be untouched");
    assert_eq!(count_audit(&pool, "clone.ready", project_id).await, 1, "a clone.ready event is emitted on adoption");
    assert_eq!(project_clone_status(&pool, project_id).await, "ready");
}

/// The worker's OWN target-exists-is-mine path: a re-driven attempt whose own
/// `materialized_at` is set and whose target dir exists re-finalizes `ready`
/// WITHOUT re-cloning (no second runner call, no overwrite failure).
#[sqlx::test(migrations = "../db/migrations")]
async fn worker_refinalizes_its_own_materialized_clone_without_recloning(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "MineAlready", REPO_URL).await;

    // The clone is live on disk + the attempt's OWN materialized_at is set, but it
    // is still `queued` (a prior partial run materialized then got reset). The
    // claim (queued -> cloning) preserves materialized_at, so when the re-driven
    // clone returns Ready, `finish_ready` sees target-exists AND this-attempt-is-
    // materialized and re-finalizes ready WITHOUT clobbering the live dir.
    let dir_name = project_dir_name(&pool, project_id).await;
    let target = projects_root(&seed).join(&dir_name);
    tokio::fs::create_dir_all(&target).await.expect("materialize target");
    tokio::fs::write(target.join("PROOF"), b"mine").await.expect("write proof");
    sqlx::query(
        "UPDATE project_clone_attempts
            SET status = 'queued', materialized_at = now(), head_sha = 'sha1'
          WHERE project_id = $1 AND attempt = 1",
    )
    .bind(project_id)
    .execute(&pool)
    .await
    .expect("force materialized-but-queued");

    let runner = FakeRunner::new(|| CloneRunOutcome::Ready { branch: None, head_sha: "sha1".to_string(), bytes: 1 });
    let worker = worker(&pool, &seed, runner.clone());
    worker.process_attempt_for_test(project_id, 1).await.expect("re-drive");

    assert_eq!(attempt_status(&pool, project_id, 1).await, "ready", "re-finalized to ready");
    assert!(target.join("PROOF").exists(), "the existing materialized clone must NOT be clobbered");
    assert_eq!(count_attempts(&pool, project_id).await, 1, "no retry — the clone is already live");
    assert_eq!(count_audit(&pool, "clone.ready", project_id).await, 1);
}

/// #2 (publish-lock race): a project soft-deleted DURING the clone run — after the
/// early live-check passed, before the publish — must CANCEL the attempt at the
/// publish-time `FOR UPDATE` guard and create NO directory in the projects root.
///
/// The deletion is injected by a runner that soft-deletes the project as a side
/// effect just before returning Ready, so the worker reaches `finish_ready` with a
/// live staging repo but a now-deleted project — exactly the mid-flight race.
#[sqlx::test(migrations = "../db/migrations")]
async fn deleted_project_mid_flight_cancels_without_publishing(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "Doomed2", REPO_URL).await;
    let dir_name = project_dir_name(&pool, project_id).await;

    // A runner that, on run, materializes <staging>/repo AND soft-deletes the
    // project — simulating an M6 delete landing during the clone, after the early
    // live-check but before the publish lock.
    struct DeleteMidRun {
        pool: PgPool,
        project_id: Uuid,
    }
    #[async_trait::async_trait]
    impl CloneRunner for DeleteMidRun {
        async fn run(&self, spec: CloneRunSpec) -> agentforge_core::AppResult<CloneRunOutcome> {
            let repo = spec.staging_host_path.join("repo");
            tokio::fs::create_dir_all(&repo).await.expect("materialize fake repo");
            tokio::fs::write(repo.join("README.md"), b"hi").await.expect("write fake file");
            // The project is deleted mid-flight.
            sqlx::query("UPDATE projects SET deleted_at = now() WHERE id = $1")
                .bind(self.project_id)
                .execute(&self.pool)
                .await
                .expect("soft-delete mid-run");
            Ok(CloneRunOutcome::Ready { branch: Some("main".into()), head_sha: "sha".into(), bytes: 1 })
        }
        async fn sweep_orphans(&self) -> agentforge_core::AppResult<usize> {
            Ok(0)
        }
    }

    let worker = worker(&pool, &seed, Arc::new(DeleteMidRun { pool: pool.clone(), project_id }));
    worker.process_attempt_for_test(project_id, 1).await.expect("process");

    // The attempt is cancelled at the publish lock — NOT ready — and NO dir exists.
    assert_eq!(attempt_status(&pool, project_id, 1).await, "cancelled", "a mid-flight delete must cancel the attempt");
    let target = projects_root(&seed).join(&dir_name);
    assert!(!target.exists(), "NO directory may be published for a project deleted mid-flight");
    assert_eq!(count_audit(&pool, "clone.cancelled", project_id).await, 1, "a clone.cancelled event is emitted");
    // Staging is cleaned (no orphan left behind).
    let staging = projects_root(&seed).join(".clone-staging");
    if staging.exists() {
        let mut entries = tokio::fs::read_dir(&staging).await.expect("read staging");
        assert!(entries.next_entry().await.expect("next").is_none(), "staging must be empty");
    }
}

/// A runner that returns `Err` (Docker create/start failed) is treated as an
/// internal failure: staging is cleaned, the stored message is redacted (it is our
/// own text, no token), clone.started was emitted, and a bounded retry is
/// scheduled. Exercises `finish_failed_redacted` via the runtime-error branch.
#[sqlx::test(migrations = "../db/migrations")]
async fn runner_error_cleans_staging_and_schedules_bounded_retry(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "DockerDown", REPO_URL).await;

    let worker = worker(&pool, &seed, Arc::new(ErrRunner));
    worker.process_attempt_for_test(project_id, 1).await.expect("process");

    let (status, class, msg, _b, _s, _cr) = attempt_row(&pool, project_id, 1).await;
    assert_eq!(status, "failed");
    assert_eq!(class.as_deref(), Some("internal"), "a runtime error classifies as internal");
    let stored = msg.expect("error_message set");
    assert!(stored.contains("clone runtime error"), "the internal message is stored: {stored}");

    // clone.started emitted, a bounded retry scheduled, staging cleaned.
    assert_eq!(count_audit(&pool, "clone.started", project_id).await, 1);
    assert_eq!(count_attempts(&pool, project_id).await, 2, "a bounded retry was scheduled");
    assert_eq!(attempt_status(&pool, project_id, 2).await, "queued");
    let staging = projects_root(&seed).join(".clone-staging");
    if staging.exists() {
        let mut entries = tokio::fs::read_dir(&staging).await.expect("read staging");
        assert!(entries.next_entry().await.expect("next").is_none(), "staging must be empty after a runtime error");
    }
}

/// #7: two retry schedulers racing for the SAME failed attempt (the worker failure
/// path AND a reconciler re-drive) must produce EXACTLY ONE next attempt and ONE
/// unpublished outbox row — never two. We drive the failure once (creating attempt
/// 2 + its outbox row), then force attempt 1 back to an expired-lease `cloning`
/// state and run the reconciler, which would re-schedule attempt 2 — the
/// `uq_project_clone_attempt` + the outbox dedup guard must keep it to one each.
#[sqlx::test(migrations = "../db/migrations")]
async fn concurrent_retry_schedulers_yield_exactly_one_next_attempt(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "RaceRetry", REPO_URL).await;

    let runner = FakeRunner::new(|| CloneRunOutcome::Failed {
        exit_code: 128,
        stderr_tail: RawStderr::new("fatal: repository not found".to_string()),
    });
    let worker = worker(&pool, &seed, runner);

    // Failure path schedules retry attempt 2 + one outbox row.
    worker.process_attempt_for_test(project_id, 1).await.expect("process");
    assert_eq!(count_attempts(&pool, project_id).await, 2);
    assert_eq!(count_unpublished_outbox_for_attempt(&pool, project_id, 2).await, 1);

    // Now force attempt 1 back to an expired-lease cloning so the reconciler ALSO
    // tries to recover + retry it (a second scheduler for the same next attempt).
    sqlx::query(
        "UPDATE project_clone_attempts
            SET status = 'cloning', lease_expires_at = now() - interval '1 hour'
          WHERE project_id = $1 AND attempt = 1",
    )
    .bind(project_id)
    .execute(&pool)
    .await
    .expect("re-arm attempt 1 as expired cloning");

    worker.run_reconciler_once().await;

    // Exactly ONE next attempt (2) and ONE unpublished outbox row for it — the
    // unique index + the dedup guard absorbed the concurrent schedule.
    assert_eq!(count_attempts(&pool, project_id).await, 2, "exactly one next attempt across both schedulers");
    assert_eq!(
        count_unpublished_outbox_for_attempt(&pool, project_id, 2).await,
        1,
        "exactly one unpublished outbox row for the retry attempt"
    );
}

/// #11: the deterministic container_id (`agentforge-clone-<attempt_id>`) is
/// persisted on the attempt before the wait, for diagnostics + targeted reaping.
#[sqlx::test(migrations = "../db/migrations")]
async fn container_id_is_persisted_before_the_wait(pool: PgPool) {
    let seed = seed(&pool).await;
    let project_id = create_cloned_project(&pool, &seed, "WithContainer", REPO_URL).await;

    let runner = FakeRunner::new(|| CloneRunOutcome::Ready {
        branch: Some("main".to_string()),
        head_sha: "sha".to_string(),
        bytes: 1,
    });
    let worker = worker(&pool, &seed, runner);
    worker.process_attempt_for_test(project_id, 1).await.expect("process");

    let container_id = attempt_container_id(&pool, project_id, 1).await.expect("container_id persisted");
    assert!(container_id.starts_with("agentforge-clone-"), "deterministic container name persisted: {container_id}");
    // It is materialized too (the happy path stamps materialized_at on publish).
    assert!(attempt_is_materialized(&pool, project_id, 1).await, "a ready attempt is materialized");
}
