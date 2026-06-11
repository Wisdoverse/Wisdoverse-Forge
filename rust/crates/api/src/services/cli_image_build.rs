//! Operator-initiated claude local image build (deployment-global, admin-gated).
//!
//! `claude` has no public registry overlay image — its license requires a
//! self-build — so unlike the registry tools (pulled + re-tagged by the
//! auto-updater) it is BUILT server-side: `POST /admin/cli-images/claude/build`
//! answers `202 { started, targetVersion }` immediately and runs the docker
//! build as a background task. Progress and the final `updated`/`failed`
//! outcome land in the shared [`CliImageUpdateStatus`] snapshot the status
//! endpoint serves (plus the `broadcast.admin.cli_image` toast), exactly like
//! an auto-updater sweep tick.
//!
//! Independence: the build path holds its own Docker handle, status handle, and
//! NATS sink — it does NOT require the (default-off) auto-update poller task to
//! be running. Single-flight: the claude build slot lives on
//! `CliImageUpdateStatus` and is shared with the sweep's auto-build, so a
//! manual build and an auto-build can never run concurrently (the loser gets
//! 409 here / a skipped tick there).
//!
//! Safety: image-level only — `ensure base` + `docker build` + `tag`; never a
//! container operation, so running agents are untouched and
//! `platform/security.rs` is not involved.

use std::sync::Arc;

use agentforge_core::{AppConfig, AppResult};
use agentforge_jobs::{ClaudeBuildContext, CliImageUpdateStatus, execute_claude_build, fetch_claude_latest_version};
use agentforge_platform::DockerClient;

pub(crate) use crate::domain::cli_image::{
    LocalBuildToolPolicy, claude_build_in_progress_error, claude_build_runtime_unavailable_error,
    claude_version_lookup_failed_error, cli_image_build_response,
};

pub struct CliImageBuildService {
    status: Arc<CliImageUpdateStatus>,
    /// `None` when this deployment has no Docker runtime — a build request then
    /// fails fast with a clear 503 instead of a doomed background task.
    docker: Option<Arc<DockerClient>>,
    /// NATS sink for the completion toast; `None` leaves toasts off (the status
    /// snapshot still records the outcome).
    event_sink: Option<async_nats::Client>,
    /// `CLI_IMAGE_NPM_REGISTRY` override for the version lookup AND the
    /// `NPM_REGISTRY` build-arg of the generated Dockerfile.
    npm_registry: Option<String>,
}

impl CliImageBuildService {
    pub(crate) fn from_runtime(
        status: Arc<CliImageUpdateStatus>,
        docker: Option<Arc<DockerClient>>,
        event_sink: Option<async_nats::Client>,
        config: &AppConfig,
    ) -> Self {
        Self { status, docker, event_sink, npm_registry: config.cli_image_npm_registry.clone() }
    }

    /// Start one claude build: resolve npm `latest`, claim the single-flight
    /// slot, and spawn the build task. Returns the target version for the 202
    /// body. Errors (all typed in the domain): 422 not-buildable tool, 503
    /// runtime down or npm unreachable (nothing was started), 409 build already
    /// in flight.
    pub async fn start_build(&self, tool: &str) -> AppResult<String> {
        // Defense-in-depth: re-assert the allowlist here, never trusting only
        // the route (registry tools / unknown values are rejected with 422).
        LocalBuildToolPolicy::ensure_local_buildable(tool)?;

        let Some(docker) = self.docker.clone() else {
            return Err(claude_build_runtime_unavailable_error());
        };

        // Claim the slot BEFORE the npm lookup so two racing clicks can't both
        // resolve a version and double-build; the guard releases on any early
        // return below, so a failed lookup never wedges future builds.
        let Some(slot) = self.status.try_acquire_claude_build() else {
            return Err(claude_build_in_progress_error());
        };

        let target_version = fetch_claude_latest_version(self.npm_registry.as_deref())
            .await
            .map_err(|detail| claude_version_lookup_failed_error(&detail))?;

        let ctx = ClaudeBuildContext {
            docker,
            status: self.status.clone(),
            event_sink: self.event_sink.clone(),
            npm_registry: self.npm_registry.clone(),
        };
        let version = target_version.clone();
        tokio::spawn(async move {
            execute_claude_build(slot, ctx, &version).await;
        });

        Ok(target_version)
    }
}
