//! Deployment-side CLI agent-image auto-updater (default-OFF).
//!
//! Closes the gap between the GHCR-side `watch-cli-versions.yml` workflow (which
//! rebuilds + pushes `agent-<tool>:latest` overlays every 6h) and a running
//! self-hosted deployment, which otherwise keeps the last-pulled image until an
//! operator manually runs `make update-agents`. When enabled
//! (`CLI_IMAGE_AUTO_UPDATE_ENABLED=true`) this worker periodically:
//!
//!   1. asks the registry for the current manifest digest of
//!      `${AGENT_REGISTRY}/agent-<tool>:<tag>` (no pull — daemon-side
//!      `/distribution/<image>/json`),
//!   2. compares it to the locally-pulled digest of the same ref,
//!   3. on drift: `docker pull` the new image and re-tag it to the runtime's
//!      `agentforge-agent:<tool>` ref, so the NEXT spawned agent uses the new
//!      CLI.
//!
//! Policy: RUNNING agents are NEVER touched — only the image the next spawn
//! resolves is refreshed. `claude` has no public registry image (built locally
//! under license), so it is excluded from the registry poll set; instead the
//! sweep checks the npm registry for a newer `@anthropic-ai/claude-code` and —
//! opt-in via `CLI_IMAGE_CLAUDE_AUTO_BUILD` or one click in the admin panel —
//! builds the overlay image server-side from an in-memory Dockerfile (see
//! [`CLAUDE_OVERLAY_DOCKERFILE`]). The worker is deployment-global: it holds no
//! tenant scope and queries no org-scoped table, only Docker (plus the npm
//! registry for claude). Notification is via a structured `warn!` event,
//! Prometheus metrics, a read-only admin status API (`GET /admin/cli-images`),
//! and — when NATS is configured — a live admin WebSocket toast on
//! `broadcast.admin.cli_image`. The worker can also prune superseded overlays
//! (`with_prune`, default-off). Warm-pool drain-on-drift remains a tracked
//! follow-up (`platform/pool.rs` is still dormant).

use std::collections::{BTreeMap, HashMap};
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use agentforge_core::CliToolKind;
use agentforge_core::broadcast_protocol::{ADMIN_CLI_IMAGE_SUBJECT, CLI_IMAGE_UPDATED_EVENT};
use agentforge_platform::container::PlatformError;
use agentforge_platform::{DockerClient, LocalImage, RemoveOutcome};
use tokio::sync::{RwLock, watch};

/// Per-tool image-update state, surfaced read-only by `GET /admin/cli-images`.
#[derive(Debug, Clone, Default)]
pub struct CliToolImageState {
    /// Registry tools: `up_to_date` | `updated` | `failed`.
    /// `claude` (local build) additionally: `update_available`.
    pub state: String,
    /// How this tool's image is kept current: `registry` (pull + re-tag from
    /// GHCR) or `local_build` (claude — built server-side from npm).
    pub update_mode: String,
    pub local_digest: Option<String>,
    pub remote_digest: Option<String>,
    /// Local-build tools only: the CLI version baked into the local image
    /// (`org.agentforge.cli-version` label; `None` = unknown).
    pub local_version: Option<String>,
    /// Local-build tools only: the latest version on the npm registry.
    pub remote_version: Option<String>,
    /// True while a server-side local build is running for this tool.
    pub building: bool,
    pub last_checked_unix: Option<i64>,
    pub last_updated_unix: Option<i64>,
    pub last_error: Option<String>,
}

/// Result of the most recent prune sweep, surfaced read-only by the admin
/// status endpoint. `enabled=false` means the sweep never runs (the default).
#[derive(Debug, Clone, Default)]
pub struct CliImagePruneSummary {
    pub enabled: bool,
    pub last_run_unix: Option<i64>,
    /// Candidate superseded agent images considered this sweep.
    pub scanned: u64,
    pub removed: u64,
    /// Left intact because a container still references them.
    pub skipped_in_use: u64,
    /// Left intact because Docker returned 409 (still tagged / has a child).
    pub skipped_conflict: u64,
    pub errors: u64,
    pub last_error: Option<String>,
}

/// Shared in-memory snapshot of the latest per-tool update state + prune result,
/// plus the single-flight slot for the claude local build. Written by the worker
/// each tick, read by the admin status endpoint, and shared with the manual
/// `POST /admin/cli-images/claude/build` path (which must work even when the
/// poller task was never spawned). Deployment-global (image state is per host,
/// not per org), so no tenant scope.
#[derive(Debug, Default)]
pub struct CliImageUpdateStatus {
    tools: RwLock<BTreeMap<String, CliToolImageState>>,
    prune: RwLock<CliImagePruneSummary>,
    /// True while a claude local build is running (auto-build in the sweep OR a
    /// manual admin build). The single-flight truth shared by both initiators.
    claude_build_inflight: AtomicBool,
}

impl CliImageUpdateStatus {
    pub fn new() -> Self {
        Self::default()
    }

    /// A clone of the current per-tool states (cheap; a handful of tools).
    pub async fn snapshot(&self) -> BTreeMap<String, CliToolImageState> {
        self.tools.read().await.clone()
    }

    /// A clone of the latest prune summary.
    pub async fn prune_snapshot(&self) -> CliImagePruneSummary {
        self.prune.read().await.clone()
    }

    /// Whether a claude local build is currently running (sweep or manual).
    pub fn claude_build_in_flight(&self) -> bool {
        self.claude_build_inflight.load(Ordering::SeqCst)
    }

    /// Try to claim the claude build single-flight slot. `None` means a build
    /// is already in flight (the manual endpoint maps that to 409). The slot is
    /// released when the returned guard drops — including on early return or
    /// panic — so a failed build can never wedge future builds.
    pub fn try_acquire_claude_build(self: &Arc<Self>) -> Option<ClaudeBuildSlot> {
        self.claude_build_inflight
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
            .then(|| ClaudeBuildSlot { status: Arc::clone(self) })
    }

    async fn record(&self, tool: &str, state: CliToolImageState) {
        self.tools.write().await.insert(tool.to_string(), state);
    }

    async fn record_prune(&self, summary: CliImagePruneSummary) {
        *self.prune.write().await = summary;
    }
}

/// RAII guard for the claude build single-flight slot. Mirrors the roll path's
/// `RollGuard` semantics: hold for the duration of the build, release on drop.
#[derive(Debug)]
pub struct ClaudeBuildSlot {
    status: Arc<CliImageUpdateStatus>,
}

impl Drop for ClaudeBuildSlot {
    fn drop(&mut self) {
        self.status.claude_build_inflight.store(false, Ordering::SeqCst);
    }
}

/// Default poll cadence (15 min). CLI publishers ship at most a few times per
/// week, so this is well clear of registry rate limits while still observing a
/// new release within one operator alerting window.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(900);

/// Floor on the poll interval. A `0`/tiny interval would busy-loop the registry
/// and trip anonymous-pull rate limits (a self-inflicted ban). Clamped up.
const MIN_INTERVAL: Duration = Duration::from_secs(60);

const DEFAULT_REGISTRY: &str = "ghcr.io/wisdoverse/wisdoverse-forge";
const DEFAULT_CLI_IMAGE_TAG: &str = "latest";

/// The Container CLI tools that have a public registry overlay image. `claude`
/// is excluded — it is built locally (license precludes a public GHCR image),
/// so the updater must never attempt to PULL it. Claude is still checked each
/// sweep via [`CliImageUpdater::check_claude`] (npm version diff + optional
/// local build); it is just never part of the registry pull/prune/roll set.
fn pollable_tools() -> impl Iterator<Item = CliToolKind> {
    CliToolKind::ALL.into_iter().filter(|tool| !matches!(tool, CliToolKind::Claude))
}

/// Canonical pollable tool names (`CliToolKind::ALL` minus `claude`). The single
/// source of truth shared by the worker's registry poll loop, the prune scoping,
/// and the roll allowlist, so none of those paths can ever touch `claude`.
pub fn pollable_tool_names() -> Vec<&'static str> {
    pollable_tools().map(|tool| tool.as_str()).collect()
}

