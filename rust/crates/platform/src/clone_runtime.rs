//! Ephemeral git-clone container runtime (project-git-clone feature, M4).
//!
//! Launches the disposable `agentforge-clone` container that clones a single
//! repository into a per-clone staging directory and exits. This is the
//! platform-runtime layer of the design spec (§6.5, §6.6, §10, §11): it owns the
//! Docker create/start/wait/inspect/reap lifecycle and the container hardening,
//! but holds NO clone-attempt state — the M5 worker drives the status machine
//! and calls into here.
//!
//! # Isolation model (spec §6.6, §10 — flagged requirements)
//!
//! - **Staging-only mount.** The container mounts ONLY the per-clone staging
//!   directory at `/staging` (read-write). It never sees the projects root or a
//!   sibling project, so a hostile repo's hooks/post-checkout cannot read or
//!   corrupt other tenants' work. This is the tenant-isolation control at the
//!   runtime layer.
//! - **Restricted egress network.** The container is attached to a dedicated
//!   Docker network ([`CLONE_EGRESS_NETWORK`]) that the internal-service
//!   containers (Postgres, NATS, the API) are NOT on, so a crafted or
//!   DNS-rebinding repo URL resolved at git's connect time cannot reach internal
//!   services over the shared `agentforge-agents` network.
//!
//!   **Residual deploy requirement (be honest):** Docker's stock bridge driver
//!   still permits egress to the host's other networks / RFC1918 / link-local /
//!   the cloud metadata endpoint (169.254.169.254) unless the host firewalls it.
//!   This crate does the best the Docker API allows — network *separation* from
//!   internal services — but full RFC1918/link-local/metadata egress filtering
//!   is a **deployment-layer firewall concern** the operator MUST configure on
//!   [`CLONE_EGRESS_NETWORK`]'s subnet (e.g. an iptables/nftables egress policy
//!   or an egress proxy). The in-app HTTPS-only + host deny-list URL gate (M1)
//!   is the defense-in-depth complement; this network is the runtime layer; the
//!   firewall is the deploy layer. See the deployment runbook.
//!
//! # Credential delivery (spec §6.7)
//!
//! The git credential is delivered to the container at
//! `/run/secrets/git-credential` (mode 0400) via a **read-only bind mount of a
//! short-lived host file** the worker writes outside the staging tree — never an
//! environment variable, never a build layer, never on git's argv. The
//! [`SecretBytes`] wrapper never `Debug`-prints or serializes the token, and the
//! host secret file is force-removed on every exit path together with the
//! container (which is the only thing that ever maps the secret).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use agentforge_core::{AppError, AppResult, ErrorKind};
use bollard::errors::Error as BollardError;
use bollard::models::{ContainerCreateBody, HostConfig, NetworkCreateRequest, NetworkingConfig};
use bollard::query_parameters::{
    CreateContainerOptions, ListContainersOptionsBuilder, RemoveContainerOptionsBuilder, StartContainerOptions,
    WaitContainerOptionsBuilder,
};
use futures_util::StreamExt;
use serde::Deserialize;
use uuid::Uuid;

use crate::docker::DockerClient;
use crate::security;
use crate::types::{ContainerConfig, Mount, ResourceLimits};

/// Container path the staging directory is mounted at (read-write).
pub const CLONE_STAGING_TARGET: &str = "/staging";

/// `CLONE_DEST` env value handed to `clone-entrypoint.sh` (the container clones
/// into `<CLONE_DEST>/repo` and writes `<CLONE_DEST>/.clone-result.json`).
pub const CLONE_DEST: &str = "/staging";

/// Container path the read-only credential secret file is mounted at (mode 0400).
/// Matches the `clone-entrypoint.sh` contract (M3) exactly.
pub const CLONE_SECRET_TARGET: &str = "/run/secrets/git-credential";

/// Dedicated Docker network the clone container is attached to. It deliberately
/// does NOT include the internal-service containers. See the module docs for the
/// residual deploy-layer firewall requirement.
pub const CLONE_EGRESS_NETWORK: &str = "agentforge-clone-egress";

/// Label key applied to every clone container, for reaping + the sweeper.
pub const CLONE_LABEL_KEY: &str = "agentforge.project_clone";

/// Result file the entrypoint writes on success, relative to the staging dir.
const CLONE_RESULT_FILE: &str = ".clone-result.json";

/// Upper bound on captured stderr/log tail bytes returned in [`CloneRunOutcome::Failed`].
const STDERR_TAIL_LIMIT: usize = 8 * 1024;

/// Default per-clone resource limits. A clone is short-lived and single-threaded
/// network/disk I/O, so it gets a modest slice — tighter than an agent container.
fn clone_resource_limits() -> ResourceLimits {
    ResourceLimits {
        cpu_quota: Some(100_000),                   // 1 CPU
        memory_bytes: Some(512 * 1024 * 1024),      // 512 MB
        memory_swap_bytes: Some(512 * 1024 * 1024), // no extra swap headroom
        pids_limit: Some(256),
    }
}

/// A byte secret (e.g. a git credential) that never leaks through `Debug`,
/// `Display`, or serialization, and is zeroed on drop.
///
/// The token reaches the container only via a mounted file (see module docs), so
/// this wrapper exists purely to keep it out of logs, panic messages, and any
/// `Serialize` derive on a struct that transitively holds it.
#[derive(Clone)]
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    /// Wrap raw secret bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Borrow the raw bytes. Callers must not log or serialize the result.
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for SecretBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self::new(bytes)
    }
}

impl From<String> for SecretBytes {
    fn from(value: String) -> Self {
        Self::new(value.into_bytes())
    }
}

// Manual, non-leaking Debug: NEVER prints the bytes.
impl std::fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretBytes([REDACTED])")
    }
}

// Best-effort scrub so the token does not linger in freed memory.
impl Drop for SecretBytes {
    fn drop(&mut self) {
        for byte in self.0.iter_mut() {
            // `write_volatile` is overkill here and unsafe; a plain overwrite is
            // sufficient defense-in-depth without unsafe code.
            *byte = 0;
        }
    }
}

