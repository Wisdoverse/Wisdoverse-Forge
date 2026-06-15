//! Project-clone worker + reconciler (design spec §6.3, §6.6, §6.7, §7, §8, §12).
//!
//! Lives in the **api** crate (not `jobs`) because it needs api-side types the
//! jobs crate cannot depend on without a cycle: the `RedactedError`/`CloneStatus`/
//! `CloneErrorClass`/`WorkspaceDirName` domain types, the
//! `ProjectCloneRepository`, and `GitCredentialService::resolve_for_host`. It
//! consumes the `agentforge_jobs::queue` primitives and the
//! `agentforge_platform::CloneRuntime` directly.
//!
//! # What it owns
//!
//! Every clone-attempt status transition (the spec §7 state machine):
//!
//! ```text
//! queued -> cloning -> ready
//!               \-> failed -> (bounded retry) -> queued
//! ```
//!
//! On dequeue it claims the `queued` attempt (the durable, exactly-once-per-
//! attempt guard is the `uq_project_clone_attempt(project_id, attempt)` row — NOT
//! the transient job `unique_key`), transitions it to `cloning` with a lease,
//! resolves the host-matched credential, materializes the secret bytes ONLY at
//! container launch, runs the ephemeral clone container via [`CloneRunner`], and
//! maps the outcome onto the attempt + project state:
//!
//! - `Ready`   → atomic same-filesystem rename of `staging/repo` →
//!   `<projects_root>/<workspace_dir_name>`, attempt+project `ready`, staging
//!   removed, `clone.ready` audit + WS + metrics.
//! - `Failed`/`Timeout`/`TooLarge` → redact the raw stderr (M1 `RedactedError`),
//!   classify (M1 `CloneErrorClass`), attempt+project `failed`, staging removed,
//!   `clone.failed` audit + WS + metrics, then a BOUNDED retry (a new attempt +
//!   outbox row) if attempts remain.
//!
//! # Reconciler (the polling fallback; `pg_notify` is wake-up only)
//!
//! - Crashed-worker `cloning` attempts past their lease are recovered: the
//!   container is force-removed via the runtime sweep and the attempt is failed
//!   (then retried if attempts remain), so status can never stick at `cloning`.
//! - `queued` attempts whose enqueue was lost (no live job + no pending outbox)
//!   are re-enqueued.
//! - `sweep_orphans` reaps crashed-worker clone containers on startup + each pass.
//!
//! # Credential lifetime
//!
//! The decrypted credential bytes are resolved + materialized to the host secret
//! file ONLY at the moment the container is launched (inside `run_clone`, which
//! owns the 0644-in-0700-root mechanics and unlinks the file on every exit path).
//! The worker holds the [`SecretBytes`] for exactly the `run` call and never
//! logs, serializes, or retains it — it is dropped (and `zeroize`-scrubbed) the
//! instant `run_clone` returns. The raw `RawStderr` is redacted BEFORE it is ever
//! persisted, so a token can never reach `error_message` (the M4-deferred
//! redaction-before-persist guarantee, tested here).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use agentforge_core::{AppResult, OrgId, ProjectId};
use agentforge_db::entities::ProjectCloneAttempt;
use agentforge_platform::{CloneDockerBackend, CloneRunOutcome, CloneRunSpec, CloneRuntime, RawStderr};
use async_nats::Client;
use chrono::Utc;
use sqlx::PgPool;
use tokio::sync::watch;
use uuid::Uuid;

use crate::domain::agent_workspace::{WorkspaceMountScope, resolve_agent_workspace_paths};
use crate::domain::project_clone::{CloneAttemptStatus, CloneErrorClass, CloneOutboxPayload, WorkspaceDirName, redact};
use crate::repositories::project_clone::{CloneFailure, CloneSuccess, ProjectCloneRepository, ReconcileCandidate};
use crate::services::audit::AuditService;
use crate::services::git_credential::GitCredentialService;

/// Default clone image ref. Override with `CLONE_IMAGE`.
pub const DEFAULT_CLONE_IMAGE: &str = "agentforge-clone:latest";

/// Audit `resource_type` for every clone lifecycle event.
const CLONE_AUDIT_RESOURCE: &str = "project_clone";

/// Subdirectory under the projects root that holds per-attempt staging dirs.
/// On the SAME filesystem as the projects root so the final rename is atomic.
const CLONE_STAGING_SUBDIR: &str = ".clone-staging";

/// The name the entrypoint clones into under the staging dir (`$CLONE_DEST/repo`).
const STAGING_REPO_SUBDIR: &str = "repo";

/// Hard ceiling on retries: attempt 1 plus this many retries. Spec §8 "bounded
/// retry up to N attempts."
pub const DEFAULT_MAX_ATTEMPTS: i32 = 3;

/// The clone execution dependency, abstracted so the worker is unit-testable
/// WITHOUT a real Docker daemon. The live impl is [`LiveCloneRunner`] over
/// [`CloneRuntime`]; tests inject a fake returning a scripted [`CloneRunOutcome`].
#[async_trait::async_trait]
pub trait CloneRunner: Send + Sync {
    /// Run one ephemeral clone, consuming the spec (and its credential secret).
    async fn run(&self, spec: CloneRunSpec) -> AppResult<CloneRunOutcome>;
    /// Reap crashed-worker clone containers; returns how many were removed.
    async fn sweep_orphans(&self) -> AppResult<usize>;
}

/// Live [`CloneRunner`] backed by the platform [`CloneRuntime`].
pub struct LiveCloneRunner<B: CloneDockerBackend + 'static> {
    runtime: CloneRuntime<B>,
}

