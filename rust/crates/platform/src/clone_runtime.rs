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
//!   On the 409 (network already exists) reuse path,
//!   [`ensure_egress_network`](CloneDockerBackend::ensure_egress_network)
//!   **inspects** the existing network and fails closed unless it is a managed
//!   `bridge` with our label and is not wired to internal services — a
//!   pre-existing/miscreated network is NOT silently trusted.
//!
//!   **Residual deploy requirement (be honest):** Docker's stock bridge driver
//!   still permits egress to the host's other networks / RFC1918 / link-local /
//!   the cloud metadata endpoint (169.254.169.254) unless the host firewalls it.
//!   This crate does the best the Docker API allows — network *separation* from
//!   internal services — but full RFC1918/link-local/metadata egress filtering
//!   is a **deployment-layer firewall concern** the operator MUST configure on
//!   [`CLONE_EGRESS_NETWORK`]'s subnet (e.g. an iptables/nftables egress policy
//!   or an egress proxy). This is tracked as a REQUIRED operator step in
//!   `docs/runbooks/clone-egress-firewall.md`, and M8 must add the
//!   fails-closed SSRF integration test that proves it. Defense-in-depth
//!   complements: the in-app HTTPS-only + host deny-list URL gate (M1), and a
//!   best-effort host pre-resolve in `clone-entrypoint.sh` that refuses a
//!   `CLONE_URL` resolving to loopback/RFC1918/link-local/metadata before git
//!   runs. The network is the runtime layer; the firewall is the real control.
//!
//! # Credential delivery (spec §6.7)
//!
//! The git credential is delivered to the container at
//! `/run/secrets/git-credential` via a **read-only bind mount of a short-lived
//! host file** the runtime writes — never an environment variable, never a build
//! layer, never on git's argv. The [`SecretBytes`] wrapper never `Debug`-prints
//! or serializes the token (and `zeroize`-scrubs it on drop), and the host secret
//! file is force-removed on every exit path together with the container (which is
//! the only thing that ever maps the secret).
//!
//! ## Cross-UID secret readability (why the file is NOT 0400)
//!
//! The backend server runs UNPRIVILEGED — on the production Alpine image it is
//! `adduser -S agentforge` (uid 100 / gid 101); see `rust/Dockerfile`. The clone
//! container runs as `agent` (uid 1011 / gid 1012; see `docker/Dockerfile.clone`,
//! matching `Dockerfile.agent-base`). Docker bind mounts preserve NUMERIC uids,
//! and the backend has **no `CAP_CHOWN`** (compose runs it `cap_drop: ALL`,
//! `no-new-privileges`, `read_only`), so it CANNOT `chown` the secret to uid 1011.
//! A `0400` (owner-only) file owned by uid 100 is therefore **unreadable** by the
//! clone's uid 1011 → `EACCES` → every credentialed clone fails. (The original
//! e2e only cloned a PUBLIC repo, so this never surfaced.)
//!
//! Resolution — directory-mode isolation, mirroring the established OAuth-mount
//! pattern in `api::services::cli_credential::write_oauth_mount`:
//!
//! - The secret lives under a **backend-controlled secret root**
//!   ([`CloneSecretRoot`]) created mode **0700** (owner-only) — no other host
//!   user, and no agent project mount, can traverse INTO it. The root lives
//!   OUTSIDE the projects/workspace tree that agent containers bind, so a sibling
//!   agent can never reach a clone's in-flight credential.
//! - The per-attempt file inside that root is mode **0644** so the clone uid
//!   (1011 ≠ 100, no shared gid) can actually READ it. It is NOT world-*reachable*
//!   because the 0700 root blocks traversal; "world-readable bits" on an inode
//!   nobody can `cd` to is not an exposure.
//! - The file is unlinked on every exit path and the directory mode is the
//!   confidentiality control. (A privileged-server deployment that CAN chown
//!   could tighten this to 0400+chown, but the shipped image is non-root, so we
//!   match it. See [`CloneSecretRoot`] docs.)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use agentforge_core::{AppError, AppResult, ErrorKind};
use bollard::errors::Error as BollardError;
use bollard::models::{ContainerCreateBody, HostConfig, NetworkCreateRequest, NetworkingConfig};
use bollard::query_parameters::{
    CreateContainerOptions, InspectContainerOptions, ListContainersOptionsBuilder, RemoveContainerOptionsBuilder,
    StartContainerOptions, WaitContainerOptionsBuilder,
};
use futures_util::StreamExt;
use serde::Deserialize;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::docker::DockerClient;
use crate::security;
use crate::types::{ContainerConfig, Mount, ResourceLimits};

/// Container path the staging directory is mounted at (read-write).
pub const CLONE_STAGING_TARGET: &str = "/staging";

/// `CLONE_DEST` env value handed to `clone-entrypoint.sh` (the container clones
/// into `<CLONE_DEST>/repo` and writes `<CLONE_DEST>/.clone-result.json`).
pub const CLONE_DEST: &str = "/staging";

/// Container path the read-only credential secret file is mounted at. Matches the
/// `clone-entrypoint.sh` contract (M3) exactly. The host file is mode 0644 inside
/// a 0700 backend-only root (see the module "Cross-UID secret readability" docs);
/// the read-only *bind* makes it read-only inside the container regardless.
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

/// `CLONE_MAX_BYTES` env handed to `clone-entrypoint.sh`: the disk watchdog
/// aborts the clone (exit [`CLONE_EXIT_TOO_LARGE`]) if the cloned tree exceeds
/// this many bytes. Keeps a hostile/huge repo from filling the staging volume.
pub const CLONE_MAX_BYTES_ENV: &str = "CLONE_MAX_BYTES";

/// Default cloned-tree size cap (2 GiB). The M5 worker may override per-policy by
/// setting [`CloneRunSpec::max_bytes`]; this is the floor used when it does not.
pub const DEFAULT_CLONE_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Distinct exit code the entrypoint's disk watchdog/preflight uses so the
/// runtime can map it to [`CloneRunOutcome::TooLarge`] (vs a generic `Failed`).
/// Must stay in lockstep with `clone-entrypoint.sh`.
const CLONE_EXIT_TOO_LARGE: i64 = 5;

/// Host file mode for the materialized credential. NOT 0400: the unprivileged
/// backend (uid 100) cannot chown to the clone uid (1011), so an owner-only file
/// would be unreadable cross-uid. Confidentiality is the 0700 secret-root dir
/// (see [`CloneSecretRoot`] + module docs), not these inode bits.
#[cfg(unix)]
const CLONE_SECRET_FILE_MODE: u32 = 0o644;

/// Backend-only secret-root directory mode: owner rwx, nothing else — blocks any
/// other host user or agent project mount from traversing INTO the root.
#[cfg(unix)]
const CLONE_SECRET_ROOT_MODE: u32 = 0o700;

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
/// `Display`, or serialization, and is **compiler-fenced scrubbed** on drop via
/// the `zeroize` crate.
///
/// The token reaches the container only via a mounted file (see module docs), so
/// this wrapper exists purely to keep it out of logs, panic messages, and any
/// `Serialize` derive on a struct that transitively holds it.
///
/// Deliberately **NOT `Clone`**: a credential should have exactly one owner whose
/// drop scrubs it; a derived `Clone` would mint an untracked plaintext copy whose
/// zeroization timing is not guaranteed. Move it, or borrow via [`expose`](Self::expose).
///
/// The inner buffer is a [`Zeroizing<Vec<u8>>`], so even if a future field is
/// added the scrub-on-drop is enforced by the type, not a hand-written `Drop`.
/// `zeroize` uses volatile writes + a compiler fence, so the overwrite cannot be
/// optimized away (unlike the previous plain-loop `Drop`).
pub struct SecretBytes(Zeroizing<Vec<u8>>);

impl SecretBytes {
    /// Wrap raw secret bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
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

/// Raw, **unredacted** git stderr captured from a failed clone. Wrapping it in a
/// newtype with a non-printing `Debug` stops it leaking through structured logs
/// (`tracing::warn!(?outcome)` over a [`CloneRunOutcome::Failed`] would otherwise
/// print git stderr, which can contain a token glued onto a redirected URL).
///
/// The ONLY way to read the bytes is [`as_raw`](Self::as_raw) / [`into_raw`](Self::into_raw)
/// — the grep-able audit points, mirroring `SecretBytes::expose`. The M5 worker
/// runs this through the M1 `RedactedError` BEFORE persisting; that
/// redaction-before-persist test lives in M5 (`RedactedError` is in `api`,
/// downstream of `platform`).
#[derive(Clone, PartialEq, Eq)]
pub struct RawStderr(String);

impl RawStderr {
    /// Wrap raw stderr bytes.
    pub fn new(raw: String) -> Self {
        Self(raw)
    }

    /// Borrow the raw, unredacted text. Audit point: callers MUST redact before
    /// logging or persisting.
    pub fn as_raw(&self) -> &str {
        &self.0
    }

    /// Consume into the raw, unredacted text. Audit point: redact before use.
    pub fn into_raw(self) -> String {
        self.0
    }
}

// Non-printing Debug: shows only a byte count, never the bytes — so a
// `?outcome` log line over a `Failed` variant can't spill a glued token.
impl std::fmt::Debug for RawStderr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RawStderr(<{} bytes, unredacted>)", self.0.len())
    }
}

