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
//! only Docker. Notification in this increment is via a structured `warn!` event
//! + Prometheus metrics (an admin status API + UI are a tracked follow-up).

use std::env;
use std::sync::Arc;
use std::time::Duration;

use agentforge_core::CliToolKind;
use agentforge_platform::DockerClient;
use agentforge_platform::container::PlatformError;
use tokio::sync::watch;

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

pub struct CliImageUpdater {
    docker: Arc<DockerClient>,
    interval: Duration,
}

impl CliImageUpdater {
    pub fn new(docker: Arc<DockerClient>) -> Self {
        Self { docker, interval: DEFAULT_INTERVAL }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = if interval < MIN_INTERVAL {
            tracing::warn!(
                requested_secs = interval.as_secs(),
                floor_secs = MIN_INTERVAL.as_secs(),
                "cli image auto-update interval below floor; clamping to avoid hammering the registry"
            );
            MIN_INTERVAL
        } else {
            interval
        };
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
    /// hiccup never aborts the others.
    async fn poll_once(&self) {
        for tool in pollable_tools() {
            if let Err(err) = self.check_and_update(tool).await {
                tracing::warn!(tool = tool.as_str(), error = %err, "cli image update check failed");
                metrics::counter!("agentforge_cli_image_pull_total", "tool" => tool.as_str(), "result" => "failed")
                    .increment(1);
            }
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
            return Ok(UpdateOutcome::UpToDate);
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
    UpToDate,
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
    let registry = non_empty_env("AGENT_REGISTRY").unwrap_or_else(|| DEFAULT_REGISTRY.to_string());
    let tag = non_empty_env("AGENT_CLI_IMAGE_TAG").unwrap_or_else(|| DEFAULT_CLI_IMAGE_TAG.to_string());
    build_remote_ref(&registry, &tag, tool)
}

fn build_remote_ref(registry: &str, tag: &str, tool: &str) -> String {
    format!("{}/agent-{tool}:{tag}", registry.trim_end_matches('/'))
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

    #[test]
    fn build_remote_ref_shape() {
        assert_eq!(
            build_remote_ref("ghcr.io/wisdoverse/wisdoverse-forge", "latest", "codex"),
            "ghcr.io/wisdoverse/wisdoverse-forge/agent-codex:latest"
        );
        // trailing slash on the registry is normalised
        assert_eq!(build_remote_ref("ghcr.io/org/", "1.2.3", "gemini"), "ghcr.io/org/agent-gemini:1.2.3");
    }

    #[test]
    fn split_repo_tag_for_runtime_ref() {
        assert_eq!(split_repo_tag("agentforge-agent:codex"), ("agentforge-agent".into(), "codex".into()));
        assert_eq!(split_repo_tag("agentforge-agent"), ("agentforge-agent".into(), "latest".into()));
        // a custom registry override with host:port and no tag
        assert_eq!(split_repo_tag("reg:5000/agent"), ("reg:5000/agent".into(), "latest".into()));
    }
}