impl<B: CloneDockerBackend + 'static> LiveCloneRunner<B> {
    pub fn new(runtime: CloneRuntime<B>) -> Self {
        Self { runtime }
    }
}

#[async_trait::async_trait]
impl<B: CloneDockerBackend + 'static> CloneRunner for LiveCloneRunner<B> {
    async fn run(&self, spec: CloneRunSpec) -> AppResult<CloneRunOutcome> {
        self.runtime.run_clone(spec).await
    }

    async fn sweep_orphans(&self) -> AppResult<usize> {
        self.runtime.sweep_orphans().await
    }
}

/// Tunable knobs for the worker. All have safe defaults; the server bin overrides
/// from config/env.
#[derive(Debug, Clone)]
pub struct CloneWorkerConfig {
    /// Clone image ref handed to the container.
    pub image: String,
    /// Host root under which org/workspace projects trees live (mirrors
    /// `AGENTFORGE_WORKSPACE_ROOT`).
    pub workspace_root: String,
    /// Backend-controlled 0700 secret root the credential file is written under
    /// (OUTSIDE the projects tree). The runtime owns the file mechanics.
    pub secret_root: PathBuf,
    /// Hard wall-clock timeout per clone.
    pub timeout: Duration,
    /// Worker lease TTL: how long a claimed `cloning` attempt is trusted before
    /// the reconciler treats it as a crashed worker. Must exceed `timeout`.
    pub lease_ttl: Duration,
    /// Cloned-tree size cap (`CLONE_MAX_BYTES`); `None` ⇒ runtime default.
    pub max_bytes: Option<u64>,
    /// Max attempts (attempt 1 + retries). `failed` past this stays terminal.
    pub max_attempts: i32,
    /// Base backoff applied between retries (advisory; the retry attempt is
    /// queued immediately but the worker logs the intended backoff).
    pub retry_backoff: Duration,
    /// Idle poll interval when the queue is empty (pg_notify is wake-up only).
    pub poll_interval: Duration,
    /// How often the reconciler sweep runs.
    pub reconcile_interval: Duration,
}

impl Default for CloneWorkerConfig {
    fn default() -> Self {
        Self {
            image: DEFAULT_CLONE_IMAGE.to_string(),
            workspace_root: crate::services::agent_workspace::workspace_root_from_env(),
            secret_root: PathBuf::from("/tmp/agentforge/clone-secrets"),
            timeout: Duration::from_secs(600),
            lease_ttl: Duration::from_secs(900),
            max_bytes: None,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            retry_backoff: Duration::from_secs(30),
            poll_interval: Duration::from_millis(500),
            reconcile_interval: Duration::from_secs(60),
        }
    }
}

/// The project-clone worker + reconciler.
pub struct ProjectCloneWorker<R: CloneRunner> {
    pool: PgPool,
    repo: ProjectCloneRepository,
    credentials: Arc<GitCredentialService>,
    audit: AuditService,
    /// Realtime broadcast client; `None` disables WS emission (status + metrics
    /// still recorded). Mirrors the orchestration realtime pattern.
    realtime: Option<Client>,
    runner: Arc<R>,
    config: CloneWorkerConfig,
    worker_id: String,
}