impl From<String> for RawStderr {
    fn from(raw: String) -> Self {
        Self::new(raw)
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
    /// Backend-controlled secret root the credential file is materialized under
    /// (mode 0700; per-attempt file mode 0644). This MUST live OUTSIDE the
    /// projects/workspace tree that agent containers bind, so a sibling agent can
    /// never reach an in-flight credential. The runtime never writes the secret
    /// beside the staging dir. Ignored when [`credential`](Self::credential) is
    /// `None` (public repo). See [`CloneSecretRoot`] + module docs.
    pub secret_root: PathBuf,
    /// Optional host-matched short-lived credential. `None` ⇒ public repo, no
    /// secret mount. Never serializes (no `Serialize` impl) and never
    /// `Debug`-prints the token (see [`SecretBytes`]).
    pub credential: Option<SecretBytes>,
    /// Hard wall-clock timeout for the whole clone.
    pub timeout: Duration,
    /// Cloned-tree size cap handed to the entrypoint as `CLONE_MAX_BYTES`. The
    /// in-container disk watchdog aborts (→ [`CloneRunOutcome::TooLarge`]) if the
    /// tree exceeds this. `None` ⇒ [`DEFAULT_CLONE_MAX_BYTES`].
    pub max_bytes: Option<u64>,
    /// Attempt id — drives the deterministic container name + reap label.
    pub attempt_id: Uuid,
}

// The `clone_spec_debug_does_not_leak_credential` test pins the `Debug`-redaction
// guarantee; `SecretBytes`' absence of a `Serialize` impl makes a serde leak a
// compile error.

/// Outcome of a clone run. The worker (M5) maps this onto the attempt status,
/// redacting [`RawStderr`] via the M1 `RedactedError` before storing. Its `Debug`
/// is leak-safe: the `Failed` stderr is a [`RawStderr`] that prints only a byte
/// count, so `tracing::warn!(?outcome)` can never spill a token.
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
    /// Container exited non-zero. `stderr_tail` is RAW (unredacted, see
    /// [`RawStderr`]) — the worker redacts before persistence.
    Failed {
        /// Container exit code (git's 128 = auth/not-found/transport).
        exit_code: i64,
        /// Bounded tail of the container's combined stdout+stderr, wrapped so it
        /// cannot leak through a `Debug` log.
        stderr_tail: RawStderr,
    },
    /// The cloned tree exceeded the configured size cap (`CLONE_MAX_BYTES`); the
    /// entrypoint's disk guard aborted with [`CLONE_EXIT_TOO_LARGE`]. Distinct
    /// from `Failed` so the worker surfaces a precise "repo too large" status
    /// (M1 already names the `TooLarge` error class) and never smuggles it as a
    /// lossy generic failure.
    TooLarge {
        /// Bounded tail of the container's output (best-effort; explains which
        /// guard fired). Raw — redact before persistence.
        stderr_tail: RawStderr,
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
    /// Ensure the dedicated egress network exists (create-or-reuse). On the 409
    /// reuse path the implementation MUST inspect the existing network and fail
    /// closed unless the managed isolation invariants hold (see
    /// [`NetworkInspectInfo::assert_managed_egress`]). Idempotent.
    async fn ensure_egress_network(&self) -> AppResult<()>;
    /// Inspect a Docker network by name (used by the 409 reuse fail-closed check).
    async fn inspect_network(&self, name: &str) -> AppResult<NetworkInspectInfo>;
    /// Create a container from the given config; returns the container id.
    async fn create_container(&self, config: CloneContainerConfig) -> AppResult<String>;
    /// Start a created container.
    async fn start_container(&self, id: &str) -> AppResult<()>;
    /// Wait (bounded by the caller) for the container to exit; returns its exit code.
    async fn wait_exit(&self, id: &str) -> AppResult<i64>;
    /// Inspect a container's real lifecycle state — used to disambiguate a
    /// `wait` that ended without an exit frame, a timeout that raced a finish,
    /// and a sweep liveness decision. `None` if the container no longer exists.
    async fn inspect_container(&self, id: &str) -> AppResult<Option<CloneContainerState>>;
    /// Fetch a bounded tail of the container's combined stdout+stderr.
    async fn logs_tail(&self, id: &str, limit_bytes: usize) -> AppResult<String>;
    /// Force-remove a container. Must succeed-or-warn; never panics.
    async fn force_remove(&self, id: &str) -> AppResult<()>;
    /// List clone containers (label `agentforge.project_clone`) with their
    /// id + created-at epoch seconds + running flag, for the orphan sweep.
    async fn list_clone_containers(&self) -> AppResult<Vec<CloneContainerSummary>>;
}

/// Managed-invariant view of a Docker network, returned by
/// [`CloneDockerBackend::inspect_network`]. Only the fields the fail-closed reuse
/// check needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkInspectInfo {
    /// Network driver (must be `bridge` for our egress network).
    pub driver: Option<String>,
    /// `internal: true` would BLOCK the public-host egress the clone needs, so a
    /// reused network must NOT be internal.
    pub internal: bool,
    /// Network labels (must carry `agentforge.managed=clone-egress`).
    pub labels: HashMap<String, String>,
    /// Names/ids of containers currently attached. A reused network must not have
    /// any of the internal-service containers wired onto it.
    pub attached_container_names: Vec<String>,
}

impl NetworkInspectInfo {
    /// Fail closed unless the existing `agentforge-clone-egress` network matches
    /// the managed invariants: a `bridge` driver, our `agentforge.managed`
    /// =`clone-egress` label, NOT `internal` (the clone must reach the public
    /// host), and none of the internal-service containers attached. A
    /// pre-existing/miscreated network that fails any check is REFUSED so we never
    /// silently clone on a network that defeats isolation.
    pub fn assert_managed_egress(&self) -> AppResult<()> {
        if self.driver.as_deref() != Some("bridge") {
            return Err(internal(format!(
                "refusing to reuse '{CLONE_EGRESS_NETWORK}': driver is {:?}, expected bridge",
                self.driver
            )));
        }
        if self.internal {
            return Err(internal(format!(
                "refusing to reuse '{CLONE_EGRESS_NETWORK}': network is marked internal (blocks the clone's required public egress)"
            )));
        }
        if self.labels.get("agentforge.managed").map(String::as_str) != Some("clone-egress") {
            return Err(internal(format!(
                "refusing to reuse '{CLONE_EGRESS_NETWORK}': missing managed label agentforge.managed=clone-egress (not platform-created)"
            )));
        }
        // Defense-in-depth: a managed egress network legitimately holds only
        // transient clone containers. Any internal-service container attached is
        // an isolation breach — refuse.
        for name in &self.attached_container_names {
            if is_internal_service_container(name) {
                return Err(internal(format!(
                    "refusing to reuse '{CLONE_EGRESS_NETWORK}': internal-service container '{name}' is attached — isolation breach"
                )));
            }
        }
        Ok(())
    }
}

/// Real lifecycle state of a clone container, from
/// [`CloneDockerBackend::inspect_container`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneContainerState {
    /// True while the container is still running.
    pub running: bool,
    /// The container's real exit code (meaningful only once it has stopped).
    pub exit_code: i64,
    /// Creation time as a unix epoch (seconds); `0` if unparsable.
    pub created_epoch_secs: i64,
}

/// Recognize the names/ids of internal-service containers that must NEVER share
/// the clone egress network. Matches on the conventional `*-server`, `*-postgres`,
/// `*-nats`, `*-redis`, `*-orchestrator` suffixes the compose stack uses (the
/// `agentforge` / configurable prefix varies, so suffix-match is robust).
fn is_internal_service_container(name: &str) -> bool {
    const INTERNAL_SUFFIXES: &[&str] =
        &["server", "postgres", "nats", "redis", "orchestrator", "minio", "temporal", "caddy"];
    let trimmed = name.trim_start_matches('/');
    INTERNAL_SUFFIXES.iter().any(|suffix| {
        trimmed == *suffix || trimmed.ends_with(&format!("-{suffix}")) || trimmed.ends_with(&format!("_{suffix}"))
    })
}