/// Every tool the admin status report lists: the registry-pollable set PLUS
/// `claude` (checked via npm + local build). Drives `GET /admin/cli-images` so
/// claude is a first-class row even though it is never pulled from a registry.
pub fn reported_tool_names() -> Vec<&'static str> {
    CliToolKind::ALL.into_iter().map(|tool| tool.as_str()).collect()
}

/// How a tool's image is kept current: `registry` (pull + re-tag from GHCR) or
/// `local_build` (claude — no public image, built server-side from npm).
pub fn update_mode_for(tool: &str) -> &'static str {
    if tool == CliToolKind::Claude.as_str() { UPDATE_MODE_LOCAL_BUILD } else { UPDATE_MODE_REGISTRY }
}

/// `update_mode` value for tools pulled from a public registry.
pub const UPDATE_MODE_REGISTRY: &str = "registry";
/// `update_mode` value for tools built locally on this server (claude).
pub const UPDATE_MODE_LOCAL_BUILD: &str = "local_build";

/// npm package that ships the claude Container CLI.
pub const CLAUDE_NPM_PACKAGE: &str = "@anthropic-ai/claude-code";

/// Default npm registry base for the claude version check + build. Operators
/// can override with `CLI_IMAGE_NPM_REGISTRY` (e.g. a China mirror).
pub const DEFAULT_NPM_REGISTRY: &str = "https://registry.npmjs.org";

/// Image label carrying the CLI version baked into a locally-built claude
/// image. Written by [`CLAUDE_OVERLAY_DOCKERFILE`] and by the Makefile's
/// `build-agent` target (`--label org.agentforge.cli-version=$(_VER)`).
pub const CLAUDE_VERSION_LABEL: &str = "org.agentforge.cli-version";

/// Pre-existing label baked by `docker/Dockerfile.agent` since before the
/// local-build feature. Read as a fallback so images operators built with an
/// older Makefile still report their version instead of "unknown".
const LEGACY_CLAUDE_VERSION_LABEL: &str = "org.wisdoverse.cli-version";

/// The runtime ref the container-start resolver spawns claude agents from —
/// the build's final re-tag target (same convention as `local_runtime_ref`).
const CLAUDE_RUNTIME_REF: &str = "agentforge-agent:claude";

/// Local tag of the shared agent base image every overlay builds FROM. Pulled
/// from `${AGENT_REGISTRY}/agent-base:<tag>` and re-tagged when absent
/// (mirrors the Makefile's `update-agent-base` target).
const AGENT_BASE_IMAGE_REF: &str = "agentforge-agent-base:latest";

/// Build context for the server-side claude overlay build.
///
/// MIRROR OF `docker/Dockerfile.agent` for `CLI_TOOL=claude` — every
/// instruction after that file's ARG block is reproduced here (npm-mirror
/// config, the claude arm of the install `case`, the `AGENTFORGE_CLI_*` ENVs,
/// and the version LABELs). `docker/Dockerfile.agent` carries a reciprocal
/// comment pointing back at this constant; change them together. `CLI_VERSION`
/// and `NPM_REGISTRY` are supplied as Docker build-args at build time
/// ([`build_claude_overlay`]), exactly like the Makefile's `build-agent`
/// target passes `--build-arg`.
pub const CLAUDE_OVERLAY_DOCKERFILE: &str = r#"# Generated in-memory by the Wisdoverse Forge server for the claude local
# image build (no public registry image — license requires self-build).
# Source of truth: rust/crates/jobs/src/cli_image_updater.rs
# (CLAUDE_OVERLAY_DOCKERFILE). Mirrors docker/Dockerfile.agent with
# CLI_TOOL=claude; keep both files in sync.

ARG BASE_IMAGE=agentforge-agent-base:latest
FROM ${BASE_IMAGE}

ARG CLI_TOOL=claude
# CLI_VERSION: pinned version for reproducible builds (e.g. "2.1.38")
ARG CLI_VERSION=latest
# China mirror: pass --build-arg NPM_REGISTRY=https://registry.npmmirror.com
ARG NPM_REGISTRY=
RUN if [ -n "$NPM_REGISTRY" ]; then npm config set registry "$NPM_REGISTRY"; fi

# Install CLI tool with pinned version for immutable, reproducible images.
RUN npm install -g @anthropic-ai/claude-code@${CLI_VERSION}

# Bake the CLI tool type and version into the image
ENV AGENTFORGE_CLI_TOOL=$CLI_TOOL
ENV AGENTFORGE_CLI_VERSION=$CLI_VERSION
LABEL org.wisdoverse.cli-version=$CLI_VERSION
LABEL org.agentforge.cli-version=$CLI_VERSION
"#;

/// The registry base the updater pulls overlays from (`AGENT_REGISTRY`, else the
/// built-in GHCR default). Surfaced for the admin status endpoint so operators
/// see the exact ref source without re-deriving the env convention.
pub fn configured_registry() -> String {
    non_empty_env("AGENT_REGISTRY").unwrap_or_else(|| DEFAULT_REGISTRY.to_string())
}

/// The image tag the updater tracks (`AGENT_CLI_IMAGE_TAG`, else `latest`).
pub fn configured_image_tag() -> String {
    non_empty_env("AGENT_CLI_IMAGE_TAG").unwrap_or_else(|| DEFAULT_CLI_IMAGE_TAG.to_string())
}

/// The cadence the worker will ACTUALLY poll at for a configured value, after
/// the `MIN_INTERVAL` floor. The single source of truth shared by the worker
/// (`with_interval`) and the admin status projection, so the panel never
/// reports a faster cadence than the worker runs.
pub fn effective_interval_secs(configured_secs: u64) -> u64 {
    configured_secs.max(MIN_INTERVAL.as_secs())
}

pub struct CliImageUpdater {
    docker: Arc<DockerClient>,
    status: Arc<CliImageUpdateStatus>,
    interval: Duration,
    /// Optional NATS client used to publish admin toast frames on the
    /// `broadcast.admin.cli_image` subject. `None` (the default) makes toast
    /// emission a no-op — the worker still records status + metrics. Set via
    /// [`Self::with_event_sink`] when NATS is configured.
    event_sink: Option<async_nats::Client>,
    /// When true, each poll sweep also prunes superseded (dangling) agent
    /// overlay images. Default `false` — a destructive image-removal path stays
    /// opt-in. Set via [`Self::with_prune`].
    prune_enabled: bool,
    /// When true, the sweep builds the claude overlay locally as soon as a
    /// newer npm version is detected (zero clicks). Default `false` — the sweep
    /// stays detect-only and the admin panel offers the one-click build. Set
    /// via [`Self::with_claude_auto_build`].
    claude_auto_build: bool,
    /// npm registry base override for the claude version check + build
    /// (`CLI_IMAGE_NPM_REGISTRY`); `None` uses [`DEFAULT_NPM_REGISTRY`]. Set
    /// via [`Self::with_npm_registry`].
    npm_registry: Option<String>,
}

impl CliImageUpdater {
    pub fn new(docker: Arc<DockerClient>, status: Arc<CliImageUpdateStatus>) -> Self {
        Self {
            docker,
            status,
            interval: DEFAULT_INTERVAL,
            event_sink: None,
            prune_enabled: false,
            claude_auto_build: false,
            npm_registry: None,
        }
    }

    /// Attach the NATS client the worker publishes admin toast frames to. Pass
    /// `None` (e.g. NATS not configured) to leave toasts disabled.
    pub fn with_event_sink(mut self, client: Option<async_nats::Client>) -> Self {
        self.event_sink = client;
        self
    }

    /// Enable pruning of superseded agent overlay images after each sweep.
    pub fn with_prune(mut self, enabled: bool) -> Self {
        self.prune_enabled = enabled;
        self
    }

    /// Enable the zero-click claude local build when a newer npm version lands.
    pub fn with_claude_auto_build(mut self, enabled: bool) -> Self {
        self.claude_auto_build = enabled;
        self
    }

    /// Override the npm registry base used for the claude check + build.
    pub fn with_npm_registry(mut self, registry: Option<String>) -> Self {
        self.npm_registry = registry.map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
        self
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        if interval < MIN_INTERVAL {
            tracing::warn!(
                requested_secs = interval.as_secs(),
                floor_secs = MIN_INTERVAL.as_secs(),
                "cli image auto-update interval below floor; clamping to avoid hammering the registry"
            );
        }
        self.interval = Duration::from_secs(effective_interval_secs(interval.as_secs()));
        self
    }