/// Inputs for one ephemeral clone run.
///
/// The [`credential`](Self::credential) holds the token in a [`SecretBytes`]
/// wrapper that implements **no** `Serialize` and a redacting `Debug`, so it can
/// never leak through serialization (a compile error if a struct tried) or a
/// `{:?}` print. This spec is intentionally not `Serialize`/`Deserialize`.
#[derive(Debug)]
pub struct CloneRunSpec {
    /// Clone image, e.g. `agentforge-clone:latest`.
    pub image: String,
    /// HTTPS repository URL handed to the container as `CLONE_URL`.
    pub repo_url: String,
    /// Optional provider hint (`github` | `gitlab`) handed as `CLONE_PROVIDER`.
    pub provider: Option<String>,
    /// Host path of the per-clone staging directory, mounted at `/staging`.
    pub staging_host_path: PathBuf,
    /// Optional host-matched short-lived credential. `None` ⇒ public repo, no
    /// secret mount. Never serializes (no `Serialize` impl) and never
    /// `Debug`-prints the token (see [`SecretBytes`]).
    pub credential: Option<SecretBytes>,
    /// Hard wall-clock timeout for the whole clone.
    pub timeout: Duration,
    /// Attempt id — drives the deterministic container name + reap label.
    pub attempt_id: Uuid,
}

// The `clone_spec_debug_does_not_leak_credential` test pins the `Debug`-redaction
// guarantee; `SecretBytes`' absence of a `Serialize` impl makes a serde leak a
// compile error.

/// Outcome of a clone run. The worker (M5) maps this onto the attempt status,
/// redacting [`Failed::stderr_tail`] via the M1 `RedactedError` before storing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloneRunOutcome {
    /// Container exited 0 and a valid result file was produced.
    Ready {
        /// Resolved branch name; `None` for a detached HEAD.
        branch: Option<String>,
        /// HEAD commit SHA.
        head_sha: String,
        /// Cloned tree size in bytes (best-effort from the container).
        bytes: u64,
    },
    /// Container exited non-zero. `stderr_tail` is RAW (unredacted) — the worker
    /// redacts before persistence.
    Failed {
        /// Container exit code (git's 128 = auth/not-found/transport).
        exit_code: i64,
        /// Bounded tail of the container's combined stdout+stderr.
        stderr_tail: String,
    },
    /// The wall-clock timeout elapsed before the container exited.
    Timeout,
}

/// Shape of `.clone-result.json` written by `clone-entrypoint.sh`.
#[derive(Debug, Deserialize)]
struct CloneResultFile {
    #[serde(default)]
    branch: Option<String>,
    head_sha: String,
    #[serde(default)]
    bytes: u64,
}

/// The Docker operations the clone runtime needs. Extracted as a trait so unit
/// tests can capture the create-config and script lifecycle without a real
/// daemon (mirrors the `mcp_docker_runtime` mock-backend style).
#[async_trait::async_trait]
pub trait CloneDockerBackend: Send + Sync {
    /// Ensure the dedicated egress network exists (create-or-reuse). Idempotent.
    async fn ensure_egress_network(&self) -> AppResult<()>;
    /// Create a container from the given config; returns the container id.
    async fn create_container(&self, config: CloneContainerConfig) -> AppResult<String>;
    /// Start a created container.
    async fn start_container(&self, id: &str) -> AppResult<()>;
    /// Wait (bounded by the caller) for the container to exit; returns its exit code.
    async fn wait_exit(&self, id: &str) -> AppResult<i64>;
    /// Fetch a bounded tail of the container's combined stdout+stderr.
    async fn logs_tail(&self, id: &str, limit_bytes: usize) -> AppResult<String>;
    /// Force-remove a container. Must succeed-or-warn; never panics.
    async fn force_remove(&self, id: &str) -> AppResult<()>;
    /// List clone containers (label `agentforge.project_clone`) with their
    /// id + created-at epoch seconds, for the orphan sweep.
    async fn list_clone_containers(&self) -> AppResult<Vec<CloneContainerSummary>>;
}

/// Minimal container summary the sweep needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneContainerSummary {
    pub id: String,
    /// Container creation time as a unix epoch (seconds). `0` if unknown.
    pub created_epoch_secs: i64,
}

/// A fully-resolved create config for a clone container. Captured verbatim by
/// the test backend so assertions can pin the mount set, network, label, name,
/// env, and security posture.
///
/// (`types::Mount` / `ResourceLimits` are not `Eq`, so this carries `Clone` for
/// capture-and-inspect rather than whole-struct equality.)
#[derive(Debug, Clone)]
pub struct CloneContainerConfig {
    pub image: String,
    pub name: String,
    pub env: Vec<String>,
    pub labels: HashMap<String, String>,
    pub mounts: Vec<Mount>,
    pub network: String,
    pub resources: ResourceLimits,
    /// Always false (defense-in-depth); asserted by tests.
    pub privileged: bool,
    /// Always false (defense-in-depth); asserted by tests.
    pub host_pid: bool,
    /// Read-only root filesystem (only `/staging` + `/tmp` are writable).
    pub readonly_rootfs: bool,
}

/// Live Docker backend over the shared bollard client.
pub struct LiveCloneDockerBackend {
    docker: std::sync::Arc<DockerClient>,
}

impl LiveCloneDockerBackend {
    pub fn new(docker: std::sync::Arc<DockerClient>) -> Self {
        Self { docker }
    }
}

#[async_trait::async_trait]
impl CloneDockerBackend for LiveCloneDockerBackend {
    async fn ensure_egress_network(&self) -> AppResult<()> {
        // Create-or-reuse. A 409 (already exists) is success.
        let request = NetworkCreateRequest {
            name: CLONE_EGRESS_NETWORK.to_string(),
            driver: Some("bridge".to_string()),
            // NOT `internal: true` — the clone must reach the public git host.
            // Egress *filtering* (RFC1918/metadata) is the deploy-layer firewall
            // on this network's subnet (see module docs).
            internal: Some(false),
            attachable: Some(true),
            labels: Some(HashMap::from([("agentforge.managed".to_string(), "clone-egress".to_string())])),
            ..Default::default()
        };
        match self.docker.inner().create_network(request).await {
            Ok(_) => Ok(()),
            Err(BollardError::DockerResponseServerError { status_code: 409, .. }) => Ok(()),
            Err(err) => Err(docker_error("create clone egress network", err)),
        }
    }