impl<R: CloneRunner + 'static> ProjectCloneWorker<R> {
    pub fn new(
        pool: PgPool,
        credentials: Arc<GitCredentialService>,
        runner: Arc<R>,
        config: CloneWorkerConfig,
    ) -> Self {
        let worker_id = format!("project-clone-{}", Uuid::now_v7());
        Self {
            repo: ProjectCloneRepository::new(pool.clone()),
            audit: AuditService::from_pool(pool.clone()),
            realtime: None,
            pool,
            credentials,
            runner,
            config,
            worker_id,
        }
    }

    /// Attach a NATS client so status transitions broadcast over the WS channel.
    pub fn with_realtime(mut self, client: Option<Client>) -> Self {
        self.realtime = client;
        self
    }

    /// Run the dequeue loop + periodic reconciler until shutdown. Runs the
    /// startup reconciler + orphan sweep first so a crashed-worker restart never
    /// leaves a stuck attempt or a leaked credential-holding container.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        tracing::info!(worker_id = %self.worker_id, "project_clone worker starting");

        // Startup recovery: reap orphan containers + recover stuck attempts before
        // we begin draining new work.
        self.sweep_orphans_once().await;
        self.run_reconciler_once().await;

        let mut reconcile = tokio::time::interval(self.config.reconcile_interval);
        reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        reconcile.tick().await; // consume the immediate first tick

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!(worker_id = %self.worker_id, "project_clone worker shutting down");
                        break;
                    }
                }
                _ = reconcile.tick() => {
                    self.sweep_orphans_once().await;
                    self.run_reconciler_once().await;
                }
                _ = async {
                    match self.dequeue_and_process().await {
                        Ok(true) => {} // did work — loop again immediately
                        Ok(false) => tokio::time::sleep(self.config.poll_interval).await,
                        Err(err) => {
                            tracing::warn!(error = %err, "project_clone worker tick failed");
                            metrics::counter!("agentforge_project_clone_worker_errors_total").increment(1);
                            tokio::time::sleep(self.config.poll_interval).await;
                        }
                    }
                } => {}
            }
        }
    }

    /// Dequeue one `project_clone` job and process it. Returns whether work was
    /// done. The job is always `complete`d (deleted) afterward: the durable
    /// dedup is the attempt row, so the job is purely a wake-up — leaving it for
    /// queue-level retry would double-process. A genuine transient failure is
    /// recovered by the reconciler, not by re-running the same job.
    async fn dequeue_and_process(&self) -> AppResult<bool> {
        let job = agentforge_jobs::queue::dequeue(
            &self.pool,
            agentforge_core::clone_protocol::CLONE_JOB_QUEUE,
            &self.worker_id,
        )
        .await
        .map_err(|e| agentforge_core::AppError::from(anyhow::Error::from(e)))?;

        let Some(job) = job else {
            return Ok(false);
        };

        // Decode the identifier-only payload. A structurally-broken payload can
        // never process; complete the job (drop it) and move on rather than
        // re-locking it forever.
        let payload: CloneOutboxPayload = match serde_json::from_value(job.payload.clone()) {
            Ok(payload) => payload,
            Err(err) => {
                tracing::error!(job_id = %job.id, error = %err, "project_clone job payload undecodable; dropping");
                metrics::counter!("agentforge_project_clone_worker_errors_total").increment(1);
                self.complete_job(job.id).await;
                return Ok(true);
            }
        };

        let result = self.process_attempt(payload.project_id, payload.attempt, Some(job.id)).await;
        // Always complete (delete) the job — the attempt row is the durable state.
        self.complete_job(job.id).await;
        result.map(|_| true)
    }

    async fn complete_job(&self, job_id: Uuid) {
        if let Err(err) = agentforge_jobs::queue::complete(&self.pool, job_id).await {
            tracing::warn!(job_id = %job_id, error = %err, "failed to complete project_clone job");
        }
    }

    /// Drive ONE clone attempt through its full lifecycle, for tests + reconciler
    /// re-drives. Public so integration tests can run a single attempt against a
    /// real DB + a fake [`CloneRunner`] without standing up the dequeue loop.
    pub async fn process_attempt_for_test(&self, project_id: Uuid, attempt: i32) -> AppResult<()> {
        self.process_attempt(project_id, attempt, None).await
    }

    /// Drive ONE clone attempt through its full lifecycle. Idempotent: a
    /// terminal/already-claimed attempt short-circuits.
    async fn process_attempt(&self, project_id: Uuid, attempt: i32, job_id: Option<Uuid>) -> AppResult<()> {
        // Re-read the authoritative attempt row (never trust the payload snapshot).
        let Some(attempt_row) = self.repo.find_attempt(project_id, attempt).await? else {
            tracing::warn!(%project_id, attempt, "project_clone attempt row not found; skipping");
            return Ok(());
        };

        // Idempotency: a terminal attempt (ready/failed/cancelled) is already
        // done — the durable dedup is this row, not the job. Skip.
        let current = attempt_row.status.parse::<CloneAttemptStatus>().ok();
        if matches!(current, Some(s) if s.is_terminal()) {
            tracing::debug!(%project_id, attempt, status = %attempt_row.status, "attempt already terminal; skipping");
            return Ok(());
        }

        // The project may have been deleted between create and dequeue. Don't
        // recreate a directory for a dead project; fail the attempt closed.
        if !self.repo.project_is_live(ProjectId::from(project_id)).await? {
            tracing::info!(%project_id, attempt, "project no longer live; failing the clone attempt");
            self.repo
                .finalize_failed(
                    attempt_row.id,
                    &CloneFailure {
                        error_class: CloneErrorClass::Internal.as_str().to_string(),
                        error_message: "project was deleted before the clone ran".to_string(),
                        duration_ms: None,
                    },
                )
                .await?;
            return Ok(());
        }

        // Claim the attempt: queued -> cloning, take a lease, stamp worker/job.
        let lease_expires_at = Utc::now() + chrono::Duration::from_std(self.config.lease_ttl).unwrap_or_default();
        let claimed =
            self.repo.claim_for_cloning(project_id, attempt, &self.worker_id, job_id, lease_expires_at).await?;
        let Some(claimed) = claimed else {
            // Lost the claim race (another worker took it) — nothing to do.
            tracing::debug!(%project_id, attempt, "attempt not in 'queued' at claim time; another worker owns it");
            return Ok(());
        };

        self.emit_event(&claimed, "clone.started", None).await;
        metrics::counter!("agentforge_project_clone_started_total").increment(1);

        self.execute_clone(&claimed).await
    }

    /// Resolve paths + credential, run the container, and map the outcome.
    async fn execute_clone(&self, attempt: &ProjectCloneAttempt) -> AppResult<()> {
        let started = Instant::now();
        let project_id = attempt.project_id.as_uuid();

        // Resolve the projects root from the SAME source AgentWorkspaceService
        // uses (org/workspace-scoped host projects root).
        let scope =
            WorkspaceMountScope::for_workspace(attempt.organization_id.as_uuid(), attempt.workspace_id.as_uuid());
        let projects_root = match resolve_agent_workspace_paths(&self.config.workspace_root, scope, None) {
            Ok(paths) => paths.host_projects_root,
            Err(err) => return self.fail_internal(attempt, format!("workspace path resolution failed: {err}")).await,
        };

        // The on-disk directory name is the validated `workspace_dir_name`.
        let dir_name = match self.project_dir_name(project_id).await {
            Ok(name) => name,
            Err(err) => return self.fail_internal(attempt, format!("workspace_dir_name invalid: {err}")).await,
        };
        let target_dir = match dir_name.resolve_under(&projects_root) {
            Ok(path) => path,
            Err(err) => {
                return self.fail_internal(attempt, format!("workspace dir escapes projects root: {err}")).await;
            }
        };

        // Create the per-clone staging dir on the SAME filesystem as the projects
        // root, asserting the device id matches so the later rename is atomic.
        let staging_dir = projects_root.join(CLONE_STAGING_SUBDIR).join(attempt.id.to_string());
        if let Err(err) = self.prepare_staging(&projects_root, &staging_dir).await {
            return self.fail_internal(attempt, format!("staging preparation failed: {err}")).await;
        }

        // Resolve the host-matched credential and materialize the bytes ONLY now,
        // immediately before the container launch. The secret is held for exactly
        // the `run` call below and dropped (zeroized) when it returns.
        let host = self.repo_host(attempt);
        let resolved = match self.resolve_credential(attempt, host.as_deref()).await {
            Ok(resolved) => resolved,
            Err(err) => {
                let _ = self.cleanup_staging(&staging_dir).await;
                return self.fail_internal(attempt, format!("credential resolution failed: {err}")).await;
            }
        };
        // Record WHICH credential we used (never the secret), then unwrap to the
        // bytes for the launch. The `ResolvedCredential` is destructured here so
        // the `SecretBytes` lives only as long as the `run` call below.
        let credential = match resolved {
            Some(resolved) => {
                if let Err(err) = self.repo.set_credential_id(attempt.id, Some(resolved.credential_id)).await {
                    tracing::warn!(attempt_id = %attempt.id, error = %err, "failed to record credential_id on attempt");
                }
                Some(resolved.secret)
            }
            None => None,
        };

        let spec = CloneRunSpec {
            image: self.config.image.clone(),
            repo_url: attempt.repository_url.clone(),
            provider: attempt.provider.clone(),
            staging_host_path: staging_dir.clone(),
            secret_root: self.config.secret_root.clone(),
            credential, // moved in; dropped+zeroized when `run` returns
            timeout: self.config.timeout,
            max_bytes: self.config.max_bytes,
            attempt_id: attempt.id,
        };

        // Run the clone. The runtime force-removes the container + scrubs the
        // host secret file on EVERY exit path; we never touch the secret again.
        let outcome = self.runner.run(spec).await;
        let duration_ms = started.elapsed().as_millis() as i64;

        match outcome {
            Ok(CloneRunOutcome::Ready { branch, head_sha, bytes }) => {
                self.finish_ready(attempt, &staging_dir, &target_dir, branch, head_sha, bytes, duration_ms).await
            }
            Ok(CloneRunOutcome::Failed { exit_code, stderr_tail }) => {
                self.finish_failed(attempt, &staging_dir, &stderr_tail, None, duration_ms, Some(exit_code)).await
            }
            Ok(CloneRunOutcome::Timeout) => {
                let tail = RawStderr::new("clone exceeded the wall-clock timeout".to_string());
                self.finish_failed(attempt, &staging_dir, &tail, Some(CloneErrorClass::Timeout), duration_ms, None)
                    .await
            }
            Ok(CloneRunOutcome::TooLarge { stderr_tail }) => {
                self.finish_failed(
                    attempt,
                    &staging_dir,
                    &stderr_tail,
                    Some(CloneErrorClass::TooLarge),
                    duration_ms,
                    None,
                )
                .await
            }
            Err(err) => {
                // The runtime itself errored (Docker create/start, etc.). Treat as
                // an internal failure with NO raw-stderr leak (err is our own text).
                let _ = self.cleanup_staging(&staging_dir).await;
                let tail = RawStderr::new(format!("clone runtime error: {err}"));
                self.finish_failed_redacted(attempt, &tail, Some(CloneErrorClass::Internal), Some(duration_ms)).await
            }
        }
    }

    /// `Ready`: atomic same-fs rename staging/repo → target, then persist.
    #[allow(clippy::too_many_arguments)]
    async fn finish_ready(
        &self,
        attempt: &ProjectCloneAttempt,
        staging_dir: &Path,
        target_dir: &Path,
        branch: Option<String>,
        head_sha: String,
        bytes: u64,
        duration_ms: i64,
    ) -> AppResult<()> {
        let repo_src = staging_dir.join(STAGING_REPO_SUBDIR);

        // The target is a FRESH project directory; it must not already exist. If
        // it does (a soft-deleted sibling, a prior partial), FAIL LOUDLY rather
        // than clobber — we never destroy existing on-disk work.
        if tokio::fs::symlink_metadata(target_dir).await.is_ok() {
            let _ = self.cleanup_staging(staging_dir).await;
            return self
                .fail_internal(
                    attempt,
                    format!("clone target {} already exists; refusing to overwrite", target_dir.display()),
                )
                .await;
        }

        // Ensure the parent (projects root) exists; the rename is atomic only
        // because staging is on the same filesystem (asserted in `prepare_staging`).
        if let Some(parent) = target_dir.parent()
            && let Err(err) = tokio::fs::create_dir_all(parent).await
        {
            let _ = self.cleanup_staging(staging_dir).await;
            return self.fail_internal(attempt, format!("failed to ensure projects root: {err}")).await;
        }

        if let Err(err) = tokio::fs::rename(&repo_src, target_dir).await {
            // Rename failed (cross-device, missing source, race). NO partial
            // target is left (rename is all-or-nothing); clean staging and fail.
            let _ = self.cleanup_staging(staging_dir).await;
            return self
                .fail_internal(
                    attempt,
                    format!("atomic rename {} -> {} failed: {err}", repo_src.display(), target_dir.display()),
                )
                .await;
        }

        // The repo is now live at the target; remove the (now-empty) staging dir.
        let _ = self.cleanup_staging(staging_dir).await;

        let success = CloneSuccess {
            resolved_branch: branch.clone(),
            head_sha: head_sha.clone(),
            bytes_cloned: i64::try_from(bytes).unwrap_or(i64::MAX),
            duration_ms,
        };
        self.repo.finalize_ready(attempt.id, &success).await?;

        self.emit_event(
            attempt,
            "clone.ready",
            Some(serde_json::json!({
                "branch": branch,
                "head_sha": head_sha,
                "bytes": bytes,
                "duration_ms": duration_ms,
            })),
        )
        .await;
        metrics::counter!("agentforge_project_clone_completed_total", "status" => "ready").increment(1);
        metrics::histogram!("agentforge_project_clone_duration_seconds").record(duration_ms as f64 / 1000.0);
        metrics::histogram!("agentforge_project_clone_bytes").record(bytes as f64);
        if let Some(provider) = provider_label(attempt.provider.as_deref()) {
            metrics::counter!("agentforge_project_clone_by_provider_total", "provider" => provider).increment(1);
        }
        tracing::info!(
            project_id = %attempt.project_id,
            attempt = attempt.attempt,
            duration_ms,
            "project clone ready"
        );
        Ok(())
    }

    /// Failure paths: redact RAW stderr, classify, persist `failed`, clean
    /// staging, emit, then schedule a bounded retry.
    async fn finish_failed(
        &self,
        attempt: &ProjectCloneAttempt,
        staging_dir: &Path,
        raw: &RawStderr,
        forced_class: Option<CloneErrorClass>,
        duration_ms: i64,
        exit_code: Option<i64>,
    ) -> AppResult<()> {
        // Never leave a partial clone behind.
        let _ = self.cleanup_staging(staging_dir).await;
        // The exit code informs server-side logs only; it is NEVER persisted raw
        // (it is harmless on its own, but we keep the persistence path
        // redaction-only). The raw stderr is redacted in `finish_failed_redacted`.
        if let Some(code) = exit_code {
            tracing::debug!(project_id = %attempt.project_id, attempt = attempt.attempt, exit_code = code, "clone container exited non-zero");
        }
        self.finish_failed_redacted(attempt, raw, forced_class, Some(duration_ms)).await
    }

    /// The redaction-before-persist core: this is the ONLY place raw `RawStderr`
    /// meets persistence. The raw text is run through the M1 `redact` (producing a
    /// `RedactedError` proof type) and ONLY the redacted string is stored — a raw
    /// token can never reach `error_message`.
    async fn finish_failed_redacted(
        &self,
        attempt: &ProjectCloneAttempt,
        raw: &RawStderr,
        forced_class: Option<CloneErrorClass>,
        duration_ms: Option<i64>,
    ) -> AppResult<()> {
        // Redaction boundary: raw stderr -> RedactedError -> stored string.
        let redacted = redact(raw.as_raw());
        let error_class = forced_class.unwrap_or_else(|| CloneErrorClass::classify(raw.as_raw()));
        let error_message = redacted.into_string();

        self.repo
            .finalize_failed(
                attempt.id,
                &CloneFailure {
                    error_class: error_class.as_str().to_string(),
                    error_message: error_message.clone(),
                    duration_ms,
                },
            )
            .await?;

        self.emit_event(
            attempt,
            "clone.failed",
            Some(serde_json::json!({ "error_class": error_class.as_str(), "error_message": error_message })),
        )
        .await;
        metrics::counter!("agentforge_project_clone_completed_total", "status" => "failed").increment(1);
        metrics::counter!(
            "agentforge_project_clone_failures_total",
            "class" => error_class.as_str().to_string()
        )
        .increment(1);
        tracing::warn!(
            project_id = %attempt.project_id,
            attempt = attempt.attempt,
            error_class = %error_class,
            "project clone failed (error_message redacted before persistence)"
        );

        self.maybe_schedule_retry(attempt).await;
        Ok(())
    }

    /// Fail with an INTERNAL error from an explicit (already-safe) message. The
    /// message originates from our own code (never git stderr), so it carries no
    /// token; it still passes through `redact` for uniformity + truncation.
    async fn fail_internal(&self, attempt: &ProjectCloneAttempt, message: String) -> AppResult<()> {
        let raw = RawStderr::new(message);
        self.finish_failed_redacted(attempt, &raw, Some(CloneErrorClass::Internal), None).await
    }

    /// Bounded retry: if `attempt < max_attempts`, insert a new `queued` attempt
    /// (attempt+1) + outbox row; otherwise leave the failure terminal.
    async fn maybe_schedule_retry(&self, attempt: &ProjectCloneAttempt) {
        if attempt.attempt >= self.config.max_attempts {
            tracing::info!(
                project_id = %attempt.project_id,
                attempt = attempt.attempt,
                max = self.config.max_attempts,
                "clone failed at the retry ceiling; leaving it terminal"
            );
            return;
        }
        let next = attempt.attempt + 1;
        match self
            .repo
            .schedule_retry(
                attempt.organization_id.as_uuid(),
                attempt.workspace_id.as_uuid(),
                attempt.project_id.as_uuid(),
                next,
                &attempt.repository_url,
                attempt.provider.as_deref(),
            )
            .await
        {
            Ok(Some(scheduled)) => {
                metrics::counter!("agentforge_project_clone_retries_total").increment(1);
                self.emit_event(attempt, "clone.retry", Some(serde_json::json!({ "next_attempt": scheduled }))).await;
                tracing::info!(
                    project_id = %attempt.project_id,
                    next_attempt = scheduled,
                    backoff_secs = self.config.retry_backoff.as_secs(),
                    "scheduled bounded clone retry"
                );
            }
            Ok(None) => {
                tracing::debug!(project_id = %attempt.project_id, next, "retry attempt already exists; not duplicating");
            }
            Err(err) => {
                tracing::warn!(project_id = %attempt.project_id, error = %err, "failed to schedule clone retry");
            }
        }
    }

    // -- reconciler ----------------------------------------------------------

    /// One reconciler pass: recover expired-lease `cloning` attempts and
    /// re-enqueue lost `queued` attempts. The polling fallback (spec §6.3, §8).
    pub async fn run_reconciler_once(&self) {
        // (a) crashed-worker `cloning` attempts past their lease.
        match self.repo.find_expired_cloning(Utc::now(), 50).await {
            Ok(candidates) => {
                for candidate in candidates {
                    self.recover_expired_cloning(&candidate).await;
                }
            }
            Err(err) => tracing::warn!(error = %err, "reconciler: find_expired_cloning failed"),
        }

        // (b) `queued` attempts whose enqueue was lost. Only consider rows older
        // than a grace window so a just-created attempt (whose publisher simply
        // has not run yet) is not double-enqueued.
        let grace = Utc::now() - chrono::Duration::from_std(self.config.reconcile_interval).unwrap_or_default();
        match self.repo.find_orphaned_queued(grace, 50).await {
            Ok(candidates) => {
                for candidate in candidates {
                    match self
                        .repo
                        .reenqueue_outbox(candidate.organization_id, candidate.project_id, candidate.attempt)
                        .await
                    {
                        Ok(true) => {
                            metrics::counter!("agentforge_project_clone_reconcile_reenqueued_total").increment(1);
                            tracing::info!(
                                project_id = %candidate.project_id,
                                attempt = candidate.attempt,
                                "reconciler re-enqueued a lost queued clone attempt"
                            );
                        }
                        Ok(false) => {}
                        Err(err) => tracing::warn!(error = %err, "reconciler: reenqueue_outbox failed"),
                    }
                }
            }
            Err(err) => tracing::warn!(error = %err, "reconciler: find_orphaned_queued failed"),
        }

        // Update the in-flight gauge for observability (spec §12).
        if let Ok((queued, cloning)) = self.repo.count_in_flight().await {
            metrics::gauge!("agentforge_project_clone_in_flight", "state" => "queued").set(queued as f64);
            metrics::gauge!("agentforge_project_clone_in_flight", "state" => "cloning").set(cloning as f64);
        }
    }

    /// Recover one crashed-worker `cloning` attempt: reap its container, fail the
    /// attempt closed, then retry if attempts remain.
    async fn recover_expired_cloning(&self, candidate: &ReconcileCandidate) {
        tracing::warn!(
            project_id = %candidate.project_id,
            attempt = candidate.attempt,
            "reconciler: recovering a clone attempt whose lease expired (worker likely crashed)"
        );
        // Best-effort: a targeted sweep removes the labelled container if it is a
        // crashed-worker leftover.
        if let Err(err) = self.runner.sweep_orphans().await {
            tracing::warn!(error = %err, "reconciler: sweep_orphans during recovery failed");
        }
        // Best-effort staging cleanup for this attempt (path is derivable).
        // We cannot know the projects-root here without re-resolving; do it.
        self.cleanup_orphan_staging(candidate).await;

        let failure = CloneFailure {
            error_class: CloneErrorClass::Internal.as_str().to_string(),
            error_message: "worker lost the clone".to_string(),
            duration_ms: None,
        };
        if let Err(err) = self.repo.finalize_failed(candidate.id, &failure).await {
            tracing::warn!(error = %err, "reconciler: finalize_failed during recovery failed");
            return;
        }
        metrics::counter!("agentforge_project_clone_reconcile_recovered_total").increment(1);

        // Retry the recovered attempt if there is budget.
        if candidate.attempt < self.config.max_attempts {
            let next = candidate.attempt + 1;
            match self
                .repo
                .schedule_retry(
                    candidate.organization_id,
                    candidate.workspace_id,
                    candidate.project_id,
                    next,
                    &candidate.repository_url,
                    candidate.provider.as_deref(),
                )
                .await
            {
                Ok(Some(_)) => metrics::counter!("agentforge_project_clone_retries_total").increment(1),
                Ok(None) => {}
                Err(err) => tracing::warn!(error = %err, "reconciler: retry scheduling failed"),
            }
        }
    }

    /// Reap crashed-worker clone containers (startup + each reconcile pass).
    pub async fn sweep_orphans_once(&self) {
        match self.runner.sweep_orphans().await {
            Ok(0) => {}
            Ok(n) => tracing::info!(reaped = n, "project_clone sweep reaped orphan clone containers"),
            Err(err) => tracing::warn!(error = %err, "project_clone orphan sweep failed"),
        }
    }

    // -- helpers -------------------------------------------------------------

    /// Resolve + validate the project's on-disk directory name.
    async fn project_dir_name(&self, project_id: Uuid) -> AppResult<WorkspaceDirName> {
        let raw: String =
            sqlx::query_scalar(r#"SELECT workspace_dir_name FROM projects WHERE id = $1 AND deleted_at IS NULL"#)
                .bind(project_id)
                .fetch_one(&self.pool)
                .await?;
        WorkspaceDirName::parse(&raw).map_err(|e| agentforge_core::ErrorKind::Validation(e.to_string()).into())
    }

    /// Create the per-clone staging dir and ASSERT it is on the same filesystem
    /// as the projects root (so the later rename is atomic). On Unix this checks
    /// the device id; on other platforms it creates the dir and trusts the layout.
    async fn prepare_staging(&self, projects_root: &Path, staging_dir: &Path) -> AppResult<()> {
        // Remove any leftover from a previous attempt with this id (idempotent).
        let _ = tokio::fs::remove_dir_all(staging_dir).await;
        let staging_parent = staging_dir.parent().unwrap_or(staging_dir);
        tokio::fs::create_dir_all(staging_parent)
            .await
            .map_err(|err| internal(format!("create staging parent {}: {err}", staging_parent.display())))?;
        tokio::fs::create_dir_all(staging_dir)
            .await
            .map_err(|err| internal(format!("create staging dir {}: {err}", staging_dir.display())))?;

        // The projects root must exist before we can stat its device.
        tokio::fs::create_dir_all(projects_root)
            .await
            .map_err(|err| internal(format!("ensure projects root {}: {err}", projects_root.display())))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let root_dev = tokio::fs::metadata(projects_root)
                .await
                .map_err(|err| internal(format!("stat projects root: {err}")))?
                .dev();
            let staging_dev = tokio::fs::metadata(staging_dir)
                .await
                .map_err(|err| internal(format!("stat staging dir: {err}")))?
                .dev();
            if root_dev != staging_dev {
                return Err(internal(format!(
                    "staging dir {} is not on the same filesystem as the projects root {} (rename would not be atomic)",
                    staging_dir.display(),
                    projects_root.display()
                )));
            }
        }
        Ok(())
    }

    /// Remove a per-clone staging dir (best-effort; never fails the clone).
    async fn cleanup_staging(&self, staging_dir: &Path) -> AppResult<()> {
        if let Err(err) = tokio::fs::remove_dir_all(staging_dir).await
            && err.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(path = %staging_dir.display(), error = %err, "failed to remove clone staging dir");
        }
        Ok(())
    }

    /// Best-effort staging cleanup for a reconciler candidate (re-resolves the
    /// projects root from its tenant snapshot).
    async fn cleanup_orphan_staging(&self, candidate: &ReconcileCandidate) {
        let scope = WorkspaceMountScope::for_workspace(candidate.organization_id, candidate.workspace_id);
        if let Ok(paths) = resolve_agent_workspace_paths(&self.config.workspace_root, scope, None) {
            let staging = paths.host_projects_root.join(CLONE_STAGING_SUBDIR).join(candidate.id.to_string());
            let _ = self.cleanup_staging(&staging).await;
        }
    }

    /// Host of the repo URL (for credential host-matching), reusing the M1 parse
    /// host extraction shape. `None` when the URL has no derivable host.
    fn repo_host(&self, attempt: &ProjectCloneAttempt) -> Option<String> {
        repo_url_host(&attempt.repository_url)
    }

    /// Resolve the host-matched credential. The decrypted bytes never leave this
    /// call's return value (a `ResolvedCredential` holding `SecretBytes`).
    async fn resolve_credential(
        &self,
        attempt: &ProjectCloneAttempt,
        host: Option<&str>,
    ) -> AppResult<Option<crate::services::git_credential::ResolvedCredential>> {
        let Some(host) = host else {
            return Ok(None);
        };
        // Build an org-scoped tenant scope for the resolution (the worker acts on
        // behalf of the project's org; resolution is org-constrained in SQL).
        let scope = agentforge_core::TenantScope::new(
            attempt.organization_id,
            // The user axis is unused by `resolve_for_host` (it is org-scoped),
            // but TenantScope requires one; use a nil placeholder.
            agentforge_core::UserId::from(Uuid::nil()),
        );
        self.credentials.resolve_for_host(&scope, host).await
    }

    /// Record a clone lifecycle event: an audit-log row (spec §12) plus, when
    /// realtime is wired, a project-scoped WS broadcast.
    async fn emit_event(&self, attempt: &ProjectCloneAttempt, action: &str, extra: Option<serde_json::Value>) {
        let project_id = attempt.project_id.as_uuid();
        let mut details = serde_json::json!({
            "project_id": project_id,
            "attempt": attempt.attempt,
            "clone_status": clone_status_for_action(action),
        });
        if let (Some(obj), Some(extra)) = (details.as_object_mut(), extra.clone())
            && let Some(extra_obj) = extra.as_object()
        {
            for (k, v) in extra_obj {
                obj.insert(k.clone(), v.clone());
            }
        }

        // Audit log (DB-backed, always written when possible).
        if let Err(err) = self
            .audit
            .log_action(attempt.organization_id, None, action, CLONE_AUDIT_RESOURCE, Some(project_id), &details, None)
            .await
        {
            tracing::warn!(error = %err, %action, "failed to write clone audit event");
        }

        // WS broadcast (project-scoped subject) when realtime is configured.
        self.broadcast_status(attempt.organization_id, project_id, action, &details).await;
    }

    /// Broadcast a project-scoped status frame over the realtime channel.
    async fn broadcast_status(&self, org_id: OrgId, project_id: Uuid, action: &str, details: &serde_json::Value) {
        let Some(client) = self.realtime.as_ref() else {
            return;
        };
        let frame = serde_json::json!({
            "type": "project_clone:status_update",
            "payload": {
                "action": action,
                "eventId": Uuid::now_v7(),
                "projectId": project_id,
                "cloneStatus": clone_status_for_action(action),
                "details": details,
            }
        });
        let payload = match serde_json::to_vec(&frame) {
            Ok(payload) => payload,
            Err(err) => {
                tracing::warn!(error = %err, "failed to serialize clone status broadcast");
                return;
            }
        };
        // Project-scoped subject (mirrors gateway::subscription_subjects).
        let subject = format!("broadcast.{}.scope.project.{}", org_id.as_uuid(), project_id);
        if let Err(err) = client.publish(subject.clone(), payload.into()).await {
            tracing::warn!(error = %err, %subject, "failed to publish clone status broadcast");
        }
    }
}