    /// Run the poll loop until shutdown is signalled.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        tracing::info!(interval_secs = self.interval.as_secs(), "cli image auto-updater started");
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_ok() && *shutdown.borrow() {
                        tracing::info!("cli image auto-updater shutting down");
                        break;
                    }
                }
                _ = ticker.tick() => self.poll_once().await,
            }
        }
    }

    /// One poll sweep. Each tool is checked independently — one tool's registry
    /// hiccup never aborts the others — and the per-tool result is recorded into
    /// the shared status snapshot the admin endpoint serves.
    async fn poll_once(&self) {
        let now = chrono::Utc::now().timestamp();
        let prior = self.status.snapshot().await;
        for tool in pollable_tools() {
            let prev = prior.get(tool.as_str());
            let state = match self.check_and_update(tool).await {
                Ok(UpdateOutcome::UpToDate { digest }) => CliToolImageState {
                    state: "up_to_date".to_string(),
                    update_mode: UPDATE_MODE_REGISTRY.to_string(),
                    local_digest: Some(digest.clone()),
                    remote_digest: Some(digest),
                    local_version: None,
                    remote_version: None,
                    building: false,
                    last_checked_unix: Some(now),
                    last_updated_unix: prev.and_then(|p| p.last_updated_unix),
                    last_error: None,
                },
                Ok(UpdateOutcome::Updated { to, .. }) => CliToolImageState {
                    state: "updated".to_string(),
                    update_mode: UPDATE_MODE_REGISTRY.to_string(),
                    local_digest: Some(to.clone()),
                    remote_digest: Some(to),
                    local_version: None,
                    remote_version: None,
                    building: false,
                    last_checked_unix: Some(now),
                    last_updated_unix: Some(now),
                    last_error: None,
                },
                Err(err) => {
                    tracing::warn!(tool = tool.as_str(), error = %err, "cli image update check failed");
                    metrics::counter!("agentforge_cli_image_pull_total", "tool" => tool.as_str(), "result" => "failed")
                        .increment(1);
                    CliToolImageState {
                        state: "failed".to_string(),
                        update_mode: UPDATE_MODE_REGISTRY.to_string(),
                        // carry forward the last-known digests so a transient
                        // registry blip doesn't blank the operator's view.
                        local_digest: prev.and_then(|p| p.local_digest.clone()),
                        remote_digest: prev.and_then(|p| p.remote_digest.clone()),
                        local_version: None,
                        remote_version: None,
                        building: false,
                        last_checked_unix: Some(now),
                        last_updated_unix: prev.and_then(|p| p.last_updated_unix),
                        last_error: Some(err.to_string()),
                    }
                }
            };
            // Toast admins only on a real transition, BEFORE recording so the
            // frame reflects this tick: every landed update, and a failure that
            // is new or whose error changed. A steady failing tool does NOT
            // re-toast every tick (that would spam NATS and, with a fixed id,
            // could mask a later distinct failure).
            if should_toast(prev, &state) {
                self.emit_toast(tool.as_str(), &state, now).await;
            }
            self.status.record(tool.as_str(), state).await;
        }

        // claude has no public registry image — check the npm registry instead
        // and (opt-in) build the overlay locally. Runs inline by design: the
        // sweep loop is sequential, so a build simply extends this tick.
        self.check_claude(now, &prior).await;

        // After refreshing every tool, optionally reclaim superseded (dangling)
        // agent overlay images. Default-off; image-level only (never a
        // container op), and scoped + reference-guarded so it can never touch a
        // live image or another stack's layers.
        if self.prune_enabled {
            let summary = self.prune_orphans(now).await;
            self.status.record_prune(summary).await;
        }
    }

    /// The claude step of the sweep: diff the npm `latest` version against the
    /// version label baked into the local `agentforge-agent:claude` image, then
    /// either record `up_to_date` / `update_available` (detect-only, the
    /// default) or — when auto-build is enabled — run the local build inline.
    /// Same carry-forward-on-transient-error and toast-on-transition semantics
    /// as the registry loop above.
    async fn check_claude(&self, now: i64, prior: &BTreeMap<String, CliToolImageState>) {
        let tool = CliToolKind::Claude.as_str();
        let prev = prior.get(tool);
        if self.status.claude_build_in_flight() {
            // A build (manual, or auto from a previous tick) is still running;
            // its completion path records the outcome. Don't clobber the
            // `building` state with a stale check result.
            return;
        }

        let state = match self.check_claude_versions().await {
            Ok(ClaudeOutcome::UpToDate { version }) => claude_state_up_to_date(now, prev, version),
            Ok(ClaudeOutcome::UpdateAvailable { local, remote }) => {
                if self.claude_auto_build
                    && let Some(slot) = self.status.try_acquire_claude_build()
                {
                    let ctx = ClaudeBuildContext {
                        docker: self.docker.clone(),
                        status: self.status.clone(),
                        event_sink: self.event_sink.clone(),
                        npm_registry: self.npm_registry.clone(),
                    };
                    // Inline await: the sweep is sequential by design, and
                    // execute_claude_build records state + toasts itself.
                    execute_claude_build(slot, ctx, &remote).await;
                    return;
                }
                claude_state_update_available(now, prev, local, remote)
            }
            Err(err) => {
                tracing::warn!(tool, error = %err, "claude local-build version check failed");
                metrics::counter!("agentforge_cli_image_pull_total", "tool" => tool, "result" => "failed").increment(1);
                claude_state_failed(now, prev, err)
            }
        };

        if should_toast(prev, &state) {
            self.emit_toast(tool, &state, now).await;
        }
        self.status.record(tool, state).await;
    }

    /// Fetch npm `latest` + read the local image's version label, reduced to a
    /// pure [`ClaudeOutcome`] decision. A missing image or missing label is an
    /// UNKNOWN local version → update available whenever npm answers.
    async fn check_claude_versions(&self) -> Result<ClaudeOutcome, String> {
        let remote = fetch_claude_latest_version(self.npm_registry.as_deref()).await?;
        let local = local_claude_version(&self.docker).await.map_err(|err| err.to_string())?;
        Ok(derive_claude_outcome(local, remote))
    }

    /// Reclaim superseded agent overlay images: list local images, keep only
    /// dangling ones whose source repo is one of OUR pollable-tool GHCR overlays,
    /// and remove each that no container (running or stopped) references.
    /// Best-effort + fully self-contained — any error is counted and logged, the
    /// summary still records, and the next sweep retries.
    async fn prune_orphans(&self, now: i64) -> CliImagePruneSummary {
        let mut summary = CliImagePruneSummary { enabled: true, last_run_unix: Some(now), ..Default::default() };

        let images = match self.docker.list_local_images().await {
            Ok(images) => images,
            Err(err) => {
                tracing::warn!(error = %err, "cli image prune: list images failed");
                summary.errors += 1;
                summary.last_error = Some(err.to_string());
                return summary;
            }
        };
        let referenced = match self.docker.referenced_image_ids().await {
            Ok(set) => set,
            Err(err) => {
                // Without the reference set we cannot prove an image is unused —
                // refuse to remove anything this sweep (fail safe).
                tracing::warn!(error = %err, "cli image prune: container reference scan failed; skipping removals");
                summary.errors += 1;
                summary.last_error = Some(err.to_string());
                return summary;
            }
        };

        let repos = agent_overlay_repos();
        for image in images.iter().filter(|img| is_prunable_agent_image(img, &repos)) {
            summary.scanned += 1;
            match self.docker.remove_image_if_unreferenced(&image.id, &referenced).await {
                Ok(RemoveOutcome::Removed) => {
                    summary.removed += 1;
                    tracing::info!(image_id = %image.id, "cli image prune: removed superseded agent image");
                }
                Ok(RemoveOutcome::SkippedInUse) => summary.skipped_in_use += 1,
                Ok(RemoveOutcome::SkippedConflict) => summary.skipped_conflict += 1,
                Ok(RemoveOutcome::NotFound) => {}
                Err(err) => {
                    tracing::warn!(image_id = %image.id, error = %err, "cli image prune: remove failed (non-fatal)");
                    summary.errors += 1;
                    summary.last_error = Some(err.to_string());
                }
            }
        }

        metrics::counter!("agentforge_cli_image_pruned_total").increment(summary.removed);
        tracing::debug!(
            scanned = summary.scanned,
            removed = summary.removed,
            in_use = summary.skipped_in_use,
            conflict = summary.skipped_conflict,
            errors = summary.errors,
            "cli image prune sweep complete"
        );
        summary
    }

    /// Publish an admin toast frame for `updated`/`update_available`/`failed`
    /// transitions. Best effort: a publish failure is logged and never affects
    /// the update result or the status snapshot. No-op when no NATS sink is
    /// attached, and silent for `up_to_date`/`pending` (those would be noise).
    async fn emit_toast(&self, tool: &str, state: &CliToolImageState, unix: i64) {
        publish_cli_image_toast(self.event_sink.as_ref(), tool, state, unix).await;
    }

    /// Diff the registry vs the local image for one tool and, on drift, pull +
    /// re-tag the runtime ref. Returns the digest decision for tests/logging.
    async fn check_and_update(&self, tool: CliToolKind) -> Result<UpdateOutcome, PlatformError> {
        let remote_ref = remote_ref(tool.as_str());
        let local_runtime_ref = local_runtime_ref(tool.as_str());

        let remote_digest = self.docker.remote_image_digest(&remote_ref, None).await?;
        // Compare the GHCR ref's LOCAL digest (a pulled image's RepoDigests
        // carries the manifest digest) against the remote manifest digest —
        // apples to apples. None => not pulled yet => drift.
        let local_digest = self.docker.local_image_digest(&remote_ref).await?;

        if local_digest.as_deref() == Some(remote_digest.as_str()) {
            metrics::counter!("agentforge_cli_image_pull_total", "tool" => tool.as_str(), "result" => "skipped")
                .increment(1);
            tracing::debug!(tool = tool.as_str(), digest = %remote_digest, "cli image up to date");
            return Ok(UpdateOutcome::UpToDate { digest: remote_digest });
        }

        metrics::counter!("agentforge_cli_image_drift_detected_total", "tool" => tool.as_str()).increment(1);
        let started = std::time::Instant::now();
        self.docker.pull_image(&remote_ref, None).await?;
        let (target_repo, target_tag) = split_repo_tag(&local_runtime_ref);
        // The pull succeeded; if the re-tag now fails the new image is on disk
        // but the runtime ref still points at the OLD one. Surface that
        // distinctly (it differs from a registry/pull failure) — the next tick
        // re-detects drift and retries the tag.
        if let Err(err) = self.docker.tag_image(&remote_ref, &target_repo, &target_tag).await {
            tracing::error!(
                tool = tool.as_str(),
                remote_ref = %remote_ref,
                runtime_ref = %local_runtime_ref,
                error = %err,
                "cli image PULLED but failed to re-tag the runtime ref; new agents still run the OLD cli until the next tick retries"
            );
            return Err(err);
        }
        metrics::histogram!("agentforge_cli_image_pull_duration_seconds", "tool" => tool.as_str())
            .record(started.elapsed().as_secs_f64());
        metrics::counter!("agentforge_cli_image_pull_total", "tool" => tool.as_str(), "result" => "success")
            .increment(1);
        tracing::warn!(
            tool = tool.as_str(),
            from = local_digest.as_deref().unwrap_or("<none>"),
            to = %remote_digest,
            remote_ref = %remote_ref,
            local_ref = %local_runtime_ref,
            "cli agent image updated; new agents will use the new CLI (running agents unaffected)"
        );
        Ok(UpdateOutcome::Updated { from: local_digest, to: remote_digest })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UpdateOutcome {
    UpToDate { digest: String },
    Updated { from: Option<String>, to: String },
}

// ---------------------------------------------------------------------------
// claude local build (no public registry image — npm version diff + docker
// build from CLAUDE_OVERLAY_DOCKERFILE)
// ---------------------------------------------------------------------------

/// Version-diff decision for the claude local build.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ClaudeOutcome {
    UpToDate { version: String },
    UpdateAvailable { local: Option<String>, remote: String },
}

