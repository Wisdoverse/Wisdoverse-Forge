//! Agent container control service.
//!
//! Owns the Docker-backed start/stop application flow for container CLI agents:
//! stale reference reconciliation, workspace directory preparation, runtime
//! credential injection, container metadata persistence, orchestration
//! participant updates, and best-effort NATS connection revocation.

use std::sync::Arc;

use agentforge_core::{AgentId, AppConfig, AppResult, TenantScope};
use agentforge_db::entities::Agent;
use agentforge_platform::{ContainerConfig, ContainerState, DockerClient, Mount};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::agent::{
    AgentContainerEnvInput, AgentContainerEnvPolicy, AgentContainerImagePolicy, AgentContainerLifecyclePolicy,
    AgentContainerRuntimePolicy, AgentContainerStartOutcome,
};
use crate::domain::context::{ContextFeature, ContextFeatureFlags};
use crate::repositories::agent::AgentRepository;
use crate::repositories::orchestration::{OrchestrationTaskRepository, ParticipantRepository};
use crate::services::agent::AgentService;
use crate::services::agent_container_credentials::AgentContainerCredentialService;
use crate::services::agent_workspace::{
    CONTAINER_WORKSPACE_ROOT, WorkspaceMountScope, ensure_workspace_belongs_to_org, host_path_for_container_cwd,
    resolve_agent_workspace_paths, workspace_root_from_env,
};
use crate::services::auth_callout::AuthCalloutService;
use crate::services::orchestration::OrchestrationService;

pub(crate) struct AgentContainerControlSettings {
    pub(crate) workspace_root: String,
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
    docker: Option<Arc<DockerClient>>,
    auth_callout: Option<Arc<AuthCalloutService>>,
    pool: PgPool,
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
            docker,
            auth_callout,
            pool,
            AgentContainerControlSettings::from_runtime(workspace_root_from_env(), config, context_features),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        agents: AgentRepository,
        orchestration_tasks: OrchestrationTaskRepository,
        participants: ParticipantRepository,
        credentials: AgentContainerCredentialService,
        docker: Option<Arc<DockerClient>>,
        auth_callout: Option<Arc<AuthCalloutService>>,
        pool: PgPool,
        settings: AgentContainerControlSettings,
    ) -> Self {
        Self {
            agents: AgentService::new(agents),
            orchestration: OrchestrationService::new(orchestration_tasks, participants),
            credentials,
            docker,
            auth_callout,
            pool,
            settings,
        }
    }

    pub(crate) async fn start(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<AgentContainerStartOutcome> {
        let docker = self.docker.as_ref().ok_or_else(AgentContainerRuntimePolicy::control_docker_unavailable)?;
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
        let nats_base_url = AgentContainerEnvPolicy::pick_nats_base_url(
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

        self.credentials
            .inject_runtime_credentials(
                scope,
                agent_id.as_uuid(),
                agent.cli_tool.as_deref(),
                &container_name,
                &mut env,
                &mut mounts,
            )
            .await;

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
        let docker = self.docker.as_ref().ok_or_else(AgentContainerRuntimePolicy::control_docker_unavailable)?;
        let agent = self.agents.get(scope, agent_id).await?;
        let container_id = AgentContainerLifecyclePolicy::running_container_id(agent.container_id.as_deref())?;

        docker.stop_container(container_id, 30).await.map_err(AgentContainerRuntimePolicy::stop_container_failed)?;

        let container_name = format!("agentforge-agent-{}", agent_id.as_uuid());
        self.credentials.cleanup_oauth_mount_best_effort(agent_id.as_uuid(), &container_name).await;

        docker
            .remove_container(container_id, true)
            .await
            .map_err(AgentContainerRuntimePolicy::remove_container_after_stop_failed)?;

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
        ensure_workspace_belongs_to_org(&self.pool, workspace_scope.org_id, workspace_scope.workspace_id).await?;
        let workspace_paths =
            resolve_agent_workspace_paths(&self.settings.workspace_root, workspace_scope, agent.cwd.as_deref())?;
        tokio::fs::create_dir_all(&workspace_paths.host_projects_root).await.map_err(|err| {
            AgentContainerRuntimePolicy::prepare_workspace_failed(workspace_paths.host_projects_root.display(), err)
        })?;
        let container_cwd_host_path =
            host_path_for_container_cwd(&workspace_paths.host_projects_root, &workspace_paths.container_cwd)?;
        tokio::fs::create_dir_all(&container_cwd_host_path).await.map_err(|err| {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_settings_keep_runtime_urls_optional() {
        let settings = AgentContainerControlSettings {
            workspace_root: "/tmp/workspaces".to_string(),
            nats_agent_url: None,
            nats_url: None,
            container_server_url: None,
            codex_default_model: "gpt-5.5".to_string(),
            context_injection_enabled: true,
        };

        assert!(
            AgentContainerEnvPolicy::pick_nats_base_url(
                settings.nats_agent_url.as_deref(),
                settings.nats_url.as_deref()
            )
            .is_none()
        );
    }
}
