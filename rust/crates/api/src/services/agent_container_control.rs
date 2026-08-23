//! Agent container control service.
//!
//! Owns the Docker-backed start/stop application flow for container CLI agents:
//! stale reference reconciliation, workspace directory preparation, runtime
//! credential injection, container metadata persistence, orchestration
//! participant updates, and best-effort NATS connection revocation.

use std::path::Path;
use std::sync::Arc;

use agentforge_core::{AgentId, AppConfig, AppResult, TenantScope};
use agentforge_db::entities::Agent;
use agentforge_platform::{ContainerConfig, ContainerState, DockerClient, Mount};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::agent::{
    AgentContainerEnvInput, AgentContainerEnvPolicy, AgentContainerImagePolicy, AgentContainerRuntimePolicy,
    AgentContainerStartOutcome, AgentContainerStopOutcome, ContainerAgent,
};
use crate::domain::context::{ContextFeature, ContextFeatureFlags};
use crate::repositories::agent::AgentRepository;
use crate::repositories::orchestration::{OrchestrationTaskRepository, ParticipantRepository};
use crate::services::agent::AgentService;
use crate::services::agent_container_credentials::AgentContainerCredentialService;
use crate::services::agent_workspace::{
    AgentWorkspaceService, CONTAINER_WORKSPACE_ROOT, WorkspaceMountScope, ensure_agent_working_directory,
    ensure_shared_workspace_directory, host_path_for_container_cwd, resolve_agent_workspace_paths,
    workspace_root_from_env,
};
use crate::services::auth_callout::AuthCalloutService;
use crate::services::orchestration::OrchestrationService;

pub(crate) struct AgentContainerControlSettings {
    pub(crate) workspace_root: String,
    pub(crate) nats_container_url: Option<String>,
    pub(crate) nats_agent_url: Option<String>,
    pub(crate) nats_url: Option<String>,
    pub(crate) container_server_url: Option<String>,
    pub(crate) codex_default_model: String,
    pub(crate) context_injection_enabled: bool,
}

impl AgentContainerControlSettings {
    pub(crate) fn from_runtime(
        workspace_root: String,
        config: &AppConfig,
        context_features: ContextFeatureFlags,
    ) -> Self {
        Self {
            workspace_root,
            nats_container_url: config.nats_container_url.clone(),
            nats_agent_url: config.nats_agent_url.clone(),
            nats_url: config.nats_url.clone(),
            container_server_url: config.container_server_url.clone(),
            codex_default_model: config.codex_default_model.clone(),
            context_injection_enabled: context_features.enabled(ContextFeature::Injection),
        }
    }
}

pub(crate) struct AgentContainerControlService {
    agents: AgentService,
    orchestration: OrchestrationService,
    credentials: AgentContainerCredentialService,
    workspaces: AgentWorkspaceService,
    docker: Option<Arc<DockerClient>>,
    auth_callout: Option<Arc<AuthCalloutService>>,
    settings: AgentContainerControlSettings,
}