/// Derive the bare host of an https repo URL for credential matching. Mirrors the
/// M1 `ProjectRepositoryUrl` host extraction (scheme strip, authority up to the
/// first `/`/`?`/`#`, strip `:port`). The URL is already validated (no userinfo).
pub(crate) fn repo_url_host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() {
        return None;
    }
    let host = if authority.starts_with('[') {
        match authority.find(']') {
            Some(end) => &authority[..=end],
            None => authority,
        }
    } else {
        authority.split(':').next().unwrap_or(authority)
    };
    if host.is_empty() { None } else { Some(host.to_ascii_lowercase()) }
}

/// Map an action to the project's denormalized clone_status for the event frame.
fn clone_status_for_action(action: &str) -> &'static str {
    match action {
        "clone.started" => "cloning",
        "clone.ready" => "ready",
        "clone.failed" => "failed",
        "clone.retry" => "queued",
        _ => "queued",
    }
}

/// Provider metric label (only for known SaaS providers; `None` otherwise).
fn provider_label(provider: Option<&str>) -> Option<String> {
    provider.and_then(|p| match p {
        "github" => Some("github".to_string()),
        "gitlab" => Some("gitlab".to_string()),
        _ => None,
    })
}

fn internal(message: String) -> agentforge_core::AppError {
    agentforge_core::ErrorKind::Internal(anyhow::anyhow!(message)).into()
}