/// Pure derivation: a local version is current only when it EXACTLY matches
/// npm `latest`. `None` (missing image or missing label) is unknown → an
/// update is available whenever npm returns a version.
fn derive_claude_outcome(local: Option<String>, remote: String) -> ClaudeOutcome {
    match local {
        Some(version) if version == remote => ClaudeOutcome::UpToDate { version },
        other => ClaudeOutcome::UpdateAvailable { local: other, remote },
    }
}

/// The CLI version baked into the local `agentforge-agent:claude` image, read
/// from the `org.agentforge.cli-version` label (falling back to the
/// pre-existing `org.wisdoverse.cli-version` label so operator-built images
/// from before this feature still report a version). `Ok(None)` = image
/// missing or unlabeled → unknown.
async fn local_claude_version(docker: &DockerClient) -> Result<Option<String>, PlatformError> {
    let Some(labels) = docker.image_labels(CLAUDE_RUNTIME_REF).await? else {
        return Ok(None);
    };
    Ok(labels
        .get(CLAUDE_VERSION_LABEL)
        .or_else(|| labels.get(LEGACY_CLAUDE_VERSION_LABEL))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

fn claude_state_base(now: i64, prev: Option<&CliToolImageState>) -> CliToolImageState {
    CliToolImageState {
        state: String::new(),
        update_mode: UPDATE_MODE_LOCAL_BUILD.to_string(),
        // local-build images have no registry manifest digests by definition.
        local_digest: None,
        remote_digest: None,
        local_version: None,
        remote_version: None,
        building: false,
        last_checked_unix: Some(now),
        last_updated_unix: prev.and_then(|p| p.last_updated_unix),
        last_error: None,
    }
}

fn claude_state_up_to_date(now: i64, prev: Option<&CliToolImageState>, version: String) -> CliToolImageState {
    CliToolImageState {
        state: "up_to_date".to_string(),
        local_version: Some(version.clone()),
        remote_version: Some(version),
        ..claude_state_base(now, prev)
    }
}

fn claude_state_update_available(
    now: i64,
    prev: Option<&CliToolImageState>,
    local: Option<String>,
    remote: String,
) -> CliToolImageState {
    CliToolImageState {
        state: "update_available".to_string(),
        local_version: local,
        remote_version: Some(remote),
        ..claude_state_base(now, prev)
    }
}

fn claude_state_updated(now: i64, prev: Option<&CliToolImageState>, version: String) -> CliToolImageState {
    CliToolImageState {
        state: "updated".to_string(),
        local_version: Some(version.clone()),
        remote_version: Some(version),
        last_updated_unix: Some(now),
        ..claude_state_base(now, prev)
    }
}

fn claude_state_failed(now: i64, prev: Option<&CliToolImageState>, error: String) -> CliToolImageState {
    CliToolImageState {
        state: "failed".to_string(),
        // carry forward the last-known versions so a transient npm/docker blip
        // doesn't blank the operator's view (same policy as registry tools).
        local_version: prev.and_then(|p| p.local_version.clone()),
        remote_version: prev.and_then(|p| p.remote_version.clone()),
        last_error: Some(error),
        ..claude_state_base(now, prev)
    }
}

/// Everything a claude build needs, decoupled from the updater struct so the
/// manual `POST /admin/cli-images/claude/build` endpoint can run the identical
/// flow even when the poller task was never spawned (auto-update off).
pub struct ClaudeBuildContext {
    pub docker: Arc<DockerClient>,
    pub status: Arc<CliImageUpdateStatus>,
    /// NATS sink for the completion toast on `broadcast.admin.cli_image`;
    /// `None` leaves toasts off (status + metrics still record).
    pub event_sink: Option<async_nats::Client>,
    /// npm registry override, forwarded as the `NPM_REGISTRY` build-arg.
    pub npm_registry: Option<String>,
}

/// Run one claude local build end to end, holding `slot` (the single-flight
/// guard) for the duration: record `building` once before, build, record the
/// final `updated`/`failed` state once after, and emit the admin toast on the
/// same transition rules as the sweep. Never returns an error — the outcome is
/// the recorded state, exactly like a sweep tick.
pub async fn execute_claude_build(slot: ClaudeBuildSlot, ctx: ClaudeBuildContext, target_version: &str) {
    let tool = CliToolKind::Claude.as_str();
    let now = chrono::Utc::now().timestamp();
    let prior = ctx.status.snapshot().await;
    let prev = prior.get(tool);

    // Record once BEFORE: the panel shows "building" for the whole docker build
    // (npm install of the CLI — typically minutes, not seconds).
    let mut in_progress = claude_state_update_available(
        now,
        prev,
        prev.and_then(|p| p.local_version.clone()),
        target_version.to_string(),
    );
    in_progress.building = true;
    ctx.status.record(tool, in_progress).await;

    metrics::counter!("agentforge_cli_image_drift_detected_total", "tool" => tool).increment(1);
    let started = std::time::Instant::now();
    let result = build_claude_overlay(&ctx.docker, target_version, ctx.npm_registry.as_deref()).await;
    let finished = chrono::Utc::now().timestamp();

    // Re-snapshot so the toast-transition compare sees the `building` record.
    let prior = ctx.status.snapshot().await;
    let prev = prior.get(tool);
    let state = match result {
        Ok(()) => {
            metrics::histogram!("agentforge_cli_image_pull_duration_seconds", "tool" => tool)
                .record(started.elapsed().as_secs_f64());
            metrics::counter!("agentforge_cli_image_pull_total", "tool" => tool, "result" => "success").increment(1);
            tracing::warn!(
                tool,
                version = target_version,
                local_ref = CLAUDE_RUNTIME_REF,
                "claude agent image built locally; new agents will use the new CLI (running agents unaffected)"
            );
            claude_state_updated(finished, prev, target_version.to_string())
        }
        Err(err) => {
            metrics::counter!("agentforge_cli_image_pull_total", "tool" => tool, "result" => "failed").increment(1);
            tracing::warn!(tool, version = target_version, error = %err, "claude local image build failed");
            claude_state_failed(finished, prev, err.to_string())
        }
    };

    if should_toast(prev, &state) {
        publish_cli_image_toast(ctx.event_sink.as_ref(), tool, &state, finished).await;
    }
    ctx.status.record(tool, state).await;
    drop(slot); // release the single-flight slot only after the outcome is recorded
}

/// Build the claude overlay image — the server-side equivalent of
/// `make build-claude`: ensure `agentforge-agent-base:latest` exists locally
/// (pull + re-tag `${AGENT_REGISTRY}/agent-base:<tag>` when absent, mirroring
/// the Makefile's `update-agent-base`/`ensure-agent-base` targets), then
/// `docker build` [`CLAUDE_OVERLAY_DOCKERFILE`] with the pinned `CLI_VERSION`,
/// tagging `agentforge-agent:claude-<version>` and re-tagging the runtime ref
/// `agentforge-agent:claude` the next spawn resolves.
pub async fn build_claude_overlay(
    docker: &DockerClient,
    version: &str,
    npm_registry: Option<&str>,
) -> Result<(), PlatformError> {
    ensure_agent_base_image(docker).await?;

    let mut build_args = HashMap::new();
    build_args.insert("CLI_VERSION".to_string(), version.to_string());
    if let Some(registry) = npm_registry.map(str::trim).filter(|value| !value.is_empty()) {
        build_args.insert("NPM_REGISTRY".to_string(), registry.to_string());
    }

    let versioned_ref = format!("{CLAUDE_RUNTIME_REF}-{version}");
    docker.build_image_from_dockerfile(CLAUDE_OVERLAY_DOCKERFILE, &versioned_ref, &build_args).await?;

    // The build succeeded; point the runtime ref at it (same recovery shape as
    // the registry path: on a tag failure the image is on disk but the next
    // spawn still uses the OLD one — the next sweep re-detects and retries).
    let (target_repo, target_tag) = split_repo_tag(CLAUDE_RUNTIME_REF);
    if let Err(err) = docker.tag_image(&versioned_ref, &target_repo, &target_tag).await {
        tracing::error!(
            versioned_ref = %versioned_ref,
            runtime_ref = CLAUDE_RUNTIME_REF,
            error = %err,
            "claude image BUILT but failed to re-tag the runtime ref; new agents still run the OLD cli until the next attempt"
        );
        return Err(err);
    }
    Ok(())
}

/// Make sure the shared agent base image exists locally before an overlay
/// build, reusing the same pull + re-tag helpers the registry updater uses.
/// An already-present base (e.g. built locally via `make build-agent-base`) is
/// left untouched — this never force-refreshes the base.
async fn ensure_agent_base_image(docker: &DockerClient) -> Result<(), PlatformError> {
    if docker.image_exists(AGENT_BASE_IMAGE_REF).await? {
        return Ok(());
    }
    let remote_ref = format!("{}/agent-base:{}", configured_registry().trim_end_matches('/'), configured_image_tag());
    tracing::info!(remote_ref = %remote_ref, local_ref = AGENT_BASE_IMAGE_REF, "agent base image missing locally; pulling");
    docker.pull_image(&remote_ref, None).await?;
    let (target_repo, target_tag) = split_repo_tag(AGENT_BASE_IMAGE_REF);
    docker.tag_image(&remote_ref, &target_repo, &target_tag).await
}

/// npm registry base after the `CLI_IMAGE_NPM_REGISTRY` override: trimmed, no
/// trailing slash, defaulting to [`DEFAULT_NPM_REGISTRY`].
fn npm_registry_base(configured: Option<&str>) -> String {
    configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_NPM_REGISTRY)
        .trim_end_matches('/')
        .to_string()
}

/// `<registry>/@anthropic-ai%2Fclaude-code/latest` — the scoped package name
/// keeps its `@` and URL-encodes the inner `/`, the canonical form for scoped
/// packages (verified against registry.npmjs.org; the unencoded form also
/// answers 200, but the encoded one is what npm itself sends and what mirrors
/// are tested against).
fn claude_npm_latest_url(registry_base: &str) -> String {
    format!("{registry_base}/{}/latest", CLAUDE_NPM_PACKAGE.replacen('/', "%2F", 1))
}

/// Timeout for the npm `latest` lookup. No retries — the sweep re-runs every
/// interval, and the manual endpoint surfaces the error to the operator.
const NPM_LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

/// GET npm `latest` for the claude CLI and parse `.version`. String errors —
/// they land verbatim in the recorded state's `last_error` / the 503 detail.
pub async fn fetch_claude_latest_version(npm_registry: Option<&str>) -> Result<String, String> {
    let url = claude_npm_latest_url(&npm_registry_base(npm_registry));
    let client = reqwest::Client::builder()
        .timeout(NPM_LOOKUP_TIMEOUT)
        .build()
        .map_err(|err| format!("npm http client init failed: {err}"))?;
    let response = client.get(&url).send().await.map_err(|err| format!("npm registry request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("npm registry returned HTTP {status} for {url}"));
    }
    let body = response.text().await.map_err(|err| format!("npm registry response read failed: {err}"))?;
    parse_npm_latest_version(&body)
}

/// Parse the `version` field from an npm `<pkg>/latest` document. Pure — unit
/// tested against literal JSON, no network.
fn parse_npm_latest_version(body: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|err| format!("npm registry returned invalid JSON: {err}"))?;
    value
        .get("version")
        .and_then(|version| version.as_str())
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty())
        .ok_or_else(|| "npm registry response has no version field".to_string())
}