impl AgentContainerControlService {
    pub(crate) fn from_runtime(
        pool: PgPool,
        config: &AppConfig,
        context_features: ContextFeatureFlags,
        encryption_key: Option<[u8; 32]>,
        docker: Option<Arc<DockerClient>>,
        auth_callout: Option<Arc<AuthCalloutService>>,
    ) -> Self {
        Self::new(
            AgentRepository::new(pool.clone()),
            OrchestrationTaskRepository::new(pool.clone()),
            ParticipantRepository::new(pool.clone()),
            AgentContainerCredentialService::from_pool_and_app_config(pool.clone(), encryption_key, config),
            AgentWorkspaceService::from_pool(pool.clone()),
            docker,
            auth_callout,
            AgentContainerControlSettings::from_runtime(workspace_root_from_env(), config, context_features),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        agents: AgentRepository,
        orchestration_tasks: OrchestrationTaskRepository,
        participants: ParticipantRepository,
        credentials: AgentContainerCredentialService,
        workspaces: AgentWorkspaceService,
        docker: Option<Arc<DockerClient>>,
        auth_callout: Option<Arc<AuthCalloutService>>,
        settings: AgentContainerControlSettings,
    ) -> Self {
        Self {
            agents: AgentService::new(agents),
            orchestration: OrchestrationService::new(orchestration_tasks, participants),
            credentials,
            workspaces,
            docker,
            auth_callout,
            settings,
        }
    }

    pub(crate) async fn start(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<AgentContainerStartOutcome> {
        let docker = self.docker.as_ref().ok_or_else(AgentContainerRuntimePolicy::control_docker_unavailable)?;
        // Typestate check: reject host_cli/api agents before any Docker I/O.
        let aggregate = self.agents.find_aggregate(scope, agent_id.as_uuid()).await?;
        let kind = aggregate.runtime_kind();
        ContainerAgent::try_from(aggregate).map_err(|r| {
            metrics::counter!(
                "agents_lifecycle_rejected_total",
                "runtime_kind" => kind.as_str(),
                "action" => "start"
            )
            .increment(1);
            r.into_app_error("Start")
        })?;
        // Load the full Agent entity for the data-rich container provisioning path.
        // AgentAggregate only carries identity columns; cli_tool/model/cwd/name
        // come from the full row.
        let agent = self.agents.get(scope, agent_id).await?;

        if let Some(container_id) = &agent.container_id {
            match docker.inspect_container(container_id).await {
                Ok(info) if info.status == ContainerState::Running => {
                    self.register_started_agent_participant_best_effort(scope, &agent).await;
                    return Ok(AgentContainerStartOutcome::already_running(container_id));
                }
                Ok(info) => {
                    tracing::info!(
                        agent_id = %agent_id,
                        container_id = %container_id,
                        status = ?info.status,
                        "agent container is not running; replacing it"
                    );
                    if let Err(cleanup_err) = docker.remove_container(container_id, true).await {
                        tracing::warn!(
                            error = %cleanup_err,
                            container_id = %container_id,
                            "failed to remove non-running existing container"
                        );
                    }
                    self.agents.clear_container(scope, agent_id).await?;
                    self.mark_participant_offline_best_effort(scope, agent_id).await;
                }
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        agent_id = %agent_id,
                        container_id = %container_id,
                        "agent container reference is stale; creating a replacement"
                    );
                    self.agents.clear_container(scope, agent_id).await?;
                    self.mark_participant_offline_best_effort(scope, agent_id).await;
                }
            }
        }

        let image = AgentContainerImagePolicy::resolve_for_start(agent.cli_tool.as_deref(), agent.model.as_deref())?;
        let container_name = format!("agentforge-agent-{}", agent_id.as_uuid());
        let hmac_secret = Uuid::new_v4().to_string();
        let nats_connect_password = Uuid::new_v4().to_string();
        let workspace_paths = self.prepare_workspace(scope, &agent).await?;
        let workspace_host_path = workspace_paths.host_projects_root.to_string_lossy().into_owned();
        let nats_base_url = AgentContainerEnvPolicy::pick_container_nats_base_url(
            self.settings.nats_container_url.as_deref(),
            self.settings.nats_agent_url.as_deref(),
            self.settings.nats_url.as_deref(),
        );
        let mut env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: agent_id.as_uuid(),
            org_id: scope.org_id().as_uuid(),
            cli_tool: agent.cli_tool.as_deref(),
            cli_model: agent.model.as_deref(),
            codex_default_model: Some(self.settings.codex_default_model.as_str()),
            nats_base_url: nats_base_url.as_deref(),
            nats_connect_password: &nats_connect_password,
            container_server_url: self.settings.container_server_url.as_deref(),
            workspace_host_path: Some(&workspace_host_path),
            hmac_secret: &hmac_secret,
            context_injection_enabled: self.settings.context_injection_enabled,
        });
        let mut mounts: Vec<Mount> =
            vec![Mount { source: workspace_host_path, target: CONTAINER_WORKSPACE_ROOT.to_string(), read_only: false }];

        // Fail closed before Docker create or participant registration when
        // the selected Container CLI has no usable runtime credentials.
        self.credentials
            .inject_runtime_credentials(
                scope,
                agent_id.as_uuid(),
                agent.cli_tool.as_deref(),
                &container_name,
                &mut env,
                &mut mounts,
            )
            .await?;

        let config = ContainerConfig {
            image: image.clone(),
            name: Some(container_name),
            working_dir: Some(workspace_paths.container_cwd),
            env,
            labels: [
                ("agentforge.agent_id".to_string(), agent_id.as_uuid().to_string()),
                ("agentforge.org_id".to_string(), scope.org_id().as_uuid().to_string()),
            ]
            .into_iter()
            .collect(),
            resources: Default::default(),
            network: Some("agentforge-agents".to_string()),
            mounts,
            privileged: false,
            host_pid: false,
            tty: true,
            open_stdin: true,
            attach_stdin: true,
            attach_stdout: true,
            attach_stderr: true,
        };

        let container_id = docker.create_container(config).await.map_err(|err| {
            AgentContainerRuntimePolicy::create_container_failed(
                &image,
                agent.cli_tool.as_deref(),
                err.is_missing_image(),
                err,
            )
        })?;

        if let Err(err) =
            self.agents.set_container(scope, agent_id, &container_id, &hmac_secret, &nats_connect_password).await
        {
            if let Err(cleanup_err) = docker.remove_container(&container_id, true).await {
                tracing::warn!(
                    error = %cleanup_err,
                    container_id = %container_id,
                    "failed to clean up container after DB persist failure"
                );
            }
            return Err(err);
        }

        if let Err(err) = docker.start_container(&container_id).await {
            if let Err(cleanup_err) = docker.remove_container(&container_id, true).await {
                tracing::warn!(
                    error = %cleanup_err,
                    container_id = %container_id,
                    "failed to clean up container after start failure"
                );
            }
            if let Err(clear_err) = self.agents.clear_container(scope, agent_id).await {
                tracing::warn!(
                    error = ?clear_err,
                    agent_id = %agent_id,
                    "failed to clear container metadata after start failure"
                );
            }
            return Err(AgentContainerRuntimePolicy::start_container_failed(err).into());
        }

        self.register_started_agent_participant_best_effort(scope, &agent).await;
        Ok(AgentContainerStartOutcome::started(container_id))
    }

    pub(crate) async fn stop(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        match self.stop_with_outcome(scope, agent_id).await? {
            AgentContainerStopOutcome::Stopped => Ok(()),
            AgentContainerStopOutcome::StillRunning => {
                Err(AgentContainerRuntimePolicy::container_still_running_after_stop().into())
            }
            AgentContainerStopOutcome::Unconfirmed => {
                Err(AgentContainerRuntimePolicy::stop_post_condition_unverified().into())
            }
        }
    }

    /// Stop and remove an agent's container, idempotently and with a verified
    /// post-condition, then reconcile the DB row to match reality.
    ///
    /// FAANG-grade rollout safety: the previous implementation chained
    /// `stop_container` → `remove_container` → `clear_container` with `?`, so
    /// the first error aborted and left `agents.container_id` advertising a
    /// container that might be gone, half-removed, or still running. Here each
    /// Docker mutation is idempotent (a `NotFound`/404 means the desired
    /// post-condition — the container's absence — already holds), each gets one
    /// bounded retry to absorb transient daemon hiccups, and a final `inspect`
    /// confirms the real state. The DB row is cleared whenever the container is
    /// confirmed absent (including "already gone"); it is left untouched only
    /// when the container is confirmed still running or its state is
    /// unverifiable, in which case the reconcile backstop converges it later.
    pub(crate) async fn stop_with_outcome(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
    ) -> AppResult<AgentContainerStopOutcome> {
        let docker = self.docker.as_ref().ok_or_else(AgentContainerRuntimePolicy::control_docker_unavailable)?;
        let aggregate = self.agents.find_aggregate(scope, agent_id.as_uuid()).await?;
        let kind = aggregate.runtime_kind();
        let container = ContainerAgent::try_from(aggregate).map_err(|r| {
            metrics::counter!(
                "agents_lifecycle_rejected_total",
                "runtime_kind" => kind.as_str(),
                "action" => "stop"
            )
            .increment(1);
            r.into_app_error("Stop")
        })?;
        let inner = container.inner();

        // No container reference: nothing to stop. The row is already in the
        // desired post-condition, so just reconcile defensively and report it.
        let Some(container_id) = inner.container_id.as_deref() else {
            self.reconcile_stopped_agent(scope, agent_id).await?;
            return Ok(AgentContainerStopOutcome::Stopped);
        };

        // Step 1: stop (idempotent + one retry). A NotFound means it is already
        // gone, which satisfies the post-condition.
        idempotent_container_op(|| docker.stop_container(container_id, 30)).await;

        let container_name = format!("agentforge-agent-{}", agent_id.as_uuid());
        self.credentials.cleanup_oauth_mount_best_effort(agent_id.as_uuid(), &container_name).await;

        // Step 2: remove (idempotent + one retry).
        idempotent_container_op(|| docker.remove_container(container_id, true)).await;

        // Step 3: verify the post-condition by inspecting. NotFound = absent
        // (success); Ok = still present (genuinely still running); other daemon
        // error = unverifiable.
        let outcome = match docker.inspect_container(container_id).await {
            Err(err) if err.is_not_found() => AgentContainerStopOutcome::Stopped,
            Ok(info) => {
                tracing::warn!(
                    agent_id = %agent_id,
                    container_id = %container_id,
                    status = ?info.status,
                    "agent container still present after stop+remove; leaving DB row for reconciliation"
                );
                AgentContainerStopOutcome::StillRunning
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    agent_id = %agent_id,
                    container_id = %container_id,
                    "could not verify agent container stopped; leaving DB row for reconciliation"
                );
                AgentContainerStopOutcome::Unconfirmed
            }
        };

        // Step 4: reconcile the DB only when the container is confirmed absent.
        if outcome == AgentContainerStopOutcome::Stopped {
            self.reconcile_stopped_agent(scope, agent_id).await?;
        }
        Ok(outcome)
    }

    /// Reconcile backstop primitive: if `container_id` is absent from Docker,
    /// clear the agent's stale reference and mark it offline; returns `true` when
    /// it reconciled. A still-present container, an inspect error (transient
    /// daemon issue — try again next sweep), or no Docker runtime all leave the
    /// row untouched and return `false`. This converges rows left behind by an
    /// `Unconfirmed`/`StillRunning` stop once the container actually goes away.
    pub(crate) async fn reconcile_agent_if_container_absent(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        container_id: &str,
    ) -> AppResult<bool> {
        let Some(docker) = self.docker.as_ref() else { return Ok(false) };
        match docker.inspect_container(container_id).await {
            Err(err) if err.is_not_found() => {
                self.reconcile_stopped_agent(scope, agent_id).await?;
                tracing::info!(
                    agent_id = %agent_id,
                    container_id = %container_id,
                    "reconcile: cleared stale container reference for a vanished container"
                );
                Ok(true)
            }
            Ok(_) => Ok(false),
            Err(err) => {
                tracing::debug!(
                    error = %err,
                    agent_id = %agent_id,
                    container_id = %container_id,
                    "reconcile: inspect failed; leaving reference for the next sweep"
                );
                Ok(false)
            }
        }
    }

    /// Drive the DB + side-effect cleanup for an agent whose container is gone:
    /// clear the container reference, mark the participant offline, and revoke
    /// the agent's connection. Idempotent — safe to call from both the stop path
    /// and the reconcile backstop.
    async fn reconcile_stopped_agent(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        self.agents.clear_container(scope, agent_id).await?;
        self.mark_participant_offline_best_effort(scope, agent_id).await;
        self.revoke_agent_connection(agent_id.as_uuid()).await;
        Ok(())
    }

    async fn prepare_workspace(
        &self,
        scope: &TenantScope,
        agent: &Agent,
    ) -> AppResult<crate::services::agent_workspace::AgentWorkspacePaths> {
        let workspace_scope =
            WorkspaceMountScope { org_id: scope.org_id().as_uuid(), workspace_id: agent.workspace_id.as_uuid() };
        self.workspaces.ensure_workspace_belongs_to_org(workspace_scope.org_id, workspace_scope.workspace_id).await?;
        let workspace_paths =
            resolve_agent_workspace_paths(&self.settings.workspace_root, workspace_scope, agent.cwd.as_deref())?;
        let workspace_root = Path::new(&self.settings.workspace_root);
        ensure_shared_workspace_directory(workspace_root, &workspace_paths.host_projects_root).map_err(|err| {
            AgentContainerRuntimePolicy::prepare_workspace_failed(workspace_paths.host_projects_root.display(), err)
        })?;
        let container_cwd_host_path =
            host_path_for_container_cwd(&workspace_paths.host_projects_root, &workspace_paths.container_cwd)?;
        ensure_agent_working_directory(workspace_root, &container_cwd_host_path).map_err(|err| {
            AgentContainerRuntimePolicy::prepare_working_directory_failed(container_cwd_host_path.display(), err)
        })?;
        Ok(workspace_paths)
    }

    async fn mark_participant_offline_best_effort(&self, scope: &TenantScope, agent_id: AgentId) {
        if let Err(err) = self.orchestration.mark_participant_offline(scope, agent_id).await {
            tracing::warn!(error = ?err, %agent_id, "failed to mark participant offline");
        }
    }

    async fn register_started_agent_participant_best_effort(&self, scope: &TenantScope, agent: &Agent) {
        if let Err(err) = self.register_started_agent_participant(scope, agent).await {
            tracing::warn!(
                error = ?err,
                agent_id = %agent.id,
                "started agent container before participant registration completed"
            );
        }
    }

    async fn register_started_agent_participant(&self, scope: &TenantScope, agent: &Agent) -> AppResult<()> {
        let fallback_name = format!("agent-{}", &agent.id.as_uuid().to_string()[..8]);
        let name =
            agent.name.as_deref().map(str::trim).filter(|name| !name.is_empty()).unwrap_or(fallback_name.as_str());
        let capabilities: Vec<String> = agent.cli_tool.clone().into_iter().collect();

        self.orchestration.register_participant(scope, agent.id, name, &capabilities).await?;
        self.orchestration.participant_heartbeat(scope, agent.id).await?;
        Ok(())
    }

    async fn revoke_agent_connection(&self, agent_id: Uuid) {
        match self.auth_callout.as_ref() {
            Some(callout) => callout.revoke(agent_id).await,
            None => tracing::info!(
                %agent_id,
                "stop_agent: auth callout disabled — revocation falls back to JWT TTL (dev profile or NATS unconfigured)"
            ),
        }
    }
}