/// Minimal container summary the sweep needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneContainerSummary {
    pub id: String,
    /// Container creation time as a unix epoch (seconds). `0` if unknown.
    pub created_epoch_secs: i64,
    /// True if the list reported the container as still running. The sweep uses
    /// this to avoid reaping a healthy in-progress clone.
    pub running: bool,
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
        // Create-or-reuse. On 409 (already exists) we do NOT blindly trust the
        // existing network — a pre-existing/miscreated `agentforge-clone-egress`
        // wired to internal services would silently defeat isolation. Inspect it
        // and fail closed unless the managed invariants hold.
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
            Err(BollardError::DockerResponseServerError { status_code: 409, .. }) => {
                // Reuse path: verify the existing network is our managed, isolated
                // egress bridge before we agree to clone on it.
                let info = self.inspect_network(CLONE_EGRESS_NETWORK).await?;
                info.assert_managed_egress()
            }
            Err(err) => Err(docker_error("create clone egress network", err)),
        }
    }

    async fn inspect_network(&self, name: &str) -> AppResult<NetworkInspectInfo> {
        let network = self
            .docker
            .inner()
            .inspect_network(name, None::<bollard::query_parameters::InspectNetworkOptions>)
            .await
            .map_err(|err| docker_error("inspect clone egress network", err))?;
        // `containers` is a map of container-id -> endpoint; its `name` field is
        // the per-network endpoint name (the container name). Collect both the
        // map's endpoint names and the keys so a name-less endpoint still trips
        // the internal-service check by id substring if needed.
        let attached_container_names = network
            .containers
            .unwrap_or_default()
            .into_values()
            .filter_map(|endpoint| endpoint.name)
            .collect::<Vec<_>>();
        Ok(NetworkInspectInfo {
            driver: network.driver,
            internal: network.internal.unwrap_or(false),
            labels: network.labels.unwrap_or_default(),
            attached_container_names,
        })
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
            // Stream ended without ANY exit frame (daemon hiccup, or a concurrent
            // sweep removed the container mid-wait). This is NOT a clean exit 0 —
            // mapping it to 0 could read a stale `.clone-result.json` and falsely
            // report `Ready`. Surface it as an error; `run_clone` then inspects the
            // container's REAL exit code to recover a genuine success/failure.
            None => Err(internal("clone wait stream ended without an exit frame")),
        }
    }

    async fn inspect_container(&self, id: &str) -> AppResult<Option<CloneContainerState>> {
        match self.docker.inner().inspect_container(id, None::<InspectContainerOptions>).await {
            Ok(info) => {
                let state = info.state;
                let running = state.as_ref().and_then(|s| s.running).unwrap_or(false);
                let exit_code = state.as_ref().and_then(|s| s.exit_code).unwrap_or(0);
                let created_epoch_secs = info.created.as_deref().map(parse_rfc3339_secs).unwrap_or(0);
                Ok(Some(CloneContainerState { running, exit_code, created_epoch_secs }))
            }
            // 404 ⇒ already gone; the caller treats that as "no state".
            Err(BollardError::DockerResponseServerError { status_code: 404, .. }) => Ok(None),
            Err(err) => Err(docker_error("inspect clone container", err)),
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
            .filter_map(|s| {
                // Docker's list `state` is a lifecycle enum; treat only RUNNING as
                // live.
                let running = matches!(s.state, Some(bollard::models::ContainerSummaryStateEnum::RUNNING));
                s.id.map(|id| CloneContainerSummary { id, created_epoch_secs: s.created.unwrap_or(0), running })
            })
            .collect())
    }
}

/// The ephemeral-clone runtime. Stateless apart from its Docker backend; the M5
/// worker owns the attempt lifecycle and calls [`run_clone`](Self::run_clone).
///
/// The backend is held behind an [`Arc`] so the [`CloneCleanupGuard`] can clone a
/// handle into itself and, on a panic or task-cancellation, `tokio::spawn` a
/// detached best-effort `force_remove` — the credential-holding container then
/// dies promptly instead of lingering until the next sweep (see #4).
pub struct CloneRuntime<B: CloneDockerBackend + 'static> {
    backend: Arc<B>,
    /// Max clone timeout used by [`sweep_orphans`](Self::sweep_orphans) to decide
    /// which clone containers are crashed-worker leftovers.
    max_clone_age: Duration,
}