/// Publish one admin toast frame (shared by the sweep and the build task).
/// Best effort: failures are logged, never propagated.
async fn publish_cli_image_toast(sink: Option<&async_nats::Client>, tool: &str, state: &CliToolImageState, unix: i64) {
    let Some(client) = sink else {
        return;
    };
    let Some(frame) = build_cli_image_frame(tool, state, unix) else {
        return;
    };
    if let Err(err) = client.publish(ADMIN_CLI_IMAGE_SUBJECT, frame.into_bytes().into()).await {
        tracing::warn!(tool, error = %err, "failed to publish cli image admin toast (non-fatal)");
    }
}

/// Register metric descriptions + initialise series at 0 so dashboards have
/// them from the first scrape even before the (default-off) worker runs.
pub fn register_metrics() {
    metrics::describe_counter!(
        "agentforge_cli_image_pull_total",
        "CLI agent-image update checks; labels tool + result (success|skipped|failed)"
    );
    metrics::describe_counter!(
        "agentforge_cli_image_drift_detected_total",
        "Times a newer CLI agent image was detected on the registry; label tool"
    );
    metrics::describe_histogram!(
        "agentforge_cli_image_pull_duration_seconds",
        "Time to pull + re-tag one updated CLI agent image; label tool"
    );
    metrics::describe_counter!(
        "agentforge_cli_image_pruned_total",
        "Superseded agent overlay images removed by the prune sweep (default-off)"
    );
    metrics::counter!("agentforge_cli_image_pruned_total").increment(0);
    // All reported tools, including `claude`: its local-build path records the
    // same pull_total/drift/duration series (a "pull" is a local build there).
    for tool in reported_tool_names() {
        metrics::counter!("agentforge_cli_image_pull_total", "tool" => tool, "result" => "success").increment(0);
        metrics::counter!("agentforge_cli_image_drift_detected_total", "tool" => tool).increment(0);
    }
}