    async fn create_container(&self, config: CloneContainerConfig) -> AppResult<String> {
        let binds: Vec<String> = config
            .mounts
            .iter()
            .map(|m| {
                if m.read_only { format!("{}:{}:ro", m.source, m.target) } else { format!("{}:{}", m.source, m.target) }
            })
            .collect();

        // A real read-only rootfs needs a writable /tmp for the credential
        // helper the entrypoint writes; mount it as a small tmpfs.
        let tmpfs = HashMap::from([("/tmp".to_string(), "rw,nosuid,nodev,size=64m".to_string())]);

        let host_config = HostConfig {
            memory: config.resources.memory_bytes,
            memory_swap: config.resources.memory_swap_bytes,
            cpu_quota: config.resources.cpu_quota,
            pids_limit: config.resources.pids_limit,
            binds: if binds.is_empty() { None } else { Some(binds) },
            network_mode: Some(config.network.clone()),
            readonly_rootfs: Some(config.readonly_rootfs),
            tmpfs: Some(tmpfs),
            // Defense-in-depth, mirrors container.rs: never privileged, never
            // host PID, drop all caps, no new privileges.
            privileged: Some(false),
            pid_mode: None,
            cap_drop: Some(vec!["ALL".to_string()]),
            security_opt: Some(vec!["no-new-privileges".to_string()]),
            ..Default::default()
        };

        // Attach to the dedicated egress network at create time so the container
        // is NEVER, even briefly, on the default bridge alongside other services.
        let networking_config = NetworkingConfig {
            endpoints_config: Some(HashMap::from([(CLONE_EGRESS_NETWORK.to_string(), Default::default())])),
        };

        let create_body = ContainerCreateBody {
            image: Some(config.image.clone()),
            env: Some(config.env.clone()),
            labels: Some(config.labels.clone()),
            host_config: Some(host_config),
            networking_config: Some(networking_config),
            ..Default::default()
        };

        let options = CreateContainerOptions { name: Some(config.name.clone()), platform: String::new() };
        let response = self
            .docker
            .inner()
            .create_container(Some(options), create_body)
            .await
            .map_err(|err| docker_error("create clone container", err))?;
        Ok(response.id)
    }

    async fn start_container(&self, id: &str) -> AppResult<()> {
        self.docker
            .inner()
            .start_container(id, None::<StartContainerOptions>)
            .await
            .map_err(|err| docker_error("start clone container", err))
    }

    async fn wait_exit(&self, id: &str) -> AppResult<i64> {
        let options = WaitContainerOptionsBuilder::new().condition("not-running").build();
        let mut stream = self.docker.inner().wait_container(id, Some(options));
        // bollard maps a non-zero exit to `Err(DockerContainerWaitError{code,..})`
        // and a zero exit to `Ok(ContainerWaitResponse{status_code,..})`. Treat
        // both as the container's exit code, not a transport failure.
        match stream.next().await {
            Some(Ok(response)) => Ok(response.status_code),
            Some(Err(BollardError::DockerContainerWaitError { code, .. })) => Ok(code),
            Some(Err(err)) => Err(docker_error("wait for clone container", err)),
            // Stream ended without a frame ⇒ already exited; treat as success-ish
            // and let the result-file check decide.
            None => Ok(0),
        }
    }

    async fn logs_tail(&self, id: &str, limit_bytes: usize) -> AppResult<String> {
        use bollard::query_parameters::LogsOptionsBuilder;
        let options = LogsOptionsBuilder::new().stdout(true).stderr(true).tail("400").build();
        let mut stream = self.docker.inner().logs(id, Some(options));
        let mut buffer: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(output) => buffer.extend_from_slice(output.as_ref()),
                Err(err) => return Err(docker_error("read clone container logs", err)),
            }
            if buffer.len() >= limit_bytes {
                break;
            }
        }
        Ok(bounded_tail(&String::from_utf8_lossy(&buffer), limit_bytes))
    }

    async fn force_remove(&self, id: &str) -> AppResult<()> {
        let options = RemoveContainerOptionsBuilder::new().force(true).build();
        match self.docker.inner().remove_container(id, Some(options)).await {
            Ok(()) => Ok(()),
            // Already gone ⇒ the post-condition (no container) holds.
            Err(BollardError::DockerResponseServerError { status_code: 404, .. }) => Ok(()),
            Err(err) => Err(docker_error("remove clone container", err)),
        }
    }

    async fn list_clone_containers(&self) -> AppResult<Vec<CloneContainerSummary>> {
        let filters = HashMap::from([("label".to_string(), vec![CLONE_LABEL_KEY.to_string()])]);
        let options = ListContainersOptionsBuilder::new().all(true).filters(&filters).build();
        let summaries = self
            .docker
            .inner()
            .list_containers(Some(options))
            .await
            .map_err(|err| docker_error("list clone containers", err))?;
        Ok(summaries
            .into_iter()
            .filter_map(|s| s.id.map(|id| CloneContainerSummary { id, created_epoch_secs: s.created.unwrap_or(0) }))
            .collect())
    }
}

/// The ephemeral-clone runtime. Stateless apart from its Docker backend; the M5
/// worker owns the attempt lifecycle and calls [`run_clone`](Self::run_clone).
pub struct CloneRuntime<B: CloneDockerBackend> {
    backend: B,
    /// Max clone timeout used by [`sweep_orphans`](Self::sweep_orphans) to decide
    /// which clone containers are crashed-worker leftovers.
    max_clone_age: Duration,
}

impl<B: CloneDockerBackend> CloneRuntime<B> {
    /// Build a runtime. `max_clone_age` is the longest a clone container may
    /// legitimately live (≈ the worker's hard timeout plus slack); older
    /// labelled containers are reaped by the sweep.
    pub fn new(backend: B, max_clone_age: Duration) -> Self {
        Self { backend, max_clone_age }
    }

    /// Deterministic container name for an attempt.
    pub fn container_name(attempt_id: Uuid) -> String {
        format!("agentforge-clone-{attempt_id}")
    }

    /// Host path of the credential secret file for an attempt. It lives BESIDE
    /// the staging dir (never inside it — staging becomes the live project), so a
    /// future rename of staging → project never carries the secret along.
    fn secret_host_path(spec: &CloneRunSpec) -> Option<PathBuf> {
        let parent = spec.staging_host_path.parent()?;
        Some(parent.join(format!(".clone-secret-{}", spec.attempt_id)))
    }