/// Register the worker's metric descriptions so dashboards have series present
/// from the first scrape (mirrors the M2 outbox metric naming).
pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_project_clone_started_total",
        "project_clone attempts claimed and transitioned to cloning by the worker"
    );
    metrics::describe_counter!(
        "agentforge_project_clone_completed_total",
        "project_clone attempts finalized, labeled by terminal status (ready|failed)"
    );
    metrics::describe_counter!(
        "agentforge_project_clone_failures_total",
        "project_clone failures, labeled by classified error class (auth|not_found|network|timeout|too_large|internal)"
    );
    metrics::describe_counter!(
        "agentforge_project_clone_retries_total",
        "bounded project_clone retries scheduled (a new attempt row + outbox)"
    );
    metrics::describe_counter!(
        "agentforge_project_clone_by_provider_total",
        "successful project_clones labeled by resolved provider (github|gitlab)"
    );
    metrics::describe_histogram!("agentforge_project_clone_duration_seconds", "wall-clock duration of a clone attempt");
    metrics::describe_histogram!("agentforge_project_clone_bytes", "cloned tree size in bytes for a ready clone");
    metrics::describe_counter!(
        "agentforge_project_clone_worker_errors_total",
        "project_clone worker tick errors (dequeue/decode failures recovered by the reconciler)"
    );
    metrics::describe_counter!(
        "agentforge_project_clone_reconcile_recovered_total",
        "crashed-worker cloning attempts the reconciler recovered (lease expired)"
    );
    metrics::describe_counter!(
        "agentforge_project_clone_reconcile_reenqueued_total",
        "queued clone attempts the reconciler re-enqueued after a lost enqueue"
    );
    metrics::describe_gauge!(
        "agentforge_project_clone_in_flight",
        "in-flight project_clone attempts, labeled by state (queued|cloning)"
    );

    // Touch each so the series exists pre-traffic.
    metrics::counter!("agentforge_project_clone_started_total").increment(0);
    metrics::counter!("agentforge_project_clone_worker_errors_total").increment(0);
    metrics::counter!("agentforge_project_clone_reconcile_recovered_total").increment(0);
    metrics::counter!("agentforge_project_clone_reconcile_reenqueued_total").increment(0);
    metrics::gauge!("agentforge_project_clone_in_flight", "state" => "queued").set(0.0);
    metrics::gauge!("agentforge_project_clone_in_flight", "state" => "cloning").set(0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_url_host_extracts_bare_host() {
        assert_eq!(repo_url_host("https://github.com/o/r.git").as_deref(), Some("github.com"));
        assert_eq!(repo_url_host("https://gitlab.example.com:8443/o/r").as_deref(), Some("gitlab.example.com"));
        assert_eq!(repo_url_host("https://GitHub.com/o/r").as_deref(), Some("github.com"));
        // Non-https / no host -> None.
        assert_eq!(repo_url_host("git@github.com:o/r"), None);
        assert_eq!(repo_url_host("https:///path"), None);
    }

    #[test]
    fn clone_status_for_action_maps_each_event() {
        assert_eq!(clone_status_for_action("clone.started"), "cloning");
        assert_eq!(clone_status_for_action("clone.ready"), "ready");
        assert_eq!(clone_status_for_action("clone.failed"), "failed");
        assert_eq!(clone_status_for_action("clone.retry"), "queued");
    }

    #[test]
    fn provider_label_only_known_saas() {
        assert_eq!(provider_label(Some("github")).as_deref(), Some("github"));
        assert_eq!(provider_label(Some("gitlab")).as_deref(), Some("gitlab"));
        assert_eq!(provider_label(Some("bitbucket")), None);
        assert_eq!(provider_label(None), None);
    }
}