/// `${AGENT_REGISTRY}/agent-<tool>:<tag>` — the public GHCR overlay ref the
/// `watch-cli-versions.yml` workflow publishes. Registry + tag come from raw env
/// (matching the Makefile's `AGENT_REGISTRY` / overlay-`:latest` convention).
fn remote_ref(tool: &str) -> String {
    build_remote_ref(&configured_registry(), &configured_image_tag(), tool)
}

fn build_remote_ref(registry: &str, tag: &str, tool: &str) -> String {
    format!("{}/agent-{tool}:{tag}", registry.trim_end_matches('/'))
}

/// The exact GHCR repo prefixes (`<registry>/agent-<tool>`, no tag) for the
/// pollable tools. The prune scoping matches a dangling image's repo digest
/// against THIS set, so it can only ever target our own overlays — never the
/// base image (`agent-base` is not a pollable tool), never `claude`, never
/// another stack's images.
fn agent_overlay_repos() -> Vec<String> {
    let registry = configured_registry();
    let base = registry.trim_end_matches('/');
    pollable_tools().map(|tool| format!("{base}/agent-{}", tool.as_str())).collect()
}

/// Is `image` a superseded agent overlay safe to consider for removal? True only
/// when it is DANGLING (no repo tags — a current image keeps its
/// `agentforge-agent:<tool>` / `ghcr…/agent-<tool>:<tag>` tag) AND a repo digest
/// names one of our pollable-tool overlay repos. Reference-safety (no container
/// uses it) is enforced separately by `remove_image_if_unreferenced`.
fn is_prunable_agent_image(image: &LocalImage, agent_repos: &[String]) -> bool {
    if !image.repo_tags.is_empty() {
        return false;
    }
    image
        .repo_digests
        .iter()
        .any(|digest| digest.rsplit_once('@').is_some_and(|(repo, _)| agent_repos.iter().any(|r| r == repo)))
}

/// The local image ref the AUTHORITATIVE container-start resolver
/// (`AgentContainerImagePolicy::resolve`) produces: `agentforge-agent:<tool>`,
/// hard-coded — it does NOT read `CONTAINER_IMAGE_<TOOL>` (only the separate MCP
/// path's `tool_images` map does). The updater re-tags the pulled GHCR image to
/// THIS ref so the container-start path picks it up. Re-tagging a
/// `CONTAINER_IMAGE_<TOOL>` override instead would silently miss the
/// container-start path — the worker would log "updated" while new agents kept
/// the stale CLI. An operator who pins `CONTAINER_IMAGE_<TOOL>` to a custom ref
/// is opting that ref out of the convention (and out of auto-update); the
/// convention ref is what stays managed here. MUST stay in sync with
/// `AgentContainerImagePolicy::resolve` in `api/src/domain/agent.rs`.
fn local_runtime_ref(tool: &str) -> String {
    format!("agentforge-agent:{tool}")
}

/// Whether a recorded per-tool state warrants a toast vs the previous tick.
/// `updated` always toasts (it is a one-shot landing — the next tick is
/// `up_to_date`). `failed` toasts only when it is new or the error text changed,
/// so a steadily-failing tool toasts once per distinct failure, not every tick.
/// `update_available` (claude local build, detect-only) toasts once per
/// transition — a steady "still available" state does NOT re-toast every tick,
/// but a NEWER npm version while one is already pending does (distinct event).
fn should_toast(prev: Option<&CliToolImageState>, state: &CliToolImageState) -> bool {
    match state.state.as_str() {
        "updated" => true,
        "failed" => {
            prev.map(|p| p.state.as_str()) != Some("failed")
                || prev.and_then(|p| p.last_error.as_deref()) != state.last_error.as_deref()
        }
        "update_available" => {
            prev.map(|p| p.state.as_str()) != Some("update_available")
                || prev.and_then(|p| p.remote_version.as_deref()) != state.remote_version.as_deref()
        }
        _ => false,
    }
}