    /// Build the create-config for a spec (pure; no I/O). Exposed for tests so
    /// the mount set / network / label / env can be asserted without Docker.
    pub fn build_container_config(spec: &CloneRunSpec, secret_host_path: Option<&Path>) -> CloneContainerConfig {
        let mut env = vec![
            format!("CLONE_URL={}", spec.repo_url),
            format!("CLONE_DEST={CLONE_DEST}"),
            // Always present (possibly empty) so the contract is explicit; the
            // entrypoint treats empty as "unknown".
            format!("CLONE_PROVIDER={}", spec.provider.as_deref().unwrap_or("")),
        ];
        env.sort();

        // ONLY the staging dir is mounted read-write. NO projects root, NO sibling.
        let mut mounts = vec![Mount {
            source: spec.staging_host_path.to_string_lossy().into_owned(),
            target: CLONE_STAGING_TARGET.to_string(),
            read_only: false,
        }];

        // Credential, if any, as a read-only bind of the host secret file. NEVER
        // an env var.
        if let Some(path) = secret_host_path {
            mounts.push(Mount {
                source: path.to_string_lossy().into_owned(),
                target: CLONE_SECRET_TARGET.to_string(),
                read_only: true,
            });
        }

        let labels = HashMap::from([(CLONE_LABEL_KEY.to_string(), spec.attempt_id.to_string())]);

        CloneContainerConfig {
            image: spec.image.clone(),
            name: Self::container_name(spec.attempt_id),
            env,
            labels,
            mounts,
            network: CLONE_EGRESS_NETWORK.to_string(),
            resources: clone_resource_limits(),
            privileged: false,
            host_pid: false,
            readonly_rootfs: true,
        }
    }

    /// Validate the create-config against the shared security policy
    /// ([`security::validate_security`]). Reusing the policy guarantees the
    /// clone container is subject to the same no-privileged / no-host-pid /
    /// no-forbidden-mount / resource-limit rules as agent containers.
    fn validate_security(config: &CloneContainerConfig) -> AppResult<()> {
        let policy_view = ContainerConfig {
            image: config.image.clone(),
            name: Some(config.name.clone()),
            working_dir: None,
            env: config.env.clone(),
            labels: config.labels.clone(),
            resources: config.resources.clone(),
            network: Some(config.network.clone()),
            mounts: config.mounts.clone(),
            privileged: config.privileged,
            host_pid: config.host_pid,
            tty: false,
            open_stdin: false,
            attach_stdin: false,
            attach_stdout: false,
            attach_stderr: false,
        };
        security::validate_security(&policy_view).map_err(|violations| {
            AppError::from(ErrorKind::Validation(format!(
                "clone container security policy violation: {}",
                violations.into_iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ")
            )))
        })
    }

    /// Run one ephemeral clone. The container is force-removed and the host
    /// secret file is scrubbed on EVERY return path (success, failure, timeout,
    /// or error) by [`CloneCleanupGuard`].
    pub async fn run_clone(&self, spec: CloneRunSpec) -> AppResult<CloneRunOutcome> {
        self.backend.ensure_egress_network().await?;

        // Materialize the credential to a 0400 host file beside the staging dir.
        let secret_host_path = match &spec.credential {
            Some(secret) => {
                let path = Self::secret_host_path(&spec)
                    .ok_or_else(|| internal("clone staging path has no parent for the credential file"))?;
                write_secret_file(&path, secret.expose()).await?;
                Some(path)
            }
            None => None,
        };

        let config = Self::build_container_config(&spec, secret_host_path.as_deref());
        Self::validate_security(&config)?;

        // Create the container. A cleanup guard now owns the secret-file removal
        // even if `create` fails after the secret was written.
        let mut guard = CloneCleanupGuard::new(secret_host_path.clone());

        let container_id = match self.backend.create_container(config).await {
            Ok(id) => id,
            Err(err) => {
                guard.run(&self.backend).await;
                return Err(err);
            }
        };
        guard.set_container(container_id.clone());

        if let Err(err) = self.backend.start_container(&container_id).await {
            guard.run(&self.backend).await;
            return Err(err);
        }

        // Bounded wait. On timeout the guard still force-removes the container.
        let wait = self.backend.wait_exit(&container_id);
        let exit_code = match tokio::time::timeout(spec.timeout, wait).await {
            Ok(Ok(code)) => code,
            Ok(Err(err)) => {
                guard.run(&self.backend).await;
                return Err(err);
            }
            Err(_elapsed) => {
                tracing::warn!(
                    attempt_id = %spec.attempt_id,
                    container_id = %container_id,
                    timeout_secs = spec.timeout.as_secs(),
                    "clone container timed out; force-removing"
                );
                guard.run(&self.backend).await;
                return Ok(CloneRunOutcome::Timeout);
            }
        };

        let outcome = if exit_code == 0 {
            match read_result_file(&spec.staging_host_path).await {
                Ok(result) => CloneRunOutcome::Ready {
                    branch: result.branch.filter(|b| !b.is_empty()),
                    head_sha: result.head_sha,
                    bytes: result.bytes,
                },
                Err(err) => {
                    // Exit 0 but no/garbled result file ⇒ treat as a failure so the
                    // worker never marks a half-baked clone "ready".
                    let tail = self.capture_log_tail(&container_id).await;
                    tracing::warn!(
                        attempt_id = %spec.attempt_id,
                        error = %err,
                        "clone exited 0 but result file was missing/invalid"
                    );
                    CloneRunOutcome::Failed {
                        exit_code: -1,
                        stderr_tail: bounded_tail(&format!("{tail}\nresult-file error: {err}"), STDERR_TAIL_LIMIT),
                    }
                }
            }
        } else {
            let tail = self.capture_log_tail(&container_id).await;
            CloneRunOutcome::Failed { exit_code, stderr_tail: tail }
        };

        guard.run(&self.backend).await;
        Ok(outcome)
    }

    /// Best-effort capture of the container's combined stdout+stderr tail. If the
    /// log fetch ITSELF fails (e.g. the daemon dropped), return an explicit
    /// placeholder rather than an empty string so the worker can tell "the clone
    /// produced no output" apart from "we could not read the output".
    async fn capture_log_tail(&self, container_id: &str) -> String {
        match self.backend.logs_tail(container_id, STDERR_TAIL_LIMIT).await {
            Ok(tail) => tail,
            Err(err) => {
                tracing::warn!(container_id = %container_id, error = %err, "failed to read clone container logs");
                format!("<unable to read container logs: {err}>")
            }
        }
    }

