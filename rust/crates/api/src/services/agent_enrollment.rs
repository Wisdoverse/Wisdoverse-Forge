//! Host CLI agent enrollment.
//!
//! This path creates a normal managed agent row, issues the same per-agent
//! NATS/HMAC material used by container sidecars, and returns a one-time shell
//! environment for running a sidecar on an operator-managed machine.

use agentforge_core::{AgentId, AppConfig, AppResult, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::agent::{
    AgentContainerEnvInput, AgentContainerEnvPolicy, AgentName, HostAgentEnrollment, HostAgentEnrollmentPolicy,
};
use crate::domain::context::{ContextFeature, ContextFeatureFlags};
use crate::repositories::agent::{AgentListItem, AgentRepository, CreateAgentParams};
use crate::services::agent_workspace::AgentWorkspaceService;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct HostAgentEnrollmentInput<'a> {
    pub(crate) name: Option<&'a str>,
    pub(crate) model: Option<&'a str>,
    pub(crate) cli_tool: &'a str,
    pub(crate) cwd: Option<&'a str>,
    pub(crate) workspace_id: Option<Uuid>,
    pub(crate) project_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
struct HostAgentEnrollmentSettings {
    nats_agent_url: Option<String>,
    nats_url: Option<String>,
    server_url: Option<String>,
    codex_default_model: String,
    context_injection_enabled: bool,
}

pub(crate) struct HostAgentEnrollmentService {
    agents: AgentRepository,
    workspaces: AgentWorkspaceService,
    settings: HostAgentEnrollmentSettings,
}

impl HostAgentEnrollmentService {
    pub(crate) fn from_runtime(pool: PgPool, config: &AppConfig, context_features: ContextFeatureFlags) -> Self {
        Self {
            agents: AgentRepository::new(pool.clone()),
            workspaces: AgentWorkspaceService::from_pool(pool),
            settings: HostAgentEnrollmentSettings {
                nats_agent_url: config.nats_agent_url.clone(),
                nats_url: config.nats_url.clone(),
                server_url: config.app_url.clone().or_else(|| config.container_server_url.clone()),
                codex_default_model: config.codex_default_model.clone(),
                context_injection_enabled: context_features.enabled(ContextFeature::Injection),
            },
        }
    }

    pub(crate) async fn enroll(
        &self,
        scope: &TenantScope,
        input: HostAgentEnrollmentInput<'_>,
    ) -> AppResult<(AgentListItem, HostAgentEnrollment)> {
        AgentName::validate(input.name)?;
        let cli_tool = HostAgentEnrollmentPolicy::require_cli_tool(input.cli_tool)?;
        let nats_base_url = HostAgentEnrollmentPolicy::require_nats_base_url(
            self.settings.nats_agent_url.as_deref(),
            self.settings.nats_url.as_deref(),
        )?;

        let workspace_scope = self
            .workspaces
            .resolve_workspace_mount_scope(scope.org_id().as_uuid(), input.workspace_id, input.project_id)
            .await?;

        let agent = self
            .agents
            .create(
                scope,
                CreateAgentParams {
                    name: input.name,
                    model: input.model,
                    provider: None,
                    cli_tool: Some(cli_tool),
                    cwd: input.cwd,
                    workspace_id: Some(workspace_scope.workspace_id),
                    project_id: input.project_id,
                    system_prompt: None,
                },
            )
            .await?;

        let hmac_secret = Uuid::new_v4().to_string();
        let nats_connect_password = Uuid::new_v4().to_string();
        let runtime_id = HostAgentEnrollmentPolicy::runtime_id(agent.id.as_uuid());
        let agent =
            self.agents.set_host_runtime(scope, agent.id, &runtime_id, &hmac_secret, &nats_connect_password).await?;

        let env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: agent.id.as_uuid(),
            org_id: scope.org_id().as_uuid(),
            cli_tool: agent.cli_tool.as_deref(),
            cli_model: agent.model.as_deref(),
            codex_default_model: Some(self.settings.codex_default_model.as_str()),
            nats_base_url: Some(nats_base_url.as_str()),
            nats_connect_password: &nats_connect_password,
            container_server_url: self.settings.server_url.as_deref(),
            workspace_host_path: None,
            hmac_secret: &hmac_secret,
            context_injection_enabled: self.settings.context_injection_enabled,
        });
        let mut env = HostAgentEnrollmentPolicy::env_map(env);
        env.insert("AGENTFORGE_RUNTIME_KIND".to_string(), "cli".to_string());

        let shell_exports = HostAgentEnrollmentPolicy::shell_exports(&env);
        let enrollment = HostAgentEnrollment {
            agent_id: agent.id.as_uuid(),
            runtime_id,
            cli_tool: cli_tool.to_string(),
            env,
            shell_exports,
            sidecar_command: HostAgentEnrollmentPolicy::SIDECAR_COMMAND.to_string(),
            server_url: self.settings.server_url.clone(),
        };
        let enriched = self.agents.find_with_owner_by_id(scope, AgentId::from(agent.id.as_uuid())).await?;
        Ok((enriched, enrollment))
    }
}