/// Run an idempotent Docker container mutation whose desired post-condition is
/// "the container is absent". A `NotFound`/404 is treated as success (the
/// post-condition already holds), and a transient daemon error gets one bounded
/// retry after a short backoff. Errors are intentionally not propagated: the
/// caller verifies the real state with a follow-up `inspect`, so a swallowed
/// error here cannot mask an unconverged state — it only avoids aborting the
/// stop sequence midway (the original non-atomic bug).
async fn idempotent_container_op<F, Fut>(mut op: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), agentforge_platform::PlatformError>>,
{
    match op().await {
        Ok(()) => {}
        Err(err) if err.is_not_found() => {}
        Err(first) => {
            tracing::warn!(error = %first, "transient docker error during stop; retrying once");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            match op().await {
                Ok(()) => {}
                Err(err) if err.is_not_found() => {}
                Err(second) => {
                    tracing::warn!(
                        error = %second,
                        "docker op still failing after retry; the post-condition inspect decides the outcome"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_settings_keep_runtime_urls_optional() {
        let settings = AgentContainerControlSettings {
            workspace_root: "/tmp/workspaces".to_string(),
            nats_container_url: None,
            nats_agent_url: None,
            nats_url: None,
            container_server_url: None,
            codex_default_model: "gpt-5.5".to_string(),
            context_injection_enabled: true,
        };

        assert!(
            AgentContainerEnvPolicy::pick_container_nats_base_url(
                settings.nats_container_url.as_deref(),
                settings.nats_agent_url.as_deref(),
                settings.nats_url.as_deref()
            )
            .is_none()
        );
    }
}