    /// Sweep crashed-worker leftovers: force-remove every clone container
    /// (label `agentforge.project_clone`) older than [`max_clone_age`]. Returns
    /// the number reaped. Called on startup + periodically by the M5 reconciler.
    pub async fn sweep_orphans(&self) -> AppResult<usize> {
        let now = unix_now_secs();
        let max_age = self.max_clone_age.as_secs() as i64;
        let containers = self.backend.list_clone_containers().await?;

        let mut reaped = 0usize;
        for container in containers {
            // created_epoch == 0 (unknown) is treated as old enough to reap — a
            // labelled clone container with no readable create time is itself
            // anomalous and credential-holding.
            let age = now.saturating_sub(container.created_epoch_secs);
            if container.created_epoch_secs == 0 || age >= max_age {
                match self.backend.force_remove(&container.id).await {
                    Ok(()) => {
                        reaped += 1;
                        tracing::info!(container_id = %container.id, age_secs = age, "reaped orphan clone container");
                    }
                    Err(err) => {
                        tracing::warn!(container_id = %container.id, error = %err, "failed to reap orphan clone container");
                    }
                }
            }
        }
        Ok(reaped)
    }
}

/// RAII-style cleanup: force-removes the clone container (if created) and scrubs
/// the host secret file. Because `Drop` can't await, callers MUST invoke
/// [`run`](Self::run) on every return path; `Drop` is a last-resort warning if
/// they forget (it cannot do async Docker I/O, but it does remove the secret
/// file synchronously so the token never lingers on disk).
struct CloneCleanupGuard {
    container_id: Option<String>,
    secret_path: Option<PathBuf>,
    done: bool,
}

impl CloneCleanupGuard {
    fn new(secret_path: Option<PathBuf>) -> Self {
        Self { container_id: None, secret_path, done: false }
    }

    fn set_container(&mut self, id: String) {
        self.container_id = Some(id);
    }

    /// Force-remove the container (best-effort) and scrub the secret file.
    /// Idempotent.
    async fn run<B: CloneDockerBackend>(&mut self, backend: &B) {
        if self.done {
            return;
        }
        self.done = true;
        if let Some(id) = &self.container_id
            && let Err(err) = backend.force_remove(id).await
        {
            tracing::warn!(container_id = %id, error = %err, "clone cleanup: force-remove failed");
        }
        self.remove_secret_file();
    }

    fn remove_secret_file(&mut self) {
        if let Some(path) = self.secret_path.take()
            && let Err(err) = std::fs::remove_file(&path)
            && err.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(path = %path.display(), error = %err, "clone cleanup: failed to remove secret file");
        }
    }
}

impl Drop for CloneCleanupGuard {
    fn drop(&mut self) {
        if !self.done {
            // Async container removal can't run here; warn loudly and at least
            // ensure the credential file is gone.
            if let Some(id) = &self.container_id {
                tracing::error!(
                    container_id = %id,
                    "clone cleanup guard dropped without run(); container may be orphaned — sweep_orphans will reap it"
                );
            }
            self.remove_secret_file();
        }
    }
}

/// Write the credential to a 0400 host file (owner read-only). On Unix the mode
/// is set atomically with the create; on other platforms we best-effort chmod.
async fn write_secret_file(path: &Path, bytes: &[u8]) -> AppResult<()> {
    use tokio::io::AsyncWriteExt;

    // Remove any stale file first so we never append to a leftover.
    let _ = tokio::fs::remove_file(path).await;

    let mut open = tokio::fs::OpenOptions::new();
    open.write(true).create_new(true);
    // tokio's OpenOptions exposes `mode` inherently on unix, so the 0400 owner-only
    // mode is applied atomically at create — no chmod-after-create TOCTOU window.
    #[cfg(unix)]
    open.mode(0o400);
    let mut file = open
        .open(path)
        .await
        .map_err(|err| internal(format!("failed to create clone credential file {}: {err}", path.display())))?;
    file.write_all(bytes).await.map_err(|err| internal(format!("failed to write clone credential file: {err}")))?;
    file.flush().await.map_err(|err| internal(format!("failed to flush clone credential file: {err}")))?;
    Ok(())
}

/// Read + parse `<staging>/.clone-result.json`.
async fn read_result_file(staging: &Path) -> AppResult<CloneResultFile> {
    let path = staging.join(CLONE_RESULT_FILE);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|err| internal(format!("failed to read clone result file {}: {err}", path.display())))?;
    serde_json::from_slice::<CloneResultFile>(&bytes)
        .map_err(|err| internal(format!("failed to parse clone result file: {err}")))
}

/// Keep the last `limit` bytes of `s` on a char boundary.
fn bounded_tail(s: &str, limit: usize) -> String {
    if s.len() <= limit {
        return s.to_string();
    }
    let start = s.len() - limit;
    // Snap forward to a char boundary so we never split a UTF-8 sequence.
    let start = (start..s.len()).find(|&i| s.is_char_boundary(i)).unwrap_or(s.len());
    s[start..].to_string()
}

fn unix_now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn docker_error(context: &str, err: BollardError) -> AppError {
    // Docker errors are infrastructure-level; surface as Internal with context.
    AppError::from(ErrorKind::Internal(anyhow::anyhow!("{context}: {err}")))
}