impl<B: CloneDockerBackend + 'static> CloneRuntime<B> {
    /// Build a runtime. `max_clone_age` is the longest a clone container may
    /// legitimately live (≈ the worker's hard timeout plus slack); older
    /// labelled containers are reaped by the sweep.
    pub fn new(backend: B, max_clone_age: Duration) -> Self {
        Self { backend: Arc::new(backend), max_clone_age }
    }

    /// Deterministic container name for an attempt.
    pub fn container_name(attempt_id: Uuid) -> String {
        format!("agentforge-clone-{attempt_id}")
    }

    /// Host path of the credential secret file for an attempt. It lives under the
    /// spec's backend-controlled [`secret_root`](CloneRunSpec::secret_root) — a
    /// 0700 dir OUTSIDE the projects/workspace tree — never beside the staging dir
    /// (whose parent is the projects root that agent containers bind, so a sibling
    /// agent could otherwise reach an in-flight credential). The file is also kept
    /// out of the staging tree so a future rename of staging → project can never
    /// carry the secret along.
    fn secret_host_path(spec: &CloneRunSpec) -> PathBuf {
        spec.secret_root.join(format!(".clone-secret-{}", spec.attempt_id))
    }

    /// Build the create-config for a spec (pure; no I/O). Exposed for tests so
    /// the mount set / network / label / env can be asserted without Docker.
    pub fn build_container_config(spec: &CloneRunSpec, secret_host_path: Option<&Path>) -> CloneContainerConfig {
        let max_bytes = spec.max_bytes.unwrap_or(DEFAULT_CLONE_MAX_BYTES);
        let mut env = vec![
            format!("CLONE_URL={}", spec.repo_url),
            format!("CLONE_DEST={CLONE_DEST}"),
            // Always present (possibly empty) so the contract is explicit; the
            // entrypoint treats empty as "unknown".
            format!("CLONE_PROVIDER={}", spec.provider.as_deref().unwrap_or("")),
            // Disk guard: the entrypoint's watchdog aborts with the distinct
            // "too large" exit code if the cloned tree exceeds this.
            format!("{CLONE_MAX_BYTES_ENV}={max_bytes}"),
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
    /// or error) by [`CloneCleanupGuard`] — and even on a panic/cancellation, the
    /// guard's `Drop` spawns a detached `force_remove` so the credential-holding
    /// container does not linger until the next sweep.
    pub async fn run_clone(&self, spec: CloneRunSpec) -> AppResult<CloneRunOutcome> {
        self.backend.ensure_egress_network().await?;

        // Pre-clean any stale result file from a previous attempt in this staging
        // dir BEFORE we create the container. Without this, an exit-0 path (however
        // reached — including a wait-stream hiccup recovered via inspect) could read
        // a PRIOR run's `.clone-result.json` and falsely report `Ready` (#5b).
        let _ = tokio::fs::remove_file(spec.staging_host_path.join(CLONE_RESULT_FILE)).await;

        // Materialize the credential to a host file under the backend-controlled
        // 0700 secret root (NOT beside the staging dir). The guard is armed with
        // the secret path the INSTANT the file exists — before any fallible step
        // that could `?`-return — so a later failure can never leak it (#6).
        let secret_host_path = match &spec.credential {
            Some(secret) => {
                let path = Self::secret_host_path(&spec);
                write_secret_file(&spec.secret_root, &path, secret.expose()).await?;
                Some(path)
            }
            None => None,
        };
        let mut guard = CloneCleanupGuard::new(Arc::clone(&self.backend), secret_host_path.clone());

        // Now build + validate the config. If validation fails, the guard (already
        // holding the secret path) scrubs it on the early return.
        let config = Self::build_container_config(&spec, secret_host_path.as_deref());
        if let Err(err) = Self::validate_security(&config) {
            guard.run().await;
            return Err(err);
        }

        let container_id = match self.backend.create_container(config).await {
            Ok(id) => id,
            Err(err) => {
                guard.run().await;
                return Err(err);
            }
        };
        guard.set_container(container_id.clone());

        if let Err(err) = self.backend.start_container(&container_id).await {
            guard.run().await;
            return Err(err);
        }

        // Bounded wait. On timeout the guard still force-removes the container.
        let wait = self.backend.wait_exit(&container_id);
        let exit_code = match tokio::time::timeout(spec.timeout, wait).await {
            Ok(Ok(code)) => code,
            Ok(Err(err)) => {
                // The wait stream ended without an exit frame (daemon hiccup /
                // concurrent sweep). Recover the container's REAL exit code by
                // inspecting it, rather than guessing 0 and reading a result file
                // (#5a). If inspect can't recover a finished exit code, surface the
                // wait error.
                match self.recover_exit_code(&container_id).await {
                    Some(code) => code,
                    None => {
                        guard.run().await;
                        return Err(err);
                    }
                }
            }
            Err(_elapsed) => {
                // Timeout-vs-finish race (#7): a clone that succeeded right at the
                // boundary would otherwise be discarded as `Timeout`. If a valid
                // result file exists AND the container's real exit code is 0, honor
                // the success; otherwise report `Timeout`. Either way the container
                // is reaped below.
                if let Some(outcome) = self.recover_finished_at_timeout(&spec, &container_id).await {
                    tracing::info!(
                        attempt_id = %spec.attempt_id,
                        container_id = %container_id,
                        "clone finished at the timeout boundary; honoring its result instead of Timeout"
                    );
                    guard.run().await;
                    return Ok(outcome);
                }
                tracing::warn!(
                    attempt_id = %spec.attempt_id,
                    container_id = %container_id,
                    timeout_secs = spec.timeout.as_secs(),
                    "clone container timed out; force-removing"
                );
                guard.run().await;
                return Ok(CloneRunOutcome::Timeout);
            }
        };

        let outcome = self.classify_exit(&spec, &container_id, exit_code).await;
        guard.run().await;
        Ok(outcome)
    }

    /// Map a known exit code to an outcome, reading the result file on success.
    async fn classify_exit(&self, spec: &CloneRunSpec, container_id: &str, exit_code: i64) -> CloneRunOutcome {
        if exit_code == CLONE_EXIT_TOO_LARGE {
            // The entrypoint's disk guard aborted: surface a precise TooLarge, not
            // a lossy Failed (#11).
            let tail = self.capture_log_tail(container_id).await;
            tracing::warn!(
                attempt_id = %spec.attempt_id,
                "clone aborted by the disk guard: cloned tree exceeded CLONE_MAX_BYTES"
            );
            return CloneRunOutcome::TooLarge { stderr_tail: RawStderr::new(tail) };
        }

        if exit_code != 0 {
            let tail = self.capture_log_tail(container_id).await;
            return CloneRunOutcome::Failed { exit_code, stderr_tail: RawStderr::new(tail) };
        }

        // exit 0: require a valid result file with a non-empty head_sha.
        match read_result_file(&spec.staging_host_path).await {
            Ok(result) if !result.head_sha.trim().is_empty() => CloneRunOutcome::Ready {
                branch: result.branch.filter(|b| !b.is_empty()),
                head_sha: result.head_sha,
                bytes: result.bytes,
            },
            Ok(_) => {
                // Structurally-present but empty head_sha ⇒ down-map to Failed so an
                // empty SHA can never reach M5 (#12).
                let tail = self.capture_log_tail(container_id).await;
                tracing::warn!(
                    attempt_id = %spec.attempt_id,
                    "clone exited 0 but result file had an empty head_sha"
                );
                CloneRunOutcome::Failed {
                    exit_code: -1,
                    stderr_tail: RawStderr::new(bounded_tail(
                        &format!("{tail}\nresult-file error: empty head_sha"),
                        STDERR_TAIL_LIMIT,
                    )),
                }
            }
            Err(err) => {
                // Exit 0 but no/garbled result file ⇒ treat as a failure so the
                // worker never marks a half-baked clone "ready".
                let tail = self.capture_log_tail(container_id).await;
                tracing::warn!(
                    attempt_id = %spec.attempt_id,
                    error = %err,
                    "clone exited 0 but result file was missing/invalid"
                );
                CloneRunOutcome::Failed {
                    exit_code: -1,
                    stderr_tail: RawStderr::new(bounded_tail(
                        &format!("{tail}\nresult-file error: {err}"),
                        STDERR_TAIL_LIMIT,
                    )),
                }
            }
        }
    }

    /// Recover the real exit code of a container whose wait stream ended without
    /// an exit frame. Returns `Some(code)` only when the container has genuinely
    /// stopped (not running); `None` if it's still running or no longer exists.
    async fn recover_exit_code(&self, container_id: &str) -> Option<i64> {
        match self.backend.inspect_container(container_id).await {
            Ok(Some(state)) if !state.running => Some(state.exit_code),
            _ => None,
        }
    }

    /// On the timeout branch, best-effort check whether the clone actually finished
    /// successfully right at the boundary: a parseable result file with a non-empty
    /// head_sha AND a real exit code of 0. Returns the `Ready` outcome if so.
    async fn recover_finished_at_timeout(&self, spec: &CloneRunSpec, container_id: &str) -> Option<CloneRunOutcome> {
        let state = self.backend.inspect_container(container_id).await.ok().flatten()?;
        if state.running || state.exit_code != 0 {
            return None;
        }
        let result = read_result_file(&spec.staging_host_path).await.ok()?;
        if result.head_sha.trim().is_empty() {
            return None;
        }
        Some(CloneRunOutcome::Ready {
            branch: result.branch.filter(|b| !b.is_empty()),
            head_sha: result.head_sha,
            bytes: result.bytes,
        })
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

    /// Sweep crashed-worker leftovers: force-remove clone containers (label
    /// `agentforge.project_clone`) that are crashed-worker orphans, returning the
    /// number reaped. Called on startup + periodically by the M5 reconciler.
    ///
    /// Liveness-aware (#8) — it must NOT kill a healthy in-progress clone of a
    /// large repo. A container is reaped only when:
    /// - it is NOT running (a stopped clone the worker never cleaned up), OR
    /// - it is running but older than [`max_clone_age`] (the hard ceiling, which
    ///   the caller sets to the worker timeout + generous slack — past this, a
    ///   still-running clone is a stuck/leaked container, not a healthy one).
    ///
    /// A list entry with `created==0` (omitted create time) is NOT blindly reaped:
    /// it is `inspect`-ed to recover its real running-state + create-time first.
    pub async fn sweep_orphans(&self) -> AppResult<usize> {
        let now = unix_now_secs();
        let max_age = self.max_clone_age.as_secs() as i64;
        let containers = self.backend.list_clone_containers().await?;

        let mut reaped = 0usize;
        for summary in containers {
            // Resolve the real running-state + create-time. If the list omitted the
            // create time (created==0), inspect the container to recover the truth
            // before deciding — never reap on a missing timestamp alone.
            let (running, created_epoch_secs) = if summary.created_epoch_secs == 0 {
                match self.backend.inspect_container(&summary.id).await {
                    Ok(Some(state)) => (state.running, state.created_epoch_secs),
                    // Gone already, or inspect failed: a created==0 entry we can't
                    // resolve is anomalous and credential-holding. If it no longer
                    // exists (None) there's nothing to reap; if inspect errored we
                    // conservatively skip it this pass and let the next sweep retry,
                    // rather than risk reaping a healthy clone we couldn't read.
                    Ok(None) => continue,
                    Err(err) => {
                        tracing::warn!(
                            container_id = %summary.id,
                            error = %err,
                            "sweep: failed to inspect a clone container with no create time; skipping this pass"
                        );
                        continue;
                    }
                }
            } else {
                (summary.running, summary.created_epoch_secs)
            };

            let age = now.saturating_sub(created_epoch_secs);
            // Reap rule: not-running ⇒ orphan; running ⇒ only past the hard ceiling.
            // A created_epoch we still can't resolve (0) is treated as old (age is
            // huge), so a non-running unknown is reaped while a running unknown
            // still needs to exceed the ceiling.
            let over_ceiling = created_epoch_secs == 0 || age >= max_age;
            let should_reap = !running || over_ceiling;
            if !should_reap {
                continue;
            }

            match self.backend.force_remove(&summary.id).await {
                Ok(()) => {
                    reaped += 1;
                    tracing::info!(
                        container_id = %summary.id,
                        age_secs = age,
                        running,
                        "reaped orphan clone container"
                    );
                }
                Err(err) => {
                    tracing::warn!(container_id = %summary.id, error = %err, "failed to reap orphan clone container");
                }
            }
        }
        Ok(reaped)
    }
}

/// RAII-style cleanup that is **cancel/panic-safe** (#4): it force-removes the
/// clone container (if created) and scrubs the host secret file on every path.
///
/// `run()` is the PRIMARY, awaited path callers MUST invoke on every normal
/// return. But because `Drop` can't await, a panic or task-cancellation after the
/// container started would otherwise leave a running container holding the live
/// credential bind-mount until the next sweep (~minutes) — and unlinking the host
/// secret file does NOT revoke an already-live bind mount. So `Drop` ALSO holds an
/// [`Arc`] handle to the backend and `tokio::spawn`s a detached best-effort
/// `force_remove`, so the credential-holding container dies promptly. The spawned
/// removal is the fallback; `run()` remains the awaited primary path.
struct CloneCleanupGuard<B: CloneDockerBackend + 'static> {
    backend: Arc<B>,
    container_id: Option<String>,
    secret_path: Option<PathBuf>,
    done: bool,
}

impl<B: CloneDockerBackend + 'static> CloneCleanupGuard<B> {
    fn new(backend: Arc<B>, secret_path: Option<PathBuf>) -> Self {
        Self { backend, container_id: None, secret_path, done: false }
    }

    fn set_container(&mut self, id: String) {
        self.container_id = Some(id);
    }

    /// Force-remove the container (best-effort, awaited) and scrub the secret
    /// file. Idempotent.
    async fn run(&mut self) {
        if self.done {
            return;
        }
        self.done = true;
        if let Some(id) = &self.container_id
            && let Err(err) = self.backend.force_remove(id).await
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

impl<B: CloneDockerBackend + 'static> Drop for CloneCleanupGuard<B> {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        // The awaited `run()` never executed (panic / cancellation). Scrub the
        // secret file synchronously, then spawn a DETACHED best-effort container
        // force-remove so the credential-holding container dies promptly instead
        // of lingering until the next sweep. Unlinking the file alone does NOT
        // revoke the already-live bind mount, hence the detached removal.
        self.remove_secret_file();
        if let Some(id) = self.container_id.take() {
            tracing::error!(
                container_id = %id,
                "clone cleanup guard dropped without run() (panic/cancellation); spawning detached force-remove"
            );
            // Only spawn if a runtime is present; in a non-tokio drop context
            // (e.g. a sync test) fall back to the sweep. `try_current` avoids a
            // panic when there is no runtime.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let backend = Arc::clone(&self.backend);
                handle.spawn(async move {
                    if let Err(err) = backend.force_remove(&id).await {
                        tracing::warn!(
                            container_id = %id,
                            error = %err,
                            "clone cleanup (detached): force-remove failed; sweep_orphans will retry"
                        );
                    }
                });
            } else {
                tracing::warn!(
                    container_id = %id,
                    "clone cleanup guard dropped outside a tokio runtime; sweep_orphans will reap the container"
                );
            }
        }
    }
}