/// A stable 64-bit discriminator of a failure's error text, so a genuinely
/// DIFFERENT failure gets a distinct toast id (a fresh unread notification)
/// rather than silently updating the one the admin already acked.
fn error_discriminator(err: Option<&str>) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    err.unwrap_or("").hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Build the admin toast frame for a recorded per-tool state. Returns
/// `Some(frame_json)` only for the transitions worth toasting (`updated` =
/// a landed image, `update_available` = a newer claude version to build,
/// `failed` = a check/build error); `None` for `up_to_date`/`pending` so steady
/// state is silent. The string is the exact WS envelope the gateway forwards
/// verbatim and the browser dispatch consumes (`type` + `payload`).
fn build_cli_image_frame(tool: &str, state: &CliToolImageState, unix: i64) -> Option<String> {
    // Dedup key: a new `updated` digest/version is a distinct toast; an
    // `update_available` key embeds the pending version; a `failed` key embeds
    // the error discriminator so a NEW failure surfaces as a fresh unread toast
    // (the browser dedups by id and preserves the read flag on a match).
    let event_id = match state.state.as_str() {
        "updated" => format!(
            "cli-image:{tool}:updated:{}",
            state.remote_digest.as_deref().or(state.remote_version.as_deref()).unwrap_or("unknown")
        ),
        "update_available" => {
            format!("cli-image:{tool}:update_available:{}", state.remote_version.as_deref().unwrap_or("unknown"))
        }
        "failed" => format!("cli-image:{tool}:failed:{}", error_discriminator(state.last_error.as_deref())),
        _ => return None,
    };
    let frame = serde_json::json!({
        "type": CLI_IMAGE_UPDATED_EVENT,
        "payload": {
            "tool": tool,
            "state": state.state,
            "localDigest": state.local_digest,
            "remoteDigest": state.remote_digest,
            "localVersion": state.local_version,
            "remoteVersion": state.remote_version,
            "lastError": state.last_error,
            "eventId": event_id,
            "unix": unix,
        }
    });
    Some(frame.to_string())
}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Split `repo:tag` for the re-tag target, defaulting the tag to `latest`. A `:`
/// before a `/` is a registry `host:port`, not a tag.
fn split_repo_tag(image_ref: &str) -> (String, String) {
    match image_ref.rsplit_once(':') {
        Some((repo, tag)) if !tag.contains('/') => (repo.to_string(), tag.to_string()),
        _ => (image_ref.to_string(), "latest".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_set_excludes_claude_only() {
        let tools: Vec<&str> = pollable_tools().map(|t| t.as_str()).collect();
        assert!(!tools.contains(&"claude"), "claude has no public image and must never be pulled");
        assert!(tools.contains(&"codex") && tools.contains(&"gemini") && tools.contains(&"opencode"));
        assert_eq!(tools.len(), 3);
    }

    fn local_image(tags: &[&str], digests: &[&str]) -> LocalImage {
        LocalImage {
            id: "sha256:image".to_string(),
            repo_tags: tags.iter().map(|s| s.to_string()).collect(),
            repo_digests: digests.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn prune_scoping_targets_only_dangling_own_overlays() {
        let repos = vec!["ghcr.io/x/agent-codex".to_string(), "ghcr.io/x/agent-gemini".to_string()];

        // dangling + our overlay repo → prunable.
        assert!(is_prunable_agent_image(&local_image(&[], &["ghcr.io/x/agent-codex@sha256:old"]), &repos));

        // STILL TAGGED (current image) → never prunable, even if it's ours.
        assert!(!is_prunable_agent_image(
            &local_image(&["ghcr.io/x/agent-codex:latest"], &["ghcr.io/x/agent-codex@sha256:cur"]),
            &repos
        ));
        assert!(!is_prunable_agent_image(
            &local_image(&["agentforge-agent:codex"], &["ghcr.io/x/agent-codex@sha256:cur"]),
            &repos
        ));

        // base image is not a pollable tool → never matched.
        assert!(!is_prunable_agent_image(&local_image(&[], &["ghcr.io/x/agent-base@sha256:b"]), &repos));
        // claude is not pollable → never matched.
        assert!(!is_prunable_agent_image(&local_image(&[], &["ghcr.io/x/agent-claude@sha256:c"]), &repos));
        // another stack's dangling image → never matched.
        assert!(!is_prunable_agent_image(&local_image(&[], &["docker.io/other/app@sha256:z"]), &repos));
        // no repo digests (locally-built, unknown origin) → never matched.
        assert!(!is_prunable_agent_image(&local_image(&[], &[]), &repos));
        // a sibling-PREFIXED repo (agent-codextra vs agent-codex) must NOT match —
        // the scoping is exact-repo equality, not a prefix, so a future fuzzy-match
        // refactor that broadened the blast radius would fail here.
        assert!(!is_prunable_agent_image(&local_image(&[], &["ghcr.io/x/agent-codextra@sha256:s"]), &repos));
        // a malformed repo digest with no `@` → unparseable → never matched.
        assert!(!is_prunable_agent_image(&local_image(&[], &["ghcr.io/x/agent-codex"]), &repos));
    }

    #[test]
    fn agent_overlay_repos_cover_pollable_tools_only() {
        // SAFELY env-free assert on the shape: one entry per pollable tool, none for claude/base.
        let repos = agent_overlay_repos();
        assert_eq!(repos.len(), pollable_tool_names().len());
        assert!(repos.iter().all(|r| r.contains("/agent-")));
        assert!(repos.iter().any(|r| r.ends_with("/agent-codex")));
        assert!(!repos.iter().any(|r| r.ends_with("/agent-claude") || r.ends_with("/agent-base")));
    }

    #[test]
    fn build_remote_ref_shape() {
        assert_eq!(
            build_remote_ref("ghcr.io/wisdoverse/wisdoverse-forge", "latest", "codex"),
            "ghcr.io/wisdoverse/wisdoverse-forge/agent-codex:latest"
        );
        // trailing slash on the registry is normalised
        assert_eq!(build_remote_ref("ghcr.io/org/", "1.2.3", "gemini"), "ghcr.io/org/agent-gemini:1.2.3");
    }

    fn state(s: &str, remote: Option<&str>, err: Option<&str>) -> CliToolImageState {
        CliToolImageState {
            state: s.to_string(),
            update_mode: UPDATE_MODE_REGISTRY.to_string(),
            local_digest: remote.map(str::to_string),
            remote_digest: remote.map(str::to_string),
            last_checked_unix: Some(1_700_000_000),
            last_error: err.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn frame_only_emitted_for_updated_and_failed() {
        assert!(build_cli_image_frame("codex", &state("up_to_date", Some("sha256:x"), None), 1).is_none());
        assert!(build_cli_image_frame("codex", &state("pending", None, None), 1).is_none());
        assert!(build_cli_image_frame("codex", &state("updated", Some("sha256:x"), None), 1).is_some());
        assert!(build_cli_image_frame("codex", &state("failed", None, Some("boom")), 1).is_some());
    }

    #[test]
    fn updated_frame_shape_and_dedup_key() {
        let raw = build_cli_image_frame("codex", &state("updated", Some("sha256:abc"), None), 1_700_000_123).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["type"], CLI_IMAGE_UPDATED_EVENT);
        assert_eq!(v["payload"]["tool"], "codex");
        assert_eq!(v["payload"]["state"], "updated");
        assert_eq!(v["payload"]["remoteDigest"], "sha256:abc");
        assert_eq!(v["payload"]["unix"], 1_700_000_123_i64);
        // updated dedup key embeds the new digest so each version is distinct.
        assert_eq!(v["payload"]["eventId"], "cli-image:codex:updated:sha256:abc");
    }

    fn event_id(raw: &str) -> String {
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        v["payload"]["eventId"].as_str().unwrap().to_string()
    }

    #[test]
    fn failed_dedup_key_is_stable_per_error_and_distinct_across_errors() {
        let raw = build_cli_image_frame("gemini", &state("failed", None, Some("registry timeout")), 1).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["payload"]["state"], "failed");
        assert_eq!(v["payload"]["lastError"], "registry timeout");

        // Same error → same id (the browser updates the existing toast in place).
        let again = build_cli_image_frame("gemini", &state("failed", None, Some("registry timeout")), 2).unwrap();
        assert_eq!(event_id(&raw), event_id(&again));
        // A DIFFERENT error → a distinct id, so a new failure is a fresh unread toast.
        let other = build_cli_image_frame("gemini", &state("failed", None, Some("auth revoked")), 3).unwrap();
        assert_ne!(event_id(&raw), event_id(&other));
        assert!(event_id(&raw).starts_with("cli-image:gemini:failed:"));
    }

    #[test]
    fn should_toast_only_on_real_transitions() {
        let up = state("up_to_date", Some("sha256:x"), None);
        let updated = state("updated", Some("sha256:y"), None);
        let failed_a = state("failed", None, Some("err A"));
        let failed_b = state("failed", None, Some("err B"));

        // landed update always toasts; steady states never do.
        assert!(should_toast(Some(&up), &updated));
        assert!(!should_toast(Some(&updated), &up));
        assert!(!should_toast(Some(&up), &up));

        // a new failure toasts; the SAME failure next tick does not; a changed error does.
        assert!(should_toast(Some(&up), &failed_a));
        assert!(!should_toast(Some(&failed_a), &failed_a));
        assert!(should_toast(Some(&failed_a), &failed_b));
    }

    #[test]
    fn effective_interval_floors_below_minimum() {
        // Below the floor clamps up; at/above passes through. Keeps the
        // admin-reported cadence in lockstep with the worker's real cadence.
        assert_eq!(effective_interval_secs(0), MIN_INTERVAL.as_secs());
        assert_eq!(effective_interval_secs(5), MIN_INTERVAL.as_secs());
        assert_eq!(effective_interval_secs(MIN_INTERVAL.as_secs()), MIN_INTERVAL.as_secs());
        assert_eq!(effective_interval_secs(900), 900);
    }

    #[test]
    fn split_repo_tag_for_runtime_ref() {
        assert_eq!(split_repo_tag("agentforge-agent:codex"), ("agentforge-agent".into(), "codex".into()));
        assert_eq!(split_repo_tag("agentforge-agent"), ("agentforge-agent".into(), "latest".into()));
        // a custom registry override with host:port and no tag
        assert_eq!(split_repo_tag("reg:5000/agent"), ("reg:5000/agent".into(), "latest".into()));
    }

    // ------------------------------------------------------------------
    // claude local build
    // ------------------------------------------------------------------

    #[test]
    fn reported_tools_are_pollable_plus_claude() {
        let reported = reported_tool_names();
        assert!(reported.contains(&"claude"), "claude must be a first-class row in the status report");
        for pollable in pollable_tool_names() {
            assert!(reported.contains(&pollable));
        }
        assert_eq!(reported.len(), pollable_tool_names().len() + 1);
    }

    #[test]
    fn update_mode_is_local_build_only_for_claude() {
        assert_eq!(update_mode_for("claude"), UPDATE_MODE_LOCAL_BUILD);
        assert_eq!(update_mode_for("codex"), UPDATE_MODE_REGISTRY);
        assert_eq!(update_mode_for("gemini"), UPDATE_MODE_REGISTRY);
        assert_eq!(update_mode_for("opencode"), UPDATE_MODE_REGISTRY);
        assert_eq!(update_mode_for("unknown"), UPDATE_MODE_REGISTRY);
    }

    #[test]
    fn claude_outcome_derivation_including_unknown_local() {
        // exact match → up to date.
        assert_eq!(
            derive_claude_outcome(Some("2.1.173".into()), "2.1.173".into()),
            ClaudeOutcome::UpToDate { version: "2.1.173".into() }
        );
        // differing version → update available, local carried.
        assert_eq!(
            derive_claude_outcome(Some("2.1.100".into()), "2.1.173".into()),
            ClaudeOutcome::UpdateAvailable { local: Some("2.1.100".into()), remote: "2.1.173".into() }
        );
        // UNKNOWN local (missing image / missing label) → update available
        // whenever npm answers — never a silent "up to date".
        assert_eq!(
            derive_claude_outcome(None, "2.1.173".into()),
            ClaudeOutcome::UpdateAvailable { local: None, remote: "2.1.173".into() }
        );
        // a "latest" placeholder label (old Makefile fallback) is not a real
        // version, so it counts as drift against a concrete npm version.
        assert_eq!(
            derive_claude_outcome(Some("latest".into()), "2.1.173".into()),
            ClaudeOutcome::UpdateAvailable { local: Some("latest".into()), remote: "2.1.173".into() }
        );
    }

    #[test]
    fn claude_states_record_local_build_mode_and_versions() {
        let up = claude_state_up_to_date(1_700_000_000, None, "2.1.173".into());
        assert_eq!(up.state, "up_to_date");
        assert_eq!(up.update_mode, UPDATE_MODE_LOCAL_BUILD);
        assert_eq!(up.local_version.as_deref(), Some("2.1.173"));
        assert_eq!(up.remote_version.as_deref(), Some("2.1.173"));
        // local-build images have no registry manifest digests by definition.
        assert!(up.local_digest.is_none() && up.remote_digest.is_none());
        assert!(!up.building);

        let avail = claude_state_update_available(1_700_000_100, Some(&up), Some("2.1.100".into()), "2.1.173".into());
        assert_eq!(avail.state, "update_available");
        assert_eq!(avail.update_mode, UPDATE_MODE_LOCAL_BUILD);
        assert_eq!(avail.local_version.as_deref(), Some("2.1.100"));
        assert_eq!(avail.remote_version.as_deref(), Some("2.1.173"));

        let updated = claude_state_updated(1_700_000_200, Some(&avail), "2.1.173".into());
        assert_eq!(updated.state, "updated");
        assert_eq!(updated.last_updated_unix, Some(1_700_000_200));
        assert_eq!(updated.local_version.as_deref(), Some("2.1.173"));

        // failure carries the last-known versions forward (transient blip must
        // not blank the operator's view) and preserves last_updated.
        let failed = claude_state_failed(1_700_000_300, Some(&updated), "npm timeout".into());
        assert_eq!(failed.state, "failed");
        assert_eq!(failed.local_version.as_deref(), Some("2.1.173"));
        assert_eq!(failed.remote_version.as_deref(), Some("2.1.173"));
        assert_eq!(failed.last_updated_unix, Some(1_700_000_200));
        assert_eq!(failed.last_error.as_deref(), Some("npm timeout"));
    }

    fn claude_avail(remote: &str) -> CliToolImageState {
        claude_state_update_available(1_700_000_000, None, Some("2.1.100".into()), remote.into())
    }

    #[test]
    fn should_toast_update_available_exactly_once_per_version() {
        let up = claude_state_up_to_date(1, None, "2.1.100".into());
        let avail = claude_avail("2.1.173");

        // first transition into update_available toasts…
        assert!(should_toast(Some(&up), &avail));
        assert!(should_toast(None, &avail));
        // …but the SAME pending version next tick does NOT re-toast.
        assert!(!should_toast(Some(&avail), &claude_avail("2.1.173")));
        // a NEWER version while one is already pending is a distinct event.
        assert!(should_toast(Some(&avail), &claude_avail("2.1.200")));
        // leaving update_available for up_to_date is silent.
        assert!(!should_toast(Some(&avail), &up));
    }

    #[test]
    fn update_available_frame_shape_and_dedup_key() {
        let raw = build_cli_image_frame("claude", &claude_avail("2.1.173"), 1_700_000_123).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["type"], CLI_IMAGE_UPDATED_EVENT);
        assert_eq!(v["payload"]["tool"], "claude");
        assert_eq!(v["payload"]["state"], "update_available");
        assert_eq!(v["payload"]["localVersion"], "2.1.100");
        assert_eq!(v["payload"]["remoteVersion"], "2.1.173");
        assert_eq!(v["payload"]["eventId"], "cli-image:claude:update_available:2.1.173");
        // up_to_date stays silent for claude exactly like registry tools.
        assert!(build_cli_image_frame("claude", &claude_state_up_to_date(1, None, "2.1.173".into()), 1).is_none());
    }

    #[test]
    fn claude_updated_frame_keys_on_version_when_digests_absent() {
        let updated = claude_state_updated(1_700_000_200, None, "2.1.173".into());
        let raw = build_cli_image_frame("claude", &updated, 1_700_000_200).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        // a local build has no digest — the dedup key falls back to the version
        // so two successive builds of different versions stay distinct toasts.
        assert_eq!(v["payload"]["eventId"], "cli-image:claude:updated:2.1.173");
        assert_eq!(v["payload"]["remoteDigest"], serde_json::Value::Null);
    }

    #[test]
    fn parse_npm_latest_version_from_json() {
        assert_eq!(
            parse_npm_latest_version(r#"{"name":"@anthropic-ai/claude-code","version":"2.1.173"}"#).unwrap(),
            "2.1.173"
        );
        // missing / empty version and invalid JSON are typed errors, not panics.
        assert!(parse_npm_latest_version(r#"{"name":"x"}"#).is_err());
        assert!(parse_npm_latest_version(r#"{"version":""}"#).is_err());
        assert!(parse_npm_latest_version(r#"{"version":42}"#).is_err());
        assert!(parse_npm_latest_version("not json").is_err());
    }

    #[test]
    fn npm_latest_url_encodes_scoped_package_and_honours_mirror() {
        assert_eq!(
            claude_npm_latest_url(&npm_registry_base(None)),
            "https://registry.npmjs.org/@anthropic-ai%2Fclaude-code/latest"
        );
        // mirror override; a trailing slash is normalised.
        assert_eq!(
            claude_npm_latest_url(&npm_registry_base(Some("https://registry.npmmirror.com/"))),
            "https://registry.npmmirror.com/@anthropic-ai%2Fclaude-code/latest"
        );
        // blank override falls back to the default base.
        assert_eq!(npm_registry_base(Some("   ")), DEFAULT_NPM_REGISTRY);
    }

    #[test]
    fn claude_build_slot_is_single_flight() {
        let status = Arc::new(CliImageUpdateStatus::new());
        assert!(!status.claude_build_in_flight());

        let slot = status.try_acquire_claude_build().expect("first acquire");
        assert!(status.claude_build_in_flight());
        // a second concurrent acquire is rejected (the endpoint maps it to 409)…
        assert!(status.try_acquire_claude_build().is_none());

        // …and dropping the guard frees the slot, even after a failed build.
        drop(slot);
        assert!(!status.claude_build_in_flight());
        assert!(status.try_acquire_claude_build().is_some());
    }

    #[test]
    fn claude_dockerfile_mirrors_the_overlay_contract() {
        let df = CLAUDE_OVERLAY_DOCKERFILE;
        // Same base, same install, same post-install ENV/LABEL steps as
        // docker/Dockerfile.agent (CLI_TOOL=claude). If that file changes,
        // change the constant AND this contract together.
        assert!(df.contains("ARG BASE_IMAGE=agentforge-agent-base:latest"));
        assert!(df.contains("FROM ${BASE_IMAGE}"));
        assert!(df.contains(r#"RUN if [ -n "$NPM_REGISTRY" ]; then npm config set registry "$NPM_REGISTRY"; fi"#));
        assert!(df.contains("RUN npm install -g @anthropic-ai/claude-code@${CLI_VERSION}"));
        assert!(df.contains("ENV AGENTFORGE_CLI_TOOL=$CLI_TOOL"));
        assert!(df.contains("ENV AGENTFORGE_CLI_VERSION=$CLI_VERSION"));
        assert!(df.contains("LABEL org.wisdoverse.cli-version=$CLI_VERSION"));
        // the introspection label the version detector reads back.
        assert!(df.contains(&format!("LABEL {CLAUDE_VERSION_LABEL}=$CLI_VERSION")));
        // pinned via build-arg, never baked to a fixed version in the template.
        assert!(df.contains("ARG CLI_VERSION=latest"));
        assert!(df.contains("ARG CLI_TOOL=claude"));
    }
}
