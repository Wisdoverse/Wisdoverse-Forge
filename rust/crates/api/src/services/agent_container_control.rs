//! Agent container control service.
//!
//! Owns the Docker-backed start/stop application flow for container CLI agents:
//! stale reference reconciliation, workspace directory preparation, runtime
//! credential injection, container metadata persistence, orchestration
//! participant updates, and best-effort NATS connection revocation.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use agentforge_core::{AgentId, AppConfig, AppError, AppResult, CliToolKind, ErrorKind, RuntimeKind, TenantScope};
use agentforge_db::entities::Agent;
use agentforge_platform::{ContainerConfig, ContainerInfo, ContainerState, DockerClient, LocalImageIdentity, Mount};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::agent::{
    AgentContainerEnvInput, AgentContainerEnvPolicy, AgentContainerImageIdentity, AgentContainerImagePolicy,
    AgentContainerRuntimePolicy, AgentContainerStartOutcome, AgentContainerStopOutcome, ContainerAgent,
};
use crate::domain::context::{ContextFeature, ContextFeatureFlags};
use crate::repositories::agent::AgentRepository;
use crate::repositories::orchestration::{OrchestrationTaskRepository, ParticipantRepository};
use crate::services::admin::PlatformAdminAuthority;
use crate::services::agent::AgentService;
use crate::services::agent_container_credentials::AgentContainerCredentialService;
use crate::services::agent_workspace::{
    AgentWorkspaceService, CONTAINER_WORKSPACE_ROOT, WorkspaceMountScope, ensure_agent_working_directory,
    ensure_shared_workspace_directory, host_path_for_container_cwd, resolve_agent_workspace_paths,
    workspace_root_from_env,
};
use crate::services::auth_callout::AuthCalloutService;
use crate::services::container_image_config::{
    capture_container_image_identity, configured_cli_images, recorded_image_trust_is_acceptable,
};
use crate::services::orchestration::OrchestrationService;

pub(crate) struct AgentContainerControlSettings {
    pub(crate) workspace_root: String,
    pub(crate) nats_container_url: Option<String>,
    pub(crate) nats_agent_url: Option<String>,
    pub(crate) nats_url: Option<String>,
    pub(crate) container_server_url: Option<String>,
    pub(crate) codex_default_model: String,
    pub(crate) context_injection_enabled: bool,
    pub(crate) cli_images: HashMap<String, String>,
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
            cli_images: configured_cli_images(),
        }
    }
}

pub(crate) struct AgentContainerControlService {
    pool: PgPool,
    agents: AgentService,
    orchestration: OrchestrationService,
    credentials: AgentContainerCredentialService,
    workspaces: AgentWorkspaceService,
    docker: Option<Arc<DockerClient>>,
    auth_callout: Option<Arc<AuthCalloutService>>,
    settings: AgentContainerControlSettings,
}

/// Proof that the caller owns the per-Agent lifecycle advisory lock. Only this
/// module can construct it, so Docker-mutating bodies cannot be called bare.
struct LifecycleGuard;

pub(crate) enum AgentContainerReplaceOutcome {
    Respawned,
    RespawnFailed(AppError),
    StillRunning,
    Unconfirmed,
    StopFailed(AppError),
}

/// One signature-verified immutable image identity captured while the caller
/// holds the tool-wide image mutation advisory lock. Every Agent in one roll
/// receives this exact content id; mutable tags are never resolved per Agent.
#[derive(Clone)]
pub(crate) struct VerifiedRollImage {
    tool: CliToolKind,
    configured_source: String,
    identity: LocalImageIdentity,
    evidence: AgentContainerImageIdentity,
}