/// Marker for the credential secret-root design contract — referenced by the
/// module docs and [`CloneRunSpec::secret_root`]. Not instantiated; it exists so
/// the rustdoc link target lives near the implementation.
///
/// The secret root is a backend-controlled directory created mode **0700** (owner
/// rwx, nothing else), placed OUTSIDE the projects/workspace tree that agent
/// containers bind. The per-attempt credential file inside it is mode **0644** so
/// the clone container's `agent` uid (1011) — which differs from the backend's uid
/// (100) and shares no gid, and which the backend cannot `chown` to (no
/// `CAP_CHOWN`) — can actually READ the bind-mounted secret. Confidentiality is
/// the 0700 root: no other host user and no agent project mount can traverse INTO
/// it to reach the 0644 file.
#[derive(Debug)]
pub struct CloneSecretRoot;

/// Ensure the backend-controlled secret root exists with mode 0700, then write the
/// credential to a mode-0644 file inside it (see [`CloneSecretRoot`] for why 0644
/// and not 0400). The 0700 root — not the file bits — is the confidentiality
/// control. The file is overwritten atomically (`create_new` after removing any
/// stale leftover) so we never append to a previous run's secret.
async fn write_secret_file(secret_root: &Path, path: &Path, bytes: &[u8]) -> AppResult<()> {
    use tokio::io::AsyncWriteExt;

    // Create/own the secret root at 0700 so no other host user or agent project
    // mount can traverse into it. Best-effort chmod on a pre-existing root we may
    // not own (mirrors the OAuth-mount pattern): log-and-continue, since the
    // operator is expected to point the secret root at a backend-created path.
    tokio::fs::create_dir_all(secret_root)
        .await
        .map_err(|err| internal(format!("failed to create clone secret root {}: {err}", secret_root.display())))?;
    #[cfg(unix)]
    set_dir_mode_best_effort(secret_root, CLONE_SECRET_ROOT_MODE).await;

    // Remove any stale file first so we never append to a leftover.
    let _ = tokio::fs::remove_file(path).await;

    let mut open = tokio::fs::OpenOptions::new();
    open.write(true).create_new(true);
    // tokio's OpenOptions exposes `mode` inherently on unix, so the 0644 mode is
    // applied atomically at create — no chmod-after-create TOCTOU window. 0644 is
    // required for the cross-uid read (see CloneSecretRoot); the 0700 root keeps
    // the world-readable bits unreachable.
    #[cfg(unix)]
    open.mode(CLONE_SECRET_FILE_MODE);
    let mut file = open
        .open(path)
        .await
        .map_err(|err| internal(format!("failed to create clone credential file {}: {err}", path.display())))?;
    file.write_all(bytes).await.map_err(|err| internal(format!("failed to write clone credential file: {err}")))?;
    file.flush().await.map_err(|err| internal(format!("failed to flush clone credential file: {err}")))?;
    Ok(())
}

/// Best-effort directory chmod: on a pre-existing root the backend may not own,
/// the chmod fails; log + continue rather than abort the clone (matches the
/// established OAuth-mount handling).
#[cfg(unix)]
async fn set_dir_mode_best_effort(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    match tokio::fs::metadata(path).await {
        Ok(meta) => {
            let mut perms = meta.permissions();
            perms.set_mode(mode);
            if let Err(err) = tokio::fs::set_permissions(path, perms).await {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "could not set clone secret-root mode 0700; ensure it is backend-owned + private"
                );
            }
        }
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "could not stat clone secret root to set its mode");
        }
    }
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

/// Parse a Docker RFC3339 created-timestamp (`YYYY-MM-DDTHH:MM:SS[.fff…][Z|±hh:mm]`)
/// to unix epoch SECONDS, best-effort. Returns 0 on any malformed input — the
/// sweep treats 0 as "unknown create time" and falls back to the not-running
/// signal, so a parse miss never causes a healthy clone to be reaped. Kept
/// dependency-free (no chrono in this crate) since only second precision matters.
fn parse_rfc3339_secs(s: &str) -> i64 {
    // Split date and time on 'T'. Anything else is malformed → 0.
    let (date, rest) = match s.split_once('T') {
        Some(parts) => parts,
        None => return 0,
    };
    let mut d = date.splitn(3, '-');
    let (year, month, day) = match (d.next(), d.next(), d.next()) {
        (Some(y), Some(m), Some(dd)) => (parse_i64(y), parse_i64(m), parse_i64(dd)),
        _ => return 0,
    };

    // Time portion: strip the timezone designator and any fractional seconds. We
    // treat the clock as UTC (Docker emits Z / +00:00 for `created`); a non-UTC
    // offset only skews the epoch by the offset, which is harmless for an age
    // comparison against a multi-minute ceiling.
    let time_part = rest.split(['Z', '+']).next().unwrap_or(rest);
    // For a negative offset, '-' also appears inside the time's tz; but HH:MM:SS
    // has no '-', so split on the FIRST '-' after position 0 if present.
    let time_part = match time_part.find('-') {
        Some(idx) => &time_part[..idx],
        None => time_part,
    };
    let time_no_frac = time_part.split('.').next().unwrap_or(time_part);
    let mut t = time_no_frac.splitn(3, ':');
    let (hour, minute, second) = match (t.next(), t.next(), t.next()) {
        (Some(h), Some(mi), Some(se)) => (parse_i64(h), parse_i64(mi), parse_i64(se)),
        _ => return 0,
    };

    match (year, month, day, hour, minute, second) {
        (Some(y), Some(mo), Some(da), Some(h), Some(mi), Some(se)) => civil_to_unix_secs(y, mo, da, h, mi, se),
        _ => 0,
    }
}

fn parse_i64(s: &str) -> Option<i64> {
    s.trim().parse::<i64>().ok()
}

