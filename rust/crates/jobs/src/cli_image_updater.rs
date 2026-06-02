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
//! under license), so it is excluded from the poll set. The worker is
//! deployment-global: it holds no tenant scope and queries no org-scoped table,
//! only Docker. Notification is via a structured `warn!` event, Prometheus
//! metrics, a read-only admin status API (`GET /admin/cli-images`), and — when
//! NATS is configured — a live admin WebSocket toast on `broadcast.admin.cli_image`.
//! The worker can also prune superseded overlays (`with_prune`, default-off).
//! Warm-pool drain-on-drift remains a tracked follow-up (`platform/pool.rs` is
//! still dormant).

use std::collections::BTreeMap;
use std::env;
use std::sync::Arc;
use std::time::Duration;

use agentforge_core::CliToolKind;
use agentforge_core::broadcast_protocol::{ADMIN_CLI_IMAGE_SUBJECT, CLI_IMAGE_UPDATED_EVENT};
use agentforge_platform::container::PlatformError;
use agentforge_platform::{DockerClient, LocalImage, RemoveOutcome};
use tokio::sync::{RwLock, watch};

/// Per-tool image-update state, surfaced read-only by `GET /admin/cli-images`.
#[derive(Debug, Clone, Default)]
pub struct CliToolImageState {
    /// `up_to_date` | `updated` | `failed`.
    pub state: String,
    pub local_digest: Option<String>,
    pub remote_digest: Option<String>,
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

/// Shared in-memory snapshot of the latest per-tool update state + prune result.
/// Written by the worker each tick, read by the admin status endpoint.
/// Deployment-global (image state is per host, not per org), so no tenant scope.
#[derive(Debug, Default)]
pub struct CliImageUpdateStatus {
    tools: RwLock<BTreeMap<String, CliToolImageState>>,
    prune: RwLock<CliImagePruneSummary>,
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

    async fn record(&self, tool: &str, state: CliToolImageState) {
        self.tools.write().await.insert(tool.to_string(), state);
    }

    async fn record_prune(&self, summary: CliImagePruneSummary) {
        *self.prune.write().await = summary;
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
/// so the updater must never attempt to pull it.
fn pollable_tools() -> impl Iterator<Item = CliToolKind> {
    CliToolKind::ALL.into_iter().filter(|tool| !matches!(tool, CliToolKind::Claude))
}

/// Canonical pollable tool names (`CliToolKind::ALL` minus `claude`). The single
/// source of truth shared by the worker's poll loop and the admin status
/// projection, so the endpoint can never list a tool the worker won't poll.
pub fn pollable_tool_names() -> Vec<&'static str> {
    pollable_tools().map(|tool| tool.as_str()).collect()
}

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
}

impl CliImageUpdater {
    pub fn new(docker: Arc<DockerClient>, status: Arc<CliImageUpdateStatus>) -> Self {
        Self { docker, status, interval: DEFAULT_INTERVAL, event_sink: None, prune_enabled: false }
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
                    local_digest: Some(digest.clone()),
                    remote_digest: Some(digest),
                    last_checked_unix: Some(now),
                    last_updated_unix: prev.and_then(|p| p.last_updated_unix),
                    last_error: None,
                },
                Ok(UpdateOutcome::Updated { to, .. }) => CliToolImageState {
                    state: "updated".to_string(),
                    local_digest: Some(to.clone()),
                    remote_digest: Some(to),
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
                        // carry forward the last-known digests so a transient
                        // registry blip doesn't blank the operator's view.
                        local_digest: prev.and_then(|p| p.local_digest.clone()),
                        remote_digest: prev.and_then(|p| p.remote_digest.clone()),
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

        // After refreshing every tool, optionally reclaim superseded (dangling)
        // agent overlay images. Default-off; image-level only (never a
        // container op), and scoped + reference-guarded so it can never touch a
        // live image or another stack's layers.
        if self.prune_enabled {
            let summary = self.prune_orphans(now).await;
            self.status.record_prune(summary).await;
        }
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

    /// Publish an admin toast frame for `updated`/`failed` transitions. Best
    /// effort: a publish failure is logged and never affects the update result
    /// or the status snapshot. No-op when no NATS sink is attached, and silent
    /// for `up_to_date`/`pending` (those would be noise).
    async fn emit_toast(&self, tool: &str, state: &CliToolImageState, unix: i64) {
        let Some(client) = self.event_sink.as_ref() else {
            return;
        };
        let Some(frame) = build_cli_image_frame(tool, state, unix) else {
            return;
        };
        if let Err(err) = client.publish(ADMIN_CLI_IMAGE_SUBJECT, frame.into_bytes().into()).await {
            tracing::warn!(tool, error = %err, "failed to publish cli image admin toast (non-fatal)");
        }
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
    for tool in pollable_tools() {
        metrics::counter!("agentforge_cli_image_pull_total", "tool" => tool.as_str(), "result" => "success")
            .increment(0);
        metrics::counter!("agentforge_cli_image_drift_detected_total", "tool" => tool.as_str()).increment(0);
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
fn should_toast(prev: Option<&CliToolImageState>, state: &CliToolImageState) -> bool {
    match state.state.as_str() {
        "updated" => true,
        "failed" => {
            prev.map(|p| p.state.as_str()) != Some("failed")
                || prev.and_then(|p| p.last_error.as_deref()) != state.last_error.as_deref()
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
/// a landed image, `failed` = a check error); `None` for `up_to_date`/`pending`
/// so steady state is silent. The string is the exact WS envelope the gateway
/// forwards verbatim and the browser dispatch consumes (`type` + `payload`).
fn build_cli_image_frame(tool: &str, state: &CliToolImageState, unix: i64) -> Option<String> {
    if state.state != "updated" && state.state != "failed" {
        return None;
    }
    // Dedup key: a new `updated` digest is a distinct toast; a `failed` key
    // embeds the error discriminator so a NEW failure surfaces as a fresh unread
    // toast (the browser dedups by id and preserves the read flag on a match).
    let event_id = if state.state == "updated" {
        format!("cli-image:{tool}:updated:{}", state.remote_digest.as_deref().unwrap_or("unknown"))
    } else {
        format!("cli-image:{tool}:failed:{}", error_discriminator(state.last_error.as_deref()))
    };
    let frame = serde_json::json!({
        "type": CLI_IMAGE_UPDATED_EVENT,
        "payload": {
            "tool": tool,
            "state": state.state,
            "localDigest": state.local_digest,
            "remoteDigest": state.remote_digest,
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
            local_digest: remote.map(str::to_string),
            remote_digest: remote.map(str::to_string),
            last_checked_unix: Some(1_700_000_000),
            last_updated_unix: None,
            last_error: err.map(str::to_string),
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
}