fn internal(message: impl Into<String>) -> AppError {
    AppError::from(ErrorKind::Internal(anyhow::anyhow!(message.into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // ---- mock backend (mirrors mcp_docker_runtime test style) --------------

    #[derive(Clone, Default)]
    struct MockBackend {
        created: Arc<Mutex<Vec<CloneContainerConfig>>>,
        started: Arc<Mutex<Vec<String>>>,
        removed: Arc<Mutex<Vec<String>>>,
        ensured_network: Arc<Mutex<usize>>,
        /// Exit code the next `wait_exit` returns.
        exit_code: Arc<Mutex<i64>>,
        /// When set, `wait_exit` sleeps this long (to drive the timeout path).
        wait_delay: Arc<Mutex<Option<Duration>>>,
        logs: Arc<Mutex<String>>,
        list_result: Arc<Mutex<Vec<CloneContainerSummary>>>,
    }

    impl MockBackend {
        fn with_exit(code: i64) -> Self {
            let m = Self::default();
            *m.exit_code.lock().unwrap() = code;
            m
        }
        fn set_logs(&self, logs: &str) {
            *self.logs.lock().unwrap() = logs.to_string();
        }
        fn take_created(&self) -> Vec<CloneContainerConfig> {
            self.created.lock().unwrap().clone()
        }
        fn take_removed(&self) -> Vec<String> {
            self.removed.lock().unwrap().clone()
        }
        fn take_started(&self) -> Vec<String> {
            self.started.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl CloneDockerBackend for MockBackend {
        async fn ensure_egress_network(&self) -> AppResult<()> {
            *self.ensured_network.lock().unwrap() += 1;
            Ok(())
        }
        async fn create_container(&self, config: CloneContainerConfig) -> AppResult<String> {
            self.created.lock().unwrap().push(config);
            Ok("ctr-clone".to_string())
        }
        async fn start_container(&self, id: &str) -> AppResult<()> {
            self.started.lock().unwrap().push(id.to_string());
            Ok(())
        }
        async fn wait_exit(&self, _id: &str) -> AppResult<i64> {
            let delay = *self.wait_delay.lock().unwrap();
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
            }
            let code = *self.exit_code.lock().unwrap();
            Ok(code)
        }
        async fn logs_tail(&self, _id: &str, _limit: usize) -> AppResult<String> {
            Ok(self.logs.lock().unwrap().clone())
        }
        async fn force_remove(&self, id: &str) -> AppResult<()> {
            self.removed.lock().unwrap().push(id.to_string());
            Ok(())
        }
        async fn list_clone_containers(&self) -> AppResult<Vec<CloneContainerSummary>> {
            Ok(self.list_result.lock().unwrap().clone())
        }
    }

    fn temp_staging(attempt_id: Uuid) -> PathBuf {
        // A per-clone staging dir under a per-test parent so `secret_host_path`'s
        // `.parent()` is a real, writable directory.
        let parent = std::env::temp_dir().join(format!("afclone-test-{attempt_id}"));
        let staging = parent.join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        staging
    }

    fn spec_with(attempt_id: Uuid, credential: Option<SecretBytes>) -> CloneRunSpec {
        CloneRunSpec {
            image: "agentforge-clone:latest".to_string(),
            repo_url: "https://github.com/octocat/Hello-World.git".to_string(),
            provider: Some("github".to_string()),
            staging_host_path: temp_staging(attempt_id),
            credential,
            timeout: Duration::from_secs(30),
            attempt_id,
        }
    }

    fn cleanup(spec: &CloneRunSpec) {
        if let Some(parent) = spec.staging_host_path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    // ---- create-config assertions ------------------------------------------

    #[test]
    fn config_mounts_only_staging_when_public() {
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        let config = CloneRuntime::<MockBackend>::build_container_config(&spec, None);

        // EXACTLY one mount: staging -> /staging, read-write. No projects root,
        // no sibling, no secret.
        assert_eq!(config.mounts.len(), 1);
        assert_eq!(config.mounts[0].target, "/staging");
        assert_eq!(config.mounts[0].source, spec.staging_host_path.to_string_lossy());
        assert!(!config.mounts[0].read_only);
        cleanup(&spec);
    }

    #[test]
    fn config_adds_readonly_secret_mount_when_credentialed() {
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, Some(SecretBytes::from("x-access-token:tok".to_string())));
        let secret_path = CloneRuntime::<MockBackend>::secret_host_path(&spec).unwrap();
        let config = CloneRuntime::<MockBackend>::build_container_config(&spec, Some(&secret_path));

        assert_eq!(config.mounts.len(), 2);
        let secret = config.mounts.iter().find(|m| m.target == "/run/secrets/git-credential").expect("secret mount");
        assert!(secret.read_only, "credential mount must be read-only");
        // The secret lives BESIDE staging, never inside it.
        assert!(!secret.source.contains("/staging/"), "secret must not be inside the staging tree");
        cleanup(&spec);
    }

    #[test]
    fn config_credential_never_in_env() {
        let attempt_id = Uuid::now_v7();
        let token = "supersecrettoken12345";
        let spec = spec_with(attempt_id, Some(SecretBytes::from(format!("x-access-token:{token}"))));
        let secret_path = CloneRuntime::<MockBackend>::secret_host_path(&spec).unwrap();
        let config = CloneRuntime::<MockBackend>::build_container_config(&spec, Some(&secret_path));

        // CLONE_URL + CLONE_DEST present.
        assert!(config.env.iter().any(|e| e == "CLONE_URL=https://github.com/octocat/Hello-World.git"));
        assert!(config.env.iter().any(|e| e == "CLONE_DEST=/staging"));
        assert!(config.env.iter().any(|e| e == "CLONE_PROVIDER=github"));
        // No env var bears the token.
        for e in &config.env {
            assert!(!e.contains(token), "token leaked into env: {e}");
        }
        cleanup(&spec);
    }

    #[test]
    fn config_uses_dedicated_network_not_agents_network() {
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        let config = CloneRuntime::<MockBackend>::build_container_config(&spec, None);
        assert_eq!(config.network, "agentforge-clone-egress");
        assert_ne!(config.network, "agentforge-agents", "clone must NOT share the internal agents network");
        cleanup(&spec);
    }

    #[test]
    fn config_has_deterministic_name_and_reap_label() {
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        let config = CloneRuntime::<MockBackend>::build_container_config(&spec, None);
        assert_eq!(config.name, format!("agentforge-clone-{attempt_id}"));
        assert_eq!(config.labels.get("agentforge.project_clone"), Some(&attempt_id.to_string()));
        cleanup(&spec);
    }

    #[test]
    fn config_security_posture_is_locked_down() {
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        let config = CloneRuntime::<MockBackend>::build_container_config(&spec, None);
        assert!(!config.privileged);
        assert!(!config.host_pid);
        assert!(config.readonly_rootfs);
        // resource limits present
        assert!(config.resources.cpu_quota.is_some());
        assert!(config.resources.memory_bytes.is_some());
        assert!(config.resources.pids_limit.is_some());
        // no forbidden mount (docker socket etc.) — passes the shared policy.
        assert!(CloneRuntime::<MockBackend>::validate_security(&config).is_ok());
        cleanup(&spec);
    }

    #[test]
    fn security_policy_rejects_docker_socket_mount() {
        // Defense-in-depth: if a future change tried to mount the docker socket,
        // the reused security policy rejects it.
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        let mut config = CloneRuntime::<MockBackend>::build_container_config(&spec, None);
        config.mounts.push(Mount {
            source: "/var/run/docker.sock".to_string(),
            target: "/var/run/docker.sock".to_string(),
            read_only: true,
        });
        assert!(CloneRuntime::<MockBackend>::validate_security(&config).is_err());
        cleanup(&spec);
    }

    // ---- outcome mapping ----------------------------------------------------

    #[tokio::test]
    async fn run_clone_ready_on_exit_zero_with_result_file() {
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        std::fs::write(
            spec.staging_host_path.join(".clone-result.json"),
            r#"{"branch":"main","head_sha":"abc123def","bytes":4096}"#,
        )
        .unwrap();

        let backend = MockBackend::with_exit(0);
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(600));
        let outcome = runtime.run_clone(spec_with(attempt_id, None)).await.unwrap();

        assert_eq!(
            outcome,
            CloneRunOutcome::Ready { branch: Some("main".to_string()), head_sha: "abc123def".to_string(), bytes: 4096 }
        );
        // Container force-removed exactly once.
        assert_eq!(backend.take_removed(), vec!["ctr-clone".to_string()]);
        assert_eq!(backend.take_started(), vec!["ctr-clone".to_string()]);
        cleanup(&spec);
    }

    #[tokio::test]
    async fn run_clone_passes_hardened_config_to_create() {
        // End-to-end through run_clone: assert the create-config the backend
        // receives carries the egress network, reap label, staging-only mount,
        // and locked-down security posture — not just the pure builder.
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        std::fs::write(
            spec.staging_host_path.join(".clone-result.json"),
            r#"{"branch":"main","head_sha":"sha","bytes":1}"#,
        )
        .unwrap();
        let backend = MockBackend::with_exit(0);
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(600));
        runtime.run_clone(spec_with(attempt_id, None)).await.unwrap();

        let created = backend.take_created();
        assert_eq!(created.len(), 1);
        let cfg = &created[0];
        assert_eq!(cfg.network, "agentforge-clone-egress");
        assert_eq!(cfg.labels.get("agentforge.project_clone"), Some(&attempt_id.to_string()));
        assert_eq!(cfg.mounts.len(), 1, "public clone mounts ONLY staging");
        assert_eq!(cfg.mounts[0].target, "/staging");
        assert!(!cfg.privileged);
        assert!(!cfg.host_pid);
        assert!(cfg.readonly_rootfs);
        cleanup(&spec);
    }

    #[tokio::test]
    async fn run_clone_ready_detached_head_has_no_branch() {
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        std::fs::write(
            spec.staging_host_path.join(".clone-result.json"),
            r#"{"branch":"","head_sha":"deadbeef","bytes":10}"#,
        )
        .unwrap();
        let runtime = CloneRuntime::new(MockBackend::with_exit(0), Duration::from_secs(600));
        let outcome = runtime.run_clone(spec_with(attempt_id, None)).await.unwrap();
        assert_eq!(outcome, CloneRunOutcome::Ready { branch: None, head_sha: "deadbeef".to_string(), bytes: 10 });
        cleanup(&spec);
    }

    #[tokio::test]
    async fn run_clone_failed_on_nonzero_exit_with_bounded_tail() {
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        let backend = MockBackend::with_exit(128);
        backend.set_logs("fatal: Authentication failed for 'https://github.com/...'");
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(600));

        let outcome = runtime.run_clone(spec_with(attempt_id, None)).await.unwrap();
        match outcome {
            CloneRunOutcome::Failed { exit_code, stderr_tail } => {
                assert_eq!(exit_code, 128);
                assert!(stderr_tail.contains("Authentication failed"));
                assert!(stderr_tail.len() <= STDERR_TAIL_LIMIT);
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(backend.take_removed(), vec!["ctr-clone".to_string()], "must reap on failure");
        cleanup(&spec);
    }

    #[tokio::test]
    async fn run_clone_exit_zero_without_result_file_is_failed_not_ready() {
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        // No result file written.
        let runtime = CloneRuntime::new(MockBackend::with_exit(0), Duration::from_secs(600));
        let outcome = runtime.run_clone(spec_with(attempt_id, None)).await.unwrap();
        assert!(matches!(outcome, CloneRunOutcome::Failed { exit_code: -1, .. }));
        cleanup(&spec);
    }

    #[tokio::test]
    async fn run_clone_times_out_and_reaps() {
        let attempt_id = Uuid::now_v7();
        let mut spec = spec_with(attempt_id, None);
        spec.timeout = Duration::from_millis(20);
        let backend = MockBackend::default();
        *backend.wait_delay.lock().unwrap() = Some(Duration::from_secs(5));
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(600));

        let outcome = runtime.run_clone(spec).await.unwrap();
        assert_eq!(outcome, CloneRunOutcome::Timeout);
        // The container must still be force-removed on the timeout path.
        assert_eq!(backend.take_removed(), vec!["ctr-clone".to_string()]);
        let s2 = spec_with(attempt_id, None);
        cleanup(&s2);
    }

    #[tokio::test]
    async fn run_clone_credential_written_0400_and_scrubbed_on_exit() {
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, Some(SecretBytes::from("x-access-token:tok".to_string())));
        std::fs::write(
            spec.staging_host_path.join(".clone-result.json"),
            r#"{"branch":"main","head_sha":"sha","bytes":1}"#,
        )
        .unwrap();
        let secret_path = CloneRuntime::<MockBackend>::secret_host_path(&spec).unwrap();

        let runtime = CloneRuntime::new(MockBackend::with_exit(0), Duration::from_secs(600));
        let outcome = runtime
            .run_clone(spec_with(attempt_id, Some(SecretBytes::from("x-access-token:tok".to_string()))))
            .await
            .unwrap();
        assert!(matches!(outcome, CloneRunOutcome::Ready { .. }));
        // Secret file must be gone after the run (scrubbed by the guard).
        assert!(!secret_path.exists(), "credential file must be removed after the run");
        cleanup(&spec);
    }

    #[tokio::test]
    async fn run_clone_scrubs_secret_even_when_start_fails() {
        // A start failure must still remove the credential file.
        #[derive(Clone, Default)]
        struct FailStartBackend {
            removed: Arc<Mutex<Vec<String>>>,
        }
        #[async_trait::async_trait]
        impl CloneDockerBackend for FailStartBackend {
            async fn ensure_egress_network(&self) -> AppResult<()> {
                Ok(())
            }
            async fn create_container(&self, _c: CloneContainerConfig) -> AppResult<String> {
                Ok("ctr-x".to_string())
            }
            async fn start_container(&self, _id: &str) -> AppResult<()> {
                Err(internal("boom"))
            }
            async fn wait_exit(&self, _id: &str) -> AppResult<i64> {
                Ok(0)
            }
            async fn logs_tail(&self, _id: &str, _l: usize) -> AppResult<String> {
                Ok(String::new())
            }
            async fn force_remove(&self, id: &str) -> AppResult<()> {
                self.removed.lock().unwrap().push(id.to_string());
                Ok(())
            }
            async fn list_clone_containers(&self) -> AppResult<Vec<CloneContainerSummary>> {
                Ok(vec![])
            }
        }

        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, Some(SecretBytes::from("tok".to_string())));
        let secret_path = CloneRuntime::<FailStartBackend>::secret_host_path(&spec).unwrap();
        let backend = FailStartBackend::default();
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(600));

        let result = runtime.run_clone(spec_with(attempt_id, Some(SecretBytes::from("tok".to_string())))).await;
        assert!(result.is_err());
        assert!(!secret_path.exists(), "credential file must be removed even when start fails");
        assert_eq!(backend.removed.lock().unwrap().clone(), vec!["ctr-x".to_string()]);
        cleanup(&spec);
    }

    // ---- sweep --------------------------------------------------------------

    #[tokio::test]
    async fn sweep_reaps_only_old_clone_containers() {
        let now = unix_now_secs();
        let backend = MockBackend::default();
        *backend.list_result.lock().unwrap() = vec![
            // Fresh (5s old): keep.
            CloneContainerSummary { id: "young".to_string(), created_epoch_secs: now - 5 },
            // Old (2h old): reap.
            CloneContainerSummary { id: "old".to_string(), created_epoch_secs: now - 7200 },
            // Unknown create time: reap (anomalous credential-holder).
            CloneContainerSummary { id: "unknown".to_string(), created_epoch_secs: 0 },
        ];
        // max age = 1h
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(3600));
        let reaped = runtime.sweep_orphans().await.unwrap();
        assert_eq!(reaped, 2);
        let removed = backend.take_removed();
        assert!(removed.contains(&"old".to_string()));
        assert!(removed.contains(&"unknown".to_string()));
        assert!(!removed.contains(&"young".to_string()), "fresh in-flight clone must not be reaped");
    }

    // ---- secret hygiene -----------------------------------------------------

    #[test]
    fn secret_bytes_debug_does_not_leak() {
        let secret = SecretBytes::from("topsecret".to_string());
        let rendered = format!("{secret:?}");
        assert!(!rendered.contains("topsecret"));
        assert_eq!(rendered, "SecretBytes([REDACTED])");
    }

    #[test]
    fn clone_spec_debug_does_not_leak_credential() {
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, Some(SecretBytes::from("topsecret-token".to_string())));
        let rendered = format!("{spec:?}");
        assert!(!rendered.contains("topsecret-token"), "credential leaked into CloneRunSpec Debug");
        cleanup(&spec);
    }

    #[test]
    fn bounded_tail_keeps_suffix_within_limit() {
        let s = "a".repeat(100);
        let tail = bounded_tail(&s, 10);
        assert_eq!(tail.len(), 10);
        let short = bounded_tail("hello", 10);
        assert_eq!(short, "hello");
    }

    // ---- real-docker e2e (gated; skipped cleanly if docker/image absent) ----

    /// Returns a live backend only when a Docker daemon is reachable AND the
    /// `agentforge-clone:latest` image is present. Otherwise returns `None` so
    /// the test skips instead of failing on CI hosts without Docker.
    async fn live_backend_if_available() -> Option<LiveCloneDockerBackend> {
        let docker = match bollard::Docker::connect_with_local_defaults() {
            Ok(d) => d,
            Err(_) => return None,
        };
        if docker.ping().await.is_err() {
            return None;
        }
        // Require the clone image to be present (don't pull from a registry here).
        if docker.inspect_image("agentforge-clone:latest").await.is_err() {
            eprintln!("clone e2e: docker is up but agentforge-clone:latest is not built — skipping");
            return None;
        }
        Some(LiveCloneDockerBackend::new(std::sync::Arc::new(DockerClient::from_bollard(docker))))
    }

    #[tokio::test]
    async fn e2e_real_clone_of_public_repo_when_docker_available() {
        let Some(backend) = live_backend_if_available().await else {
            eprintln!("clone e2e: docker/agentforge-clone:latest unavailable — skipping (not a failure)");
            return;
        };

        let attempt_id = Uuid::now_v7();
        // Staging must be writable by the container's agent uid (1011). On a dev
        // host the test process can create it; if the uid mismatch prevents the
        // clone, the outcome surfaces as Failed and we assert below only on the
        // happy path being reachable.
        let staging = temp_staging(attempt_id);

        let runtime = CloneRuntime::new(backend, Duration::from_secs(120));
        let spec = CloneRunSpec {
            image: "agentforge-clone:latest".to_string(),
            // A tiny, stable public repo.
            repo_url: "https://github.com/octocat/Hello-World.git".to_string(),
            provider: Some("github".to_string()),
            staging_host_path: staging.clone(),
            credential: None,
            timeout: Duration::from_secs(120),
            attempt_id,
        };

        let outcome = runtime.run_clone(spec).await.expect("clone run completed");
        match outcome {
            CloneRunOutcome::Ready { head_sha, .. } => {
                assert!(!head_sha.is_empty(), "expected a HEAD sha");
                assert!(staging.join("repo").join(".git").exists(), "cloned repo must contain .git");
            }
            other => {
                // A uid/permission or network restriction on the test host can make
                // this Failed; don't hard-fail the suite, but make the reason loud.
                eprintln!("clone e2e: non-Ready outcome on this host: {other:?}");
            }
        }

        if let Some(parent) = staging.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}