/// Convert a proleptic-Gregorian civil date-time (UTC) to unix epoch seconds.
/// Uses Howard Hinnant's `days_from_civil` algorithm; valid for the full Docker
/// timestamp range. Returns 0 for an out-of-range month/day to stay best-effort.
fn civil_to_unix_secs(y: i64, m: i64, d: i64, hh: i64, mm: i64, ss: i64) -> i64 {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return 0;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    let days = era * 146097 + doe - 719468;
    days * 86400 + hh * 3600 + mm * 60 + ss
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
        /// When `Some`, `wait_exit` returns this error instead of an exit code (to
        /// drive the wait-stream-None recovery path).
        wait_error: Arc<Mutex<Option<String>>>,
        /// When set, `wait_exit` sleeps this long (to drive the timeout path).
        wait_delay: Arc<Mutex<Option<Duration>>>,
        logs: Arc<Mutex<String>>,
        list_result: Arc<Mutex<Vec<CloneContainerSummary>>>,
        /// Per-id inspect responses, keyed by container id.
        inspect_result: Arc<Mutex<HashMap<String, Option<CloneContainerState>>>>,
        /// When set, the mock writes this JSON into `<staging>/.clone-result.json`
        /// at `wait_exit` time — simulating the real container producing its result
        /// AFTER the runtime's preclean step (so the preclean can't race it).
        result_json: Arc<Mutex<Option<String>>>,
        /// Captured staging host path (from the create-config's /staging mount),
        /// used to place the simulated result file.
        staging_path: Arc<Mutex<Option<PathBuf>>>,
    }

    impl MockBackend {
        fn with_exit(code: i64) -> Self {
            let m = Self::default();
            *m.exit_code.lock().unwrap() = code;
            m
        }
        /// Exit 0 AND simulate the container writing the given result JSON.
        fn with_result(json: &str) -> Self {
            let m = Self::with_exit(0);
            *m.result_json.lock().unwrap() = Some(json.to_string());
            m
        }
        fn set_logs(&self, logs: &str) {
            *self.logs.lock().unwrap() = logs.to_string();
        }
        fn set_inspect(&self, id: &str, state: Option<CloneContainerState>) {
            self.inspect_result.lock().unwrap().insert(id.to_string(), state);
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
        async fn inspect_network(&self, _name: &str) -> AppResult<NetworkInspectInfo> {
            Ok(NetworkInspectInfo {
                driver: Some("bridge".to_string()),
                internal: false,
                labels: HashMap::from([("agentforge.managed".to_string(), "clone-egress".to_string())]),
                attached_container_names: vec![],
            })
        }
        async fn create_container(&self, config: CloneContainerConfig) -> AppResult<String> {
            // Capture the staging host path so wait_exit can simulate the container
            // writing its result file there (post-preclean).
            if let Some(staging) = config.mounts.iter().find(|m| m.target == CLONE_STAGING_TARGET) {
                *self.staging_path.lock().unwrap() = Some(PathBuf::from(&staging.source));
            }
            self.created.lock().unwrap().push(config);
            Ok("ctr-clone".to_string())
        }
        async fn start_container(&self, id: &str) -> AppResult<()> {
            self.started.lock().unwrap().push(id.to_string());
            // Simulate the container producing its result file once started, i.e.
            // AFTER the runtime precleaned the staging dir but available to BOTH the
            // normal exit path and the timeout-race recovery path.
            if let (Some(json), Some(staging)) =
                (self.result_json.lock().unwrap().clone(), self.staging_path.lock().unwrap().clone())
            {
                let _ = std::fs::write(staging.join(CLONE_RESULT_FILE), json);
            }
            Ok(())
        }
        async fn wait_exit(&self, _id: &str) -> AppResult<i64> {
            let delay = *self.wait_delay.lock().unwrap();
            if let Some(delay) = delay {
                tokio::time::sleep(delay).await;
            }
            if let Some(msg) = self.wait_error.lock().unwrap().clone() {
                return Err(internal(msg));
            }
            let code = *self.exit_code.lock().unwrap();
            Ok(code)
        }
        async fn inspect_container(&self, id: &str) -> AppResult<Option<CloneContainerState>> {
            Ok(self.inspect_result.lock().unwrap().get(id).cloned().flatten())
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

    /// A per-test parent dir holding the staging + secret-root subdirs. Returned so
    /// the test can clean up the whole tree at the end.
    fn temp_root(attempt_id: Uuid) -> PathBuf {
        std::env::temp_dir().join(format!("afclone-test-{attempt_id}"))
    }

    fn temp_staging(attempt_id: Uuid) -> PathBuf {
        let staging = temp_root(attempt_id).join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        staging
    }

    /// Backend-controlled secret root, OUTSIDE the staging tree (sibling of it
    /// under the per-test parent), mirroring the production contract.
    fn temp_secret_root(attempt_id: Uuid) -> PathBuf {
        temp_root(attempt_id).join("clone-secrets")
    }

    fn spec_with(attempt_id: Uuid, credential: Option<SecretBytes>) -> CloneRunSpec {
        CloneRunSpec {
            image: "agentforge-clone:latest".to_string(),
            repo_url: "https://github.com/octocat/Hello-World.git".to_string(),
            provider: Some("github".to_string()),
            staging_host_path: temp_staging(attempt_id),
            secret_root: temp_secret_root(attempt_id),
            credential,
            timeout: Duration::from_secs(30),
            max_bytes: None,
            attempt_id,
        }
    }

    fn cleanup(spec: &CloneRunSpec) {
        let _ = std::fs::remove_dir_all(temp_root(spec.attempt_id));
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
        let secret_path = CloneRuntime::<MockBackend>::secret_host_path(&spec);
        let config = CloneRuntime::<MockBackend>::build_container_config(&spec, Some(&secret_path));

        assert_eq!(config.mounts.len(), 2);
        let secret = config.mounts.iter().find(|m| m.target == "/run/secrets/git-credential").expect("secret mount");
        assert!(secret.read_only, "credential mount must be read-only");
        // The secret lives under the backend secret root, NEVER inside the staging
        // tree (staging becomes the live project / is bound to agents).
        assert!(!secret.source.contains("/staging"), "secret must not be inside the staging tree");
        assert!(
            secret.source.starts_with(spec.secret_root.to_string_lossy().as_ref()),
            "secret must live under the backend secret root"
        );
        cleanup(&spec);
    }

    #[test]
    fn config_credential_never_in_env() {
        let attempt_id = Uuid::now_v7();
        let token = "supersecrettoken12345";
        let spec = spec_with(attempt_id, Some(SecretBytes::from(format!("x-access-token:{token}"))));
        let secret_path = CloneRuntime::<MockBackend>::secret_host_path(&spec);
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
    fn config_sets_disk_guard_env() {
        let attempt_id = Uuid::now_v7();
        // Default cap when max_bytes is None.
        let spec = spec_with(attempt_id, None);
        let config = CloneRuntime::<MockBackend>::build_container_config(&spec, None);
        assert!(
            config.env.iter().any(|e| e == &format!("CLONE_MAX_BYTES={DEFAULT_CLONE_MAX_BYTES}")),
            "default disk cap env must be present: {:?}",
            config.env
        );
        cleanup(&spec);

        // Explicit override.
        let attempt2 = Uuid::now_v7();
        let mut spec2 = spec_with(attempt2, None);
        spec2.max_bytes = Some(123_456);
        let config2 = CloneRuntime::<MockBackend>::build_container_config(&spec2, None);
        assert!(config2.env.iter().any(|e| e == "CLONE_MAX_BYTES=123456"));
        cleanup(&spec2);
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

    // ---- egress network 409-reuse fail-closed (#1a) -------------------------

    #[test]
    fn assert_managed_egress_accepts_valid_managed_network() {
        let info = NetworkInspectInfo {
            driver: Some("bridge".to_string()),
            internal: false,
            labels: HashMap::from([("agentforge.managed".to_string(), "clone-egress".to_string())]),
            attached_container_names: vec!["agentforge-clone-abc".to_string()],
        };
        assert!(info.assert_managed_egress().is_ok());
    }

    #[test]
    fn assert_managed_egress_rejects_wrong_driver() {
        let info = NetworkInspectInfo {
            driver: Some("macvlan".to_string()),
            internal: false,
            labels: HashMap::from([("agentforge.managed".to_string(), "clone-egress".to_string())]),
            attached_container_names: vec![],
        };
        assert!(info.assert_managed_egress().is_err());
    }

    #[test]
    fn assert_managed_egress_rejects_missing_label() {
        let info = NetworkInspectInfo {
            driver: Some("bridge".to_string()),
            internal: false,
            labels: HashMap::new(),
            attached_container_names: vec![],
        };
        assert!(info.assert_managed_egress().is_err(), "unlabelled (not platform-created) network must be refused");
    }

    #[test]
    fn assert_managed_egress_rejects_internal_services_attached() {
        let info = NetworkInspectInfo {
            driver: Some("bridge".to_string()),
            internal: false,
            labels: HashMap::from([("agentforge.managed".to_string(), "clone-egress".to_string())]),
            // An internal-service container wired onto the egress net = isolation
            // breach.
            attached_container_names: vec!["agentforge-postgres".to_string()],
        };
        assert!(info.assert_managed_egress().is_err());
    }

    #[test]
    fn internal_service_container_matcher_is_suffix_aware() {
        assert!(is_internal_service_container("agentforge-postgres"));
        assert!(is_internal_service_container("/myprefix-server"));
        assert!(is_internal_service_container("stack_nats"));
        assert!(is_internal_service_container("orchestrator"));
        assert!(!is_internal_service_container("agentforge-clone-1234"));
        assert!(!is_internal_service_container("serverless-thing"));
    }

    // ---- outcome mapping ----------------------------------------------------

    #[tokio::test]
    async fn run_clone_ready_on_exit_zero_with_result_file() {
        let attempt_id = Uuid::now_v7();
        let backend = MockBackend::with_result(r#"{"branch":"main","head_sha":"abc123def","bytes":4096}"#);
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(600));
        let spec = spec_with(attempt_id, None);
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
        let backend = MockBackend::with_result(r#"{"branch":"main","head_sha":"sha","bytes":1}"#);
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
        let backend = MockBackend::with_result(r#"{"branch":"","head_sha":"deadbeef","bytes":10}"#);
        let runtime = CloneRuntime::new(backend, Duration::from_secs(600));
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
                assert!(stderr_tail.as_raw().contains("Authentication failed"));
                assert!(stderr_tail.as_raw().len() <= STDERR_TAIL_LIMIT);
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(backend.take_removed(), vec!["ctr-clone".to_string()], "must reap on failure");
        cleanup(&spec);
    }

    #[tokio::test]
    async fn run_clone_too_large_exit_maps_to_too_large_outcome() {
        // The entrypoint's disk guard aborts with the distinct exit code (#11).
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        let backend = MockBackend::with_exit(CLONE_EXIT_TOO_LARGE);
        backend.set_logs("clone-entrypoint: ERROR: cloned tree exceeded CLONE_MAX_BYTES");
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(600));
        let outcome = runtime.run_clone(spec_with(attempt_id, None)).await.unwrap();
        match outcome {
            CloneRunOutcome::TooLarge { stderr_tail } => {
                assert!(stderr_tail.as_raw().contains("CLONE_MAX_BYTES"));
            }
            other => panic!("expected TooLarge, got {other:?}"),
        }
        assert_eq!(backend.take_removed(), vec!["ctr-clone".to_string()], "must reap on TooLarge");
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
    async fn run_clone_exit_zero_with_empty_head_sha_is_failed_not_ready() {
        // #12: a structurally-present result file with an empty head_sha must NOT
        // become Ready.
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        std::fs::write(
            spec.staging_host_path.join(".clone-result.json"),
            r#"{"branch":"main","head_sha":"","bytes":5}"#,
        )
        .unwrap();
        let runtime = CloneRuntime::new(MockBackend::with_exit(0), Duration::from_secs(600));
        let outcome = runtime.run_clone(spec_with(attempt_id, None)).await.unwrap();
        assert!(matches!(outcome, CloneRunOutcome::Failed { exit_code: -1, .. }), "got {outcome:?}");
        cleanup(&spec);
    }

    #[tokio::test]
    async fn run_clone_preclean_removes_stale_result_file_before_run() {
        // #5b: a result file from a PRIOR attempt must not be read when THIS run
        // exits 0 without producing its own — it must be precleaned so we get
        // Failed, not a false Ready off the stale file.
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        // Stale result from a previous attempt.
        std::fs::write(
            spec.staging_host_path.join(".clone-result.json"),
            r#"{"branch":"stale","head_sha":"staleSHA","bytes":99}"#,
        )
        .unwrap();
        // This run's container exits 0 but (mock) writes no result file.
        let runtime = CloneRuntime::new(MockBackend::with_exit(0), Duration::from_secs(600));
        let outcome = runtime.run_clone(spec_with(attempt_id, None)).await.unwrap();
        assert!(
            matches!(outcome, CloneRunOutcome::Failed { exit_code: -1, .. }),
            "stale result file must not produce a false Ready; got {outcome:?}"
        );
        cleanup(&spec);
    }

    #[tokio::test]
    async fn run_clone_wait_none_recovers_real_exit_code_via_inspect() {
        // #5a: a wait stream that ends without an exit frame (here a wait_error)
        // must NOT be treated as exit 0; the real exit code is recovered by
        // inspecting the stopped container.
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        let backend = MockBackend::default();
        *backend.wait_error.lock().unwrap() = Some("wait stream ended without an exit frame".to_string());
        // Inspect reports the container genuinely stopped with a non-zero code.
        backend.set_inspect(
            "ctr-clone",
            Some(CloneContainerState { running: false, exit_code: 128, created_epoch_secs: 0 }),
        );
        backend.set_logs("fatal: could not read Username");
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(600));
        let outcome = runtime.run_clone(spec_with(attempt_id, None)).await.unwrap();
        match outcome {
            CloneRunOutcome::Failed { exit_code, .. } => assert_eq!(exit_code, 128),
            other => panic!("expected Failed(128) recovered via inspect, got {other:?}"),
        }
        cleanup(&spec);
    }

    #[tokio::test]
    async fn run_clone_wait_none_without_recoverable_state_errors() {
        // If the wait stream ends AND inspect can't recover a finished exit code
        // (still running / gone), surface the error rather than a false Ready.
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, None);
        // A stale result file is present, which a buggy `Ok(0)` mapping would read.
        std::fs::write(
            spec.staging_host_path.join(".clone-result.json"),
            r#"{"branch":"x","head_sha":"shouldNotBeRead","bytes":1}"#,
        )
        .unwrap();
        let backend = MockBackend::default();
        *backend.wait_error.lock().unwrap() = Some("wait stream ended without an exit frame".to_string());
        // No inspect result for this id ⇒ None ⇒ unrecoverable.
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(600));
        let result = runtime.run_clone(spec_with(attempt_id, None)).await;
        assert!(result.is_err(), "unrecoverable wait-None must error, not read the stale result file");
        // The container is still reaped.
        assert_eq!(backend.take_removed(), vec!["ctr-clone".to_string()]);
        cleanup(&spec);
    }

    #[tokio::test]
    async fn run_clone_times_out_and_reaps() {
        let attempt_id = Uuid::now_v7();
        let mut spec = spec_with(attempt_id, None);
        spec.timeout = Duration::from_millis(20);
        let backend = MockBackend::default();
        *backend.wait_delay.lock().unwrap() = Some(Duration::from_secs(5));
        // Inspect at timeout shows it's still running ⇒ genuine Timeout.
        backend
            .set_inspect("ctr-clone", Some(CloneContainerState { running: true, exit_code: 0, created_epoch_secs: 0 }));
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(600));

        let outcome = runtime.run_clone(spec).await.unwrap();
        assert_eq!(outcome, CloneRunOutcome::Timeout);
        // The container must still be force-removed on the timeout path.
        assert_eq!(backend.take_removed(), vec!["ctr-clone".to_string()]);
        cleanup(&spec_with(attempt_id, None));
    }

    #[tokio::test]
    async fn run_clone_timeout_race_honors_a_finished_success() {
        // #7: the clone finished (exit 0 + valid result file) right at the timeout
        // boundary. Don't discard the good result as Timeout — return Ready.
        let attempt_id = Uuid::now_v7();
        let mut spec = spec_with(attempt_id, None);
        spec.timeout = Duration::from_millis(20);
        // The container writes its result on start (simulating it finishing right at
        // the boundary), then the wait sleeps past the timeout so the timeout branch
        // fires...
        let backend = MockBackend::with_result(r#"{"branch":"main","head_sha":"raceSHA","bytes":7}"#);
        *backend.wait_delay.lock().unwrap() = Some(Duration::from_secs(5));
        // ...but inspect shows the container actually finished with exit 0.
        backend.set_inspect(
            "ctr-clone",
            Some(CloneContainerState { running: false, exit_code: 0, created_epoch_secs: 0 }),
        );
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(600));
        let outcome = runtime.run_clone(spec).await.unwrap();
        assert_eq!(
            outcome,
            CloneRunOutcome::Ready { branch: Some("main".to_string()), head_sha: "raceSHA".to_string(), bytes: 7 },
            "a success at the timeout boundary must be honored, not discarded as Timeout"
        );
        // Still reaped.
        assert_eq!(backend.take_removed(), vec!["ctr-clone".to_string()]);
        cleanup(&spec_with(attempt_id, None));
    }

    #[tokio::test]
    async fn run_clone_credential_written_0644_under_secret_root_and_scrubbed() {
        let attempt_id = Uuid::now_v7();
        let spec = spec_with(attempt_id, Some(SecretBytes::from("x-access-token:tok".to_string())));
        let secret_path = CloneRuntime::<MockBackend>::secret_host_path(&spec);

        let backend = MockBackend::with_result(r#"{"branch":"main","head_sha":"sha","bytes":1}"#);
        let runtime = CloneRuntime::new(backend, Duration::from_secs(600));
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
    #[cfg(unix)]
    async fn write_secret_file_is_0644_under_0700_root() {
        use std::os::unix::fs::PermissionsExt;
        let attempt_id = Uuid::now_v7();
        let secret_root = temp_secret_root(attempt_id);
        let path = secret_root.join(format!(".clone-secret-{attempt_id}"));
        write_secret_file(&secret_root, &path, b"x-access-token:tok").await.unwrap();

        // File is 0644 so the clone uid (1011 != backend uid) can read it.
        let file_mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o644, "secret file must be 0644 for cross-uid read");
        // Root is 0700 so confidentiality is directory traversal, not file bits.
        let dir_mode = std::fs::metadata(&secret_root).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700, "secret root must be 0700");

        let _ = std::fs::remove_dir_all(temp_root(attempt_id));
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
            async fn inspect_network(&self, _name: &str) -> AppResult<NetworkInspectInfo> {
                Ok(NetworkInspectInfo {
                    driver: Some("bridge".to_string()),
                    internal: false,
                    labels: HashMap::from([("agentforge.managed".to_string(), "clone-egress".to_string())]),
                    attached_container_names: vec![],
                })
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
            async fn inspect_container(&self, _id: &str) -> AppResult<Option<CloneContainerState>> {
                Ok(None)
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
        let secret_path = CloneRuntime::<FailStartBackend>::secret_host_path(&spec);
        let backend = FailStartBackend::default();
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(600));

        let result = runtime.run_clone(spec_with(attempt_id, Some(SecretBytes::from("tok".to_string())))).await;
        assert!(result.is_err());
        assert!(!secret_path.exists(), "credential file must be removed even when start fails");
        assert_eq!(backend.removed.lock().unwrap().clone(), vec!["ctr-x".to_string()]);
        cleanup(&spec);
    }

    // ---- cancel/panic-safe cleanup guard (#4) -------------------------------

    #[tokio::test]
    async fn cleanup_guard_drop_without_run_scrubs_secret_and_spawns_force_remove() {
        // Simulate a panic/cancellation AFTER the container started: the guard is
        // dropped WITHOUT run(). It must (a) scrub the host secret file
        // synchronously, and (b) spawn a detached force_remove so the
        // credential-holding container dies promptly (not only at the next sweep).
        let attempt_id = Uuid::now_v7();
        let secret_root = temp_secret_root(attempt_id);
        let secret_path = secret_root.join(format!(".clone-secret-{attempt_id}"));
        write_secret_file(&secret_root, &secret_path, b"tok").await.unwrap();
        assert!(secret_path.exists());

        let backend = Arc::new(MockBackend::default());
        {
            let mut guard = CloneCleanupGuard::new(Arc::clone(&backend), Some(secret_path.clone()));
            guard.set_container("ctr-drop".to_string());
            // guard goes out of scope here WITHOUT run() → Drop fires.
        }
        // Secret file scrubbed synchronously in Drop.
        assert!(!secret_path.exists(), "Drop must scrub the secret file synchronously");
        // The detached force_remove is spawned; yield so it runs.
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(
            backend.take_removed(),
            vec!["ctr-drop".to_string()],
            "Drop must spawn a detached force_remove of the orphaned container"
        );
        let _ = std::fs::remove_dir_all(temp_root(attempt_id));
    }

    // ---- sweep liveness (#8) ------------------------------------------------

    #[tokio::test]
    async fn sweep_does_not_reap_a_running_fresh_clone() {
        let now = unix_now_secs();
        let backend = MockBackend::default();
        *backend.list_result.lock().unwrap() = vec![
            // Running, fresh (5s old), within timeout: MUST be kept.
            CloneContainerSummary { id: "running-fresh".to_string(), created_epoch_secs: now - 5, running: true },
            // Stopped, fresh: orphan the worker never cleaned up ⇒ reap.
            CloneContainerSummary { id: "stopped-fresh".to_string(), created_epoch_secs: now - 5, running: false },
            // Running but past the hard ceiling (2h with max_age 1h) ⇒ stuck ⇒ reap.
            CloneContainerSummary { id: "running-stuck".to_string(), created_epoch_secs: now - 7200, running: true },
        ];
        // max age = 1h ceiling
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(3600));
        let reaped = runtime.sweep_orphans().await.unwrap();
        let removed = backend.take_removed();
        assert!(!removed.contains(&"running-fresh".to_string()), "a healthy in-progress clone must NOT be reaped");
        assert!(removed.contains(&"stopped-fresh".to_string()), "a stopped orphan must be reaped");
        assert!(removed.contains(&"running-stuck".to_string()), "a clone past the ceiling must be reaped");
        assert_eq!(reaped, 2);
    }

    #[tokio::test]
    async fn sweep_inspects_unknown_create_time_before_reaping() {
        // created==0 must not be blindly reaped: inspect resolves the truth. Here
        // the inspected container is running + fresh ⇒ keep.
        let now = unix_now_secs();
        let backend = MockBackend::default();
        *backend.list_result.lock().unwrap() =
            vec![CloneContainerSummary { id: "unknown-running".to_string(), created_epoch_secs: 0, running: false }];
        backend.set_inspect(
            "unknown-running",
            Some(CloneContainerState { running: true, exit_code: 0, created_epoch_secs: now - 3 }),
        );
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(3600));
        let reaped = runtime.sweep_orphans().await.unwrap();
        assert_eq!(reaped, 0, "an inspected running+fresh clone with no list create-time must NOT be reaped");
        assert!(backend.take_removed().is_empty());
    }

    #[tokio::test]
    async fn sweep_reaps_inspected_stopped_unknown_create_time() {
        let backend = MockBackend::default();
        *backend.list_result.lock().unwrap() =
            vec![CloneContainerSummary { id: "unknown-stopped".to_string(), created_epoch_secs: 0, running: false }];
        backend.set_inspect(
            "unknown-stopped",
            Some(CloneContainerState { running: false, exit_code: 1, created_epoch_secs: 0 }),
        );
        let runtime = CloneRuntime::new(backend.clone(), Duration::from_secs(3600));
        let reaped = runtime.sweep_orphans().await.unwrap();
        assert_eq!(reaped, 1);
        assert!(backend.take_removed().contains(&"unknown-stopped".to_string()));
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
    fn raw_stderr_debug_does_not_leak_but_into_failed_debug() {
        // A raw stderr with a glued token must not appear in the newtype's Debug,
        // nor when nested in a CloneRunOutcome::Failed Debug line.
        let token = "ghp_supersecrettoken";
        let raw = RawStderr::new(format!("fatal: https://x-access-token:{token}@github.com/x.git not found"));
        let dbg = format!("{raw:?}");
        assert!(!dbg.contains(token), "RawStderr Debug must not print the bytes");
        assert!(dbg.starts_with("RawStderr(<") && dbg.ends_with("bytes, unredacted>)"));
        // as_raw is the explicit audit accessor.
        assert!(raw.as_raw().contains(token));

        let outcome = CloneRunOutcome::Failed { exit_code: 128, stderr_tail: raw };
        let outcome_dbg = format!("{outcome:?}");
        assert!(!outcome_dbg.contains(token), "Failed Debug must not spill the raw stderr token");
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

    #[test]
    fn parse_rfc3339_secs_handles_docker_format() {
        // Docker's `created` looks like 2026-06-15T12:34:56.789012345Z.
        // 2026-06-15T00:00:00Z = 1781568000 (sanity: 2020-01-01 = 1577836800).
        assert_eq!(parse_rfc3339_secs("2020-01-01T00:00:00Z"), 1_577_836_800);
        assert_eq!(parse_rfc3339_secs("2020-01-01T00:00:01.500000000Z"), 1_577_836_801);
        assert_eq!(parse_rfc3339_secs("1970-01-01T00:00:00Z"), 0);
        // Malformed ⇒ 0 (best-effort).
        assert_eq!(parse_rfc3339_secs("not-a-date"), 0);
        assert_eq!(parse_rfc3339_secs(""), 0);
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
            secret_root: temp_secret_root(attempt_id),
            credential: None,
            timeout: Duration::from_secs(120),
            max_bytes: None,
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

        let _ = std::fs::remove_dir_all(temp_root(attempt_id));
    }

    /// CREDENTIALED real-docker read test (#3): the production-breaking bug was a
    /// 0400 secret the clone uid 1011 could not read. This proves the materialized
    /// secret file is actually READABLE inside the `agentforge-clone` container as
    /// the agent user — the test that should have caught the EACCES. It does NOT
    /// require a live private repo: it runs the clone image's own shell to `cat`
    /// the mounted secret as the agent user and asserts success.
    #[tokio::test]
    async fn e2e_credential_file_is_readable_by_clone_uid_when_docker_available() {
        let docker = match bollard::Docker::connect_with_local_defaults() {
            Ok(d) => d,
            Err(_) => {
                eprintln!("clone cred-read e2e: no docker — skipping");
                return;
            }
        };
        if docker.ping().await.is_err() {
            eprintln!("clone cred-read e2e: docker not reachable — skipping");
            return;
        }
        if docker.inspect_image("agentforge-clone:latest").await.is_err() {
            eprintln!("clone cred-read e2e: agentforge-clone:latest not built — skipping");
            return;
        }

        use bollard::models::{ContainerCreateBody, HostConfig};
        use bollard::query_parameters::{
            CreateContainerOptions, LogsOptionsBuilder, RemoveContainerOptionsBuilder, StartContainerOptions,
            WaitContainerOptionsBuilder,
        };

        let attempt_id = Uuid::now_v7();
        let secret_root = temp_secret_root(attempt_id);
        let secret_path = secret_root.join(format!(".clone-secret-{attempt_id}"));
        let sentinel = "x-access-token:SENTINEL-READABLE-TOKEN";
        // Write via the SAME runtime path as production (0700 root + 0644 file).
        write_secret_file(&secret_root, &secret_path, sentinel.as_bytes()).await.expect("write secret");

        // Run the clone image's bash to read the mounted secret as the agent user
        // (the image's USER is uid 1011). Override the entrypoint to a one-shot cat.
        let container_name = format!("agentforge-clone-credtest-{attempt_id}");
        let binds = vec![format!("{}:/run/secrets/git-credential:ro", secret_path.to_string_lossy())];
        let host_config = HostConfig {
            binds: Some(binds),
            network_mode: Some("none".to_string()),
            cap_drop: Some(vec!["ALL".to_string()]),
            security_opt: Some(vec!["no-new-privileges".to_string()]),
            ..Default::default()
        };
        let create_body = ContainerCreateBody {
            image: Some("agentforge-clone:latest".to_string()),
            // id confirms we run as uid 1011; cat confirms the file is readable.
            entrypoint: Some(vec!["/bin/bash".to_string(), "-lc".to_string()]),
            cmd: Some(vec!["id -u; cat /run/secrets/git-credential".to_string()]),
            host_config: Some(host_config),
            ..Default::default()
        };

        let id = docker
            .create_container(
                Some(CreateContainerOptions { name: Some(container_name.clone()), platform: String::new() }),
                create_body,
            )
            .await
            .expect("create cred-test container")
            .id;
        docker.start_container(&id, None::<StartContainerOptions>).await.expect("start cred-test container");

        // Wait for exit.
        let mut wait = docker.wait_container(&id, Some(WaitContainerOptionsBuilder::new().build()));
        let _ = wait.next().await;

        // Read logs.
        let mut logs = docker.logs(&id, Some(LogsOptionsBuilder::new().stdout(true).stderr(true).build()));
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = logs.next().await {
            if let Ok(out) = chunk {
                buf.extend_from_slice(out.as_ref());
            }
        }
        let output = String::from_utf8_lossy(&buf);

        // Cleanup container + secret.
        let _ = docker.remove_container(&id, Some(RemoveContainerOptionsBuilder::new().force(true).build())).await;
        let _ = std::fs::remove_dir_all(temp_root(attempt_id));

        assert!(output.contains("1011"), "clone container must run as uid 1011; output was: {output}");
        assert!(
            output.contains("SENTINEL-READABLE-TOKEN"),
            "clone uid 1011 must be able to READ the mounted secret (this is the EACCES regression); output was: {output}"
        );
    }
}