fn container_cli_tool(agent: &Agent) -> AppResult<CliToolKind> {
    let raw = agent
        .cli_tool
        .as_deref()
        .or_else(|| agent.model.as_deref().and_then(|model| model.trim().strip_prefix("agentforge-agent:")))
        .ok_or_else(|| ErrorKind::Validation("Container agent has no Container CLI tool".to_string()))?;
    CliToolKind::parse_legacy(raw)
        .map_err(|err| ErrorKind::Validation(format!("Unsupported Container CLI tool {raw:?}: {err}")).into())
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
            pool.clone(),
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
        pool: PgPool,
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
            pool,
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
        self.agents.authorize_action(scope, agent_id, "edit").await?;
        let Some((tx, guard)) = self.admit_idle_lifecycle(scope, agent_id).await? else {
            return Err(AgentContainerRuntimePolicy::lifecycle_blocked_by_active_work().into());
        };
        let result = self.start_locked(scope, agent_id, &guard).await;
        let outcome = finish_lifecycle(tx, result).await?;
        self.register_started_agent_participant_best_effort(scope, agent_id).await;
        Ok(outcome)
    }

    async fn start_locked(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        _guard: &LifecycleGuard,
    ) -> AppResult<AgentContainerStartOutcome> {
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
                Ok(info)
                    if info.status == ContainerState::Running
                        && agent.hmac_secret.is_some()
                        && agent.nats_connect_password.is_some() =>
                {
                    if let Err(err) = self.ensure_container_image_identity(scope, &agent, container_id, &info).await {
                        self.quarantine_unverified_container(scope, agent_id, container_id).await;
                        return Err(err);
                    }
                    return Ok(AgentContainerStartOutcome::already_running(container_id));
                }
                Ok(info) => {
                    tracing::info!(
                        agent_id = %agent_id,
                        container_id = %container_id,
                        status = ?info.status,
                        "agent container is not running; replacing it"
                    );
                    remove_container_for_replacement(docker, container_id).await?;
                    if !self.agents.clear_container(scope, agent_id, container_id).await? {
                        return Err(AgentContainerRuntimePolicy::container_changed_during_lifecycle().into());
                    }
                    self.mark_participant_offline_best_effort(scope, agent_id).await;
                }
                Err(err) if err.is_not_found() => {
                    tracing::warn!(
                        error = %err,
                        agent_id = %agent_id,
                        container_id = %container_id,
                        "agent container reference is stale; creating a replacement"
                    );
                    if !self.agents.clear_container(scope, agent_id, container_id).await? {
                        return Err(AgentContainerRuntimePolicy::container_changed_during_lifecycle().into());
                    }
                    self.mark_participant_offline_best_effort(scope, agent_id).await;
                }
                Err(err) => {
                    return Err(AgentContainerRuntimePolicy::lifecycle_action_unavailable("inspect", err).into());
                }
            }
        }

        let image = AgentContainerImagePolicy::resolve_configured_for_start(
            agent.cli_tool.as_deref(),
            agent.model.as_deref(),
            &self.settings.cli_images,
        )?;
        let cli_tool = container_cli_tool(&agent)?;
        let identity = docker
            .local_image_identity(&image)
            .await
            .map_err(|err| AgentContainerRuntimePolicy::lifecycle_action_unavailable("inspect image for", err))?
            .ok_or_else(|| {
                AgentContainerRuntimePolicy::create_container_failed(
                    &image,
                    agent.cli_tool.as_deref(),
                    true,
                    "image not found",
                )
            })?;
        let image_evidence = capture_container_image_identity(cli_tool, &image, &identity).await?;
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

        let mut labels = HashMap::from([
            ("agentforge.agent_id".to_string(), agent_id.as_uuid().to_string()),
            ("agentforge.org_id".to_string(), scope.org_id().as_uuid().to_string()),
            ("agentforge.image.source".to_string(), image_evidence.source.clone()),
            ("agentforge.image.id".to_string(), identity.id.clone()),
        ]);
        if let Some(digest) = &identity.manifest_digest {
            labels.insert("agentforge.image.digest".to_string(), digest.clone());
        }

        let config = ContainerConfig {
            // Create by immutable content id, never by the mutable configured
            // tag checked above. This closes the inspect -> re-tag -> create
            // race without a global Docker lock.
            image: identity.id.clone(),
            name: Some(container_name),
            working_dir: Some(workspace_paths.container_cwd),
            env,
            labels,
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

        let created = docker.inspect_container(&container_id).await;
        if !matches!(&created, Ok(info) if info.image_id == identity.id) {
            if let Err(cleanup_err) = docker.remove_container(&container_id, true).await {
                tracing::warn!(
                    error = %cleanup_err,
                    container_id = %container_id,
                    "failed to remove container after image identity verification failed"
                );
            }
            match created {
                Ok(info) => tracing::error!(
                    container_id = %container_id,
                    expected_image_id = %identity.id,
                    actual_image_id = %info.image_id,
                    "created agent container image identity mismatch"
                ),
                Err(err) => tracing::error!(
                    container_id = %container_id,
                    expected_image_id = %identity.id,
                    error = %err,
                    "could not verify created agent container image identity"
                ),
            }
            return Err(AgentContainerRuntimePolicy::image_identity_unavailable(&image).into());
        }

        let image_evidence = serde_json::to_value(image_evidence)
            .map_err(|err| agentforge_core::ErrorKind::Internal(anyhow::anyhow!("serialize image identity: {err}")))?;
        if let Err(err) = self
            .agents
            .set_container(scope, agent_id, &container_id, &hmac_secret, &nats_connect_password, &image_evidence)
            .await
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
            match self.agents.clear_container(scope, agent_id, &container_id).await {
                Ok(true) => {}
                Ok(false) => tracing::info!(
                    agent_id = %agent_id,
                    container_id = %container_id,
                    "container metadata already changed after start failure"
                ),
                Err(clear_err) => tracing::warn!(
                    error = ?clear_err,
                    agent_id = %agent_id,
                    "failed to clear container metadata after start failure"
                ),
            }
            return Err(AgentContainerRuntimePolicy::start_container_failed(err).into());
        }

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

    /// Atomically stop and recreate an idle Agent while holding the same
    /// lifecycle lock used by task admission. `None` means work became active
    /// before the lock was acquired.
    pub(crate) async fn replace_if_idle(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
    ) -> AppResult<Option<AgentContainerReplaceOutcome>> {
        self.agents.authorize_action(scope, agent_id, "edit").await?;
        let Some((tx, guard)) = self.admit_idle_lifecycle(scope, agent_id).await? else {
            return Ok(None);
        };
        let outcome = match self.stop_with_outcome_locked(scope, agent_id, &guard).await {
            Ok(AgentContainerStopOutcome::Stopped) => match self.start_locked(scope, agent_id, &guard).await {
                Ok(_) => AgentContainerReplaceOutcome::Respawned,
                Err(err) => AgentContainerReplaceOutcome::RespawnFailed(err),
            },
            Ok(AgentContainerStopOutcome::StillRunning) => AgentContainerReplaceOutcome::StillRunning,
            Ok(AgentContainerStopOutcome::Unconfirmed) => AgentContainerReplaceOutcome::Unconfirmed,
            Err(err) => AgentContainerReplaceOutcome::StopFailed(err),
        };
        tx.commit().await?;
        if matches!(outcome, AgentContainerReplaceOutcome::Respawned) {
            self.register_started_agent_participant_best_effort(scope, agent_id).await;
        }
        Ok(Some(outcome))
    }

    /// Resolve and verify the roll target exactly once while the caller holds
    /// the tool-wide image mutation advisory lock.
    pub(crate) async fn snapshot_verified_roll_image(
        &self,
        _authority: &PlatformAdminAuthority,
        tool: CliToolKind,
    ) -> AppResult<VerifiedRollImage> {
        let docker = self.docker.as_ref().ok_or_else(AgentContainerRuntimePolicy::control_docker_unavailable)?;
        let configured_source = AgentContainerImagePolicy::resolve_configured_for_start(
            Some(tool.as_str()),
            None,
            &self.settings.cli_images,
        )?;
        let identity = docker
            .local_image_identity(&configured_source)
            .await
            .map_err(|err| AgentContainerRuntimePolicy::lifecycle_action_unavailable("inspect image for", err))?
            .ok_or_else(|| {
                AgentContainerRuntimePolicy::create_container_failed(
                    &configured_source,
                    Some(tool.as_str()),
                    true,
                    "image not found",
                )
            })?;
        let evidence = capture_container_image_identity(tool, &configured_source, &identity).await?;
        Ok(VerifiedRollImage { tool, configured_source, identity, evidence })
    }

    /// Sealed cross-tenant roll primitive. The platform-admin proof is required
    /// by the type system; the authoritative Agent is re-read only after its
    /// lifecycle lock is held, and no caller-fabricated TenantScope is used.
    pub(crate) async fn replace_if_idle_as_platform_admin(
        &self,
        authority: &PlatformAdminAuthority,
        agent_id: AgentId,
        image: &VerifiedRollImage,
    ) -> AppResult<Option<AgentContainerReplaceOutcome>> {
        let mut tx = self.pool.begin().await?;
        agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, agent_id.as_uuid()).await?;
        let agent = AgentRepository::find_by_id_as_platform_admin_in_tx(&mut tx, agent_id).await?;
        if agent.runtime_kind != RuntimeKind::Container || container_cli_tool(&agent)? != image.tool {
            tx.commit().await?;
            return Err(ErrorKind::Conflict("Agent no longer matches this Container CLI roll".to_string()).into());
        }
        let idle = agentforge_db::agent_work_admission_is_idle_in_tx(
            &mut tx,
            agent.organization_id.as_uuid(),
            agent_id.as_uuid(),
        )
        .await?
        .unwrap_or(false);
        if !idle {
            tx.commit().await?;
            return Ok(None);
        }

        let Some(expected_container_id) = agent.container_id.as_deref() else {
            tx.commit().await?;
            return Err(AgentContainerRuntimePolicy::container_changed_during_lifecycle().into());
        };
        let guard = LifecycleGuard;
        let outcome = match self.stop_and_remove_container_locked(agent_id, expected_container_id, &guard).await {
            Ok(AgentContainerStopOutcome::Stopped) => {
                if !AgentRepository::clear_container_as_platform_admin_in_tx(
                    &mut tx,
                    agent_id,
                    agent.organization_id.as_uuid(),
                    expected_container_id,
                )
                .await?
                {
                    AgentContainerReplaceOutcome::StopFailed(
                        AgentContainerRuntimePolicy::container_changed_during_lifecycle().into(),
                    )
                } else {
                    // Revoke the old generation before the replacement sidecar
                    // can authenticate with its newly rotated credentials.
                    self.revoke_agent_connection(agent_id.as_uuid()).await;
                    match self.start_stopped_as_platform_admin_locked(&mut tx, authority, &agent, image, &guard).await {
                        Ok(_) => AgentContainerReplaceOutcome::Respawned,
                        Err(err) => AgentContainerReplaceOutcome::RespawnFailed(err),
                    }
                }
            }
            Ok(AgentContainerStopOutcome::StillRunning) => AgentContainerReplaceOutcome::StillRunning,
            Ok(AgentContainerStopOutcome::Unconfirmed) => AgentContainerReplaceOutcome::Unconfirmed,
            Err(err) => AgentContainerReplaceOutcome::StopFailed(err),
        };

        // A failed respawn intentionally commits the confirmed stop/clear so
        // operators see a truthful stopped Agent instead of a stale container.
        tx.commit().await?;
        Ok(Some(outcome))
    }

    async fn start_stopped_as_platform_admin_locked(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        authority: &PlatformAdminAuthority,
        agent: &Agent,
        image: &VerifiedRollImage,
        _guard: &LifecycleGuard,
    ) -> AppResult<AgentContainerStartOutcome> {
        let docker = self.docker.as_ref().ok_or_else(AgentContainerRuntimePolicy::control_docker_unavailable)?;
        let agent_id = agent.id;
        let container_name = format!("agentforge-agent-{}", agent_id.as_uuid());
        let hmac_secret = Uuid::new_v4().to_string();
        let nats_connect_password = Uuid::new_v4().to_string();
        let workspace_paths = self.prepare_workspace_for_agent(agent.organization_id.as_uuid(), agent).await?;
        let workspace_host_path = workspace_paths.host_projects_root.to_string_lossy().into_owned();
        let nats_base_url = AgentContainerEnvPolicy::pick_container_nats_base_url(
            self.settings.nats_container_url.as_deref(),
            self.settings.nats_agent_url.as_deref(),
            self.settings.nats_url.as_deref(),
        );
        let mut env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: agent_id.as_uuid(),
            org_id: agent.organization_id.as_uuid(),
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
        let mut mounts =
            vec![Mount { source: workspace_host_path, target: CONTAINER_WORKSPACE_ROOT.to_string(), read_only: false }];
        self.credentials
            .inject_runtime_credentials_as_platform_admin(authority, agent, &container_name, &mut env, &mut mounts)
            .await?;

        let mut labels = HashMap::from([
            ("agentforge.agent_id".to_string(), agent_id.as_uuid().to_string()),
            ("agentforge.org_id".to_string(), agent.organization_id.as_uuid().to_string()),
            ("agentforge.image.source".to_string(), image.evidence.source.clone()),
            ("agentforge.image.id".to_string(), image.identity.id.clone()),
        ]);
        if let Some(digest) = &image.identity.manifest_digest {
            labels.insert("agentforge.image.digest".to_string(), digest.clone());
        }
        let config = ContainerConfig {
            image: image.identity.id.clone(),
            name: Some(container_name),
            working_dir: Some(workspace_paths.container_cwd),
            env,
            labels,
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
                &image.configured_source,
                agent.cli_tool.as_deref(),
                err.is_missing_image(),
                err,
            )
        })?;
        let created = docker.inspect_container(&container_id).await;
        if !matches!(&created, Ok(info) if info.image_id == image.identity.id) {
            if let Err(cleanup_err) = docker.remove_container(&container_id, true).await {
                tracing::warn!(error = %cleanup_err, %container_id, "failed to remove roll container after image identity mismatch");
            }
            return Err(AgentContainerRuntimePolicy::image_identity_unavailable(&image.configured_source).into());
        }
        let evidence = serde_json::to_value(&image.evidence)
            .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("serialize image identity: {err}")))?;
        if !AgentRepository::set_container_as_platform_admin_in_tx(
            tx,
            agent_id,
            agent.organization_id.as_uuid(),
            &container_id,
            &hmac_secret,
            &nats_connect_password,
            &evidence,
        )
        .await?
        {
            if let Err(cleanup_err) = docker.remove_container(&container_id, true).await {
                tracing::warn!(error = %cleanup_err, %container_id, "failed to clean up roll container after DB CAS failure");
            }
            return Err(AgentContainerRuntimePolicy::container_changed_during_lifecycle().into());
        }
        if let Err(err) = docker.start_container(&container_id).await {
            if let Err(cleanup_err) = docker.remove_container(&container_id, true).await {
                tracing::warn!(error = %cleanup_err, %container_id, "failed to clean up roll container after start failure");
            }
            let _ = AgentRepository::clear_container_as_platform_admin_in_tx(
                tx,
                agent_id,
                agent.organization_id.as_uuid(),
                &container_id,
            )
            .await;
            return Err(AgentContainerRuntimePolicy::start_container_failed(err).into());
        }
        Ok(AgentContainerStartOutcome::started(container_id))
    }

    pub(crate) async fn replace(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        match self.replace_if_idle(scope, agent_id).await? {
            None => Err(AgentContainerRuntimePolicy::lifecycle_blocked_by_active_work().into()),
            Some(AgentContainerReplaceOutcome::Respawned) => Ok(()),
            Some(AgentContainerReplaceOutcome::RespawnFailed(err) | AgentContainerReplaceOutcome::StopFailed(err)) => {
                Err(err)
            }
            Some(AgentContainerReplaceOutcome::StillRunning) => {
                Err(AgentContainerRuntimePolicy::container_still_running_after_stop().into())
            }
            Some(AgentContainerReplaceOutcome::Unconfirmed) => {
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
        self.agents.authorize_action(scope, agent_id, "edit").await?;
        let Some((tx, guard)) = self.admit_idle_lifecycle(scope, agent_id).await? else {
            return Err(AgentContainerRuntimePolicy::lifecycle_blocked_by_active_work().into());
        };
        let result = self.stop_with_outcome_locked(scope, agent_id, &guard).await;
        finish_lifecycle(tx, result).await
    }

    async fn stop_with_outcome_locked(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        guard: &LifecycleGuard,
    ) -> AppResult<AgentContainerStopOutcome> {
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
        // desired post-condition; only converge the external side effects.
        let Some(container_id) = inner.container_id.as_deref() else {
            self.mark_participant_offline_best_effort(scope, agent_id).await;
            self.revoke_agent_connection(agent_id.as_uuid()).await;
            return Ok(AgentContainerStopOutcome::Stopped);
        };
        let outcome = self.stop_and_remove_container_locked(agent_id, container_id, guard).await?;

        // Step 4: reconcile the DB only when the container is confirmed absent.
        if outcome == AgentContainerStopOutcome::Stopped {
            self.reconcile_stopped_agent(scope, agent_id, container_id).await?;
        }
        Ok(outcome)
    }

    async fn stop_and_remove_container_locked(
        &self,
        agent_id: AgentId,
        container_id: &str,
        _guard: &LifecycleGuard,
    ) -> AppResult<AgentContainerStopOutcome> {
        let docker = self.docker.as_ref().ok_or_else(AgentContainerRuntimePolicy::control_docker_unavailable)?;
        idempotent_container_op(|| docker.stop_container(container_id, 30)).await;
        let container_name = format!("agentforge-agent-{}", agent_id.as_uuid());
        self.credentials.cleanup_oauth_mount_best_effort(agent_id.as_uuid(), &container_name).await;
        idempotent_container_op(|| docker.remove_container(container_id, true)).await;

        Ok(match docker.inspect_container(container_id).await {
            Err(err) if err.is_not_found() => AgentContainerStopOutcome::Stopped,
            Ok(info) => {
                tracing::warn!(
                    agent_id = %agent_id,
                    container_id,
                    status = ?info.status,
                    "agent container still present after stop+remove; leaving DB row for reconciliation"
                );
                AgentContainerStopOutcome::StillRunning
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    agent_id = %agent_id,
                    container_id,
                    "could not verify agent container stopped; leaving DB row for reconciliation"
                );
                AgentContainerStopOutcome::Unconfirmed
            }
        })
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
        let mut tx = self.pool.begin().await?;
        agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, agent_id.as_uuid()).await?;
        let guard = LifecycleGuard;
        let result =
            self.reconcile_agent_if_container_absent_locked(&mut tx, scope, agent_id, container_id, &guard).await;
        finish_lifecycle(tx, result).await
    }

    async fn reconcile_agent_if_container_absent_locked(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        agent_id: AgentId,
        container_id: &str,
        _guard: &LifecycleGuard,
    ) -> AppResult<bool> {
        let Some(docker) = self.docker.as_ref() else { return Ok(false) };
        match docker.inspect_container(container_id).await {
            Err(err) if err.is_not_found() => {
                let reconciled = self.reconcile_stopped_agent(scope, agent_id, container_id).await?;
                if !reconciled {
                    return Ok(false);
                }
                tracing::info!(
                    agent_id = %agent_id,
                    container_id = %container_id,
                    "reconcile: cleared stale container reference for a vanished container"
                );
                Ok(true)
            }
            Ok(info) => {
                let agent = self.agents.get(scope, agent_id).await?;
                match self.ensure_container_image_identity(scope, &agent, container_id, &info).await {
                    Ok(updated) => Ok(updated),
                    Err(err) => {
                        invalidate_active_work_for_quarantine(tx, scope, agent_id).await?;
                        self.quarantine_unverified_container(scope, agent_id, container_id).await;
                        Err(err)
                    }
                }
            }
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
    async fn reconcile_stopped_agent(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        expected_container_id: &str,
    ) -> AppResult<bool> {
        if !self.agents.clear_container(scope, agent_id, expected_container_id).await? {
            return Ok(false);
        }
        self.mark_participant_offline_best_effort(scope, agent_id).await;
        self.revoke_agent_connection(agent_id.as_uuid()).await;
        Ok(true)
    }

    async fn ensure_container_image_identity(
        &self,
        scope: &TenantScope,
        agent: &Agent,
        expected_container_id: &str,
        container: &ContainerInfo,
    ) -> AppResult<bool> {
        if agent.container_id.as_deref() != Some(expected_container_id) {
            return Ok(false);
        }
        let cli_tool = container_cli_tool(agent)?;
        let recorded_identity = agent.container_image_identity.as_ref();
        let recorded_image_matches =
            recorded_identity.and_then(|identity| identity.get("imageId")).and_then(serde_json::Value::as_str)
                == Some(container.image_id.as_str());
        let recorded_trust =
            recorded_identity.and_then(|identity| identity.get("trust")).and_then(serde_json::Value::as_str);
        if recorded_image_matches && recorded_image_trust_is_acceptable(cli_tool, recorded_trust) {
            return Ok(false);
        }

        let docker = self.docker.as_ref().ok_or_else(AgentContainerRuntimePolicy::control_docker_unavailable)?;
        let configured_image = AgentContainerImagePolicy::resolve_configured_for_start(
            agent.cli_tool.as_deref(),
            agent.model.as_deref(),
            &self.settings.cli_images,
        )?;
        let identity = docker
            .local_image_identity_for_source(&container.image_id, &configured_image)
            .await
            .map_err(|err| AgentContainerRuntimePolicy::lifecycle_action_unavailable("inspect image for", err))?
            .ok_or_else(|| AgentContainerRuntimePolicy::image_identity_unavailable(&configured_image))?;
        if identity.id != container.image_id {
            return Err(AgentContainerRuntimePolicy::image_identity_unavailable(&configured_image).into());
        }
        let evidence = capture_container_image_identity(cli_tool, &configured_image, &identity).await?;
        let evidence = serde_json::to_value(evidence)
            .map_err(|err| agentforge_core::ErrorKind::Internal(anyhow::anyhow!("serialize image identity: {err}")))?;
        self.agents.set_container_image_identity(scope, agent.id, expected_container_id, &evidence).await
    }

    async fn quarantine_unverified_container(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
        expected_container_id: &str,
    ) {
        let Some(docker) = self.docker.as_ref() else { return };
        idempotent_container_op(|| docker.stop_container(expected_container_id, 30)).await;
        idempotent_container_op(|| docker.remove_container(expected_container_id, true)).await;

        if docker.inspect_container(expected_container_id).await.is_err_and(|err| err.is_not_found()) {
            if let Err(err) = self.reconcile_stopped_agent(scope, agent_id, expected_container_id).await {
                tracing::error!(error = ?err, %agent_id, "failed to clear quarantined Agent container metadata");
            }
            return;
        }

        match self.agents.quarantine_container(scope, agent_id, expected_container_id).await {
            Ok(true) => {
                self.mark_participant_offline_best_effort(scope, agent_id).await;
                self.revoke_agent_connection(agent_id.as_uuid()).await;
            }
            Ok(false) => {}
            Err(err) => tracing::error!(error = ?err, %agent_id, "failed to revoke unverified Agent container"),
        }
    }

    /// Stop a container Agent before deleting its row, while preserving the
    /// same no-active-work admission used by every other lifecycle mutation.
    pub(crate) async fn delete(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        // Authorize before any Docker side effect.
        self.agents.require_owner(scope, agent_id).await?;
        let Some((tx, guard)) = self.admit_idle_lifecycle(scope, agent_id).await? else {
            return Err(AgentContainerRuntimePolicy::lifecycle_blocked_by_active_work().into());
        };
        let runtime_kind = self.agents.find_aggregate(scope, agent_id.as_uuid()).await?.runtime_kind();
        let result = if runtime_kind == RuntimeKind::Container {
            match self.stop_with_outcome_locked(scope, agent_id, &guard).await? {
                AgentContainerStopOutcome::Stopped => self.agents.delete(scope, agent_id).await,
                AgentContainerStopOutcome::StillRunning => {
                    Err(AgentContainerRuntimePolicy::container_still_running_after_stop().into())
                }
                AgentContainerStopOutcome::Unconfirmed => {
                    Err(AgentContainerRuntimePolicy::stop_post_condition_unverified().into())
                }
            }
        } else {
            self.agents.delete(scope, agent_id).await
        };
        let result = finish_lifecycle(tx, result).await;
        if result.is_ok() && runtime_kind != RuntimeKind::Container {
            self.revoke_agent_connection(agent_id.as_uuid()).await;
        }
        result
    }

    /// Cross-tenant deletion primitive for the already-authorized platform
    /// admin route. It never fabricates a tenant scope: the authoritative Agent
    /// row is re-read only after the lifecycle lock is held.
    pub(crate) async fn delete_as_platform_admin(
        &self,
        _authority: &PlatformAdminAuthority,
        agent_id: AgentId,
    ) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, agent_id.as_uuid()).await?;
        let agent = AgentRepository::find_by_id_as_platform_admin_in_tx(&mut tx, agent_id).await?;
        let idle = agentforge_db::agent_work_admission_is_idle_in_tx(
            &mut tx,
            agent.organization_id.as_uuid(),
            agent_id.as_uuid(),
        )
        .await?
        .unwrap_or(false);
        if !idle {
            tx.commit().await?;
            return Err(AgentContainerRuntimePolicy::lifecycle_blocked_by_active_work().into());
        }

        let guard = LifecycleGuard;
        let expected_container_id = agent.container_id.clone();
        let result: AppResult<()> = async {
            if let Some(container_id) = expected_container_id.as_deref() {
                match self.stop_and_remove_container_locked(agent_id, container_id, &guard).await? {
                    AgentContainerStopOutcome::Stopped => {}
                    AgentContainerStopOutcome::StillRunning => {
                        return Err(AgentContainerRuntimePolicy::container_still_running_after_stop().into());
                    }
                    AgentContainerStopOutcome::Unconfirmed => {
                        return Err(AgentContainerRuntimePolicy::stop_post_condition_unverified().into());
                    }
                }
            }
            if !AgentRepository::delete_as_platform_admin_in_tx(&mut tx, agent_id, expected_container_id.as_deref())
                .await?
            {
                return Err(AgentContainerRuntimePolicy::container_changed_during_lifecycle().into());
            }
            Ok(())
        }
        .await;
        let result = finish_lifecycle(tx, result).await;
        if result.is_ok() {
            self.revoke_agent_connection(agent_id.as_uuid()).await;
        }
        result
    }

    async fn admit_idle_lifecycle(
        &self,
        scope: &TenantScope,
        agent_id: AgentId,
    ) -> AppResult<Option<(Transaction<'_, Postgres>, LifecycleGuard)>> {
        let mut tx = self.pool.begin().await?;
        agentforge_db::lock_agent_lifecycle_in_tx(&mut tx, agent_id.as_uuid()).await?;
        if !AgentRepository::lifecycle_is_idle_in_tx(&mut tx, scope, agent_id).await? {
            tx.commit().await?;
            return Ok(None);
        }
        Ok(Some((tx, LifecycleGuard)))
    }

    async fn prepare_workspace(
        &self,
        scope: &TenantScope,
        agent: &Agent,
    ) -> AppResult<crate::services::agent_workspace::AgentWorkspacePaths> {
        self.prepare_workspace_for_agent(scope.org_id().as_uuid(), agent).await
    }

    async fn prepare_workspace_for_agent(
        &self,
        organization_id: Uuid,
        agent: &Agent,
    ) -> AppResult<crate::services::agent_workspace::AgentWorkspacePaths> {
        let workspace_scope =
            WorkspaceMountScope { org_id: organization_id, workspace_id: agent.workspace_id.as_uuid() };
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

    async fn register_started_agent_participant_best_effort(&self, scope: &TenantScope, agent_id: AgentId) {
        let agent = match self.agents.get(scope, agent_id).await {
            Ok(agent) => agent,
            Err(err) => {
                tracing::warn!(error = ?err, %agent_id, "started Agent before post-commit participant lookup completed");
                return;
            }
        };
        if let Err(err) = self.register_started_agent_participant(scope, &agent).await {
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

const INVALIDATE_ACTIVE_WORK_FOR_QUARANTINE_SQL: &str = r#"
WITH failed_tasks AS MATERIALIZED (
    UPDATE orchestration_tasks
       SET status = 'failed',
           error = jsonb_build_object(
               'message', 'Agent image identity could not be verified; the work was stopped',
               'code', 'agent_image_unverified'
           ),
           failure_code = 'agent_image_unverified',
           retryable = FALSE,
           lease_expires_at = NULL,
           last_assignment_id = NULL,
           completed_at = NOW(),
           updated_at = NOW()
     WHERE organization_id = $1
       AND assigned_agent_id = $2
       AND status = 'working'
     RETURNING id
), closed_runs AS (
    UPDATE task_runs
       SET status = 'failed',
           finished_at = COALESCE(finished_at, NOW()),
           updated_at = NOW()
     WHERE organization_id = $1
       AND agent_id = $2
       AND finished_at IS NULL
       AND orchestration_task_id IN (SELECT id FROM failed_tasks)
)
DELETE FROM orchestration_outbox
 WHERE organization_id = $1
   AND aggregate_type = 'orchestration_task'
   AND event_type = 'assignment'
   AND published_at IS NULL
   AND aggregate_id IN (SELECT id FROM failed_tasks)"#;

async fn invalidate_active_work_for_quarantine(
    tx: &mut Transaction<'_, Postgres>,
    scope: &TenantScope,
    agent_id: AgentId,
) -> AppResult<()> {
    sqlx::query(INVALIDATE_ACTIVE_WORK_FOR_QUARANTINE_SQL)
        .bind(scope.org_id().as_uuid())
        .bind(agent_id.as_uuid())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn finish_lifecycle<T>(tx: Transaction<'_, Postgres>, result: AppResult<T>) -> AppResult<T> {
    match (tx.commit().await, result) {
        (Ok(()), result) => result,
        (Err(commit_err), Ok(_)) => Err(commit_err.into()),
        (Err(commit_err), Err(operation_err)) => {
            tracing::warn!(error = %commit_err, "failed to release Agent lifecycle transaction after operation error");
            Err(operation_err)
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

async fn remove_container_for_replacement(docker: &DockerClient, container_id: &str) -> AppResult<()> {
    idempotent_container_op(|| docker.stop_container(container_id, 30)).await;
    idempotent_container_op(|| docker.remove_container(container_id, true)).await;
    match docker.inspect_container(container_id).await {
        Err(err) if err.is_not_found() => Ok(()),
        Ok(_) => Err(AgentContainerRuntimePolicy::container_still_running_after_stop().into()),
        Err(err) => Err(AgentContainerRuntimePolicy::lifecycle_action_unavailable("confirm removal of", err).into()),
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
            cli_images: HashMap::new(),
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

    #[test]
    fn quarantine_invalidates_current_delivery_before_container_removal() {
        assert!(INVALIDATE_ACTIVE_WORK_FOR_QUARANTINE_SQL.contains("status = 'working'"));
        assert!(INVALIDATE_ACTIVE_WORK_FOR_QUARANTINE_SQL.contains("last_assignment_id = NULL"));
        assert!(INVALIDATE_ACTIVE_WORK_FOR_QUARANTINE_SQL.contains("published_at IS NULL"));
        assert!(INVALIDATE_ACTIVE_WORK_FOR_QUARANTINE_SQL.contains("finished_at = COALESCE"));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn post_start_sweep_can_reacquire_lifecycle_only_after_finish_commit(pool: PgPool) {
        let agent_id = Uuid::new_v4();
        let mut lifecycle = pool.begin().await.unwrap();
        agentforge_db::lock_agent_lifecycle_in_tx(&mut lifecycle, agent_id).await.unwrap();

        // `start`/`replace_if_idle` now call participant registration (which may
        // sweep and claim a queued task) only after this commit helper returns.
        finish_lifecycle(lifecycle, Ok(())).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            let mut sweep_claim = pool.begin().await.unwrap();
            agentforge_db::lock_agent_lifecycle_in_tx(&mut sweep_claim, agent_id).await.unwrap();
            sweep_claim.rollback().await.unwrap();
        })
        .await
        .expect("post-commit queued-task sweep must not self-deadlock on the Agent lifecycle lock");
    }
}
