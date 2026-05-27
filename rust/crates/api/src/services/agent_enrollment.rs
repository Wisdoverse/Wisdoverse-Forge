//! Host CLI agent enrollment.
//!
//! This path creates a normal managed agent row, issues the same per-agent
//! NATS/HMAC material used by container sidecars, and returns a one-time shell
//! environment for running a sidecar on an operator-managed machine.
//!
//! Key invariants enforced here (see spec §6.3):
//! - TLS gate: non-`tls://` NATS URLs are rejected unless `allow_plaintext_host_nats` is set.
//! - Idempotency fast path: the same `(org_id, user_id, idempotency_key)` triple
//!   returns the original response without creating a second agent.
//! - Atomic cold path: agent INSERT and idempotency record are written in a
//!   single transaction via `create_aggregate_in_tx`.

use std::collections::BTreeMap;

use agentforge_core::{AgentId, AppConfig, AppError, AppResult, CliToolKind, ErrorKind, TenantScope};
use anyhow::anyhow;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::agent::{
    AgentContainerEnvInput, AgentContainerEnvPolicy, AgentName, HostAgentEnrollment, HostAgentEnrollmentPolicy,
    HostCliIdentity, NewAgent,
};
use crate::domain::context::{ContextFeature, ContextFeatureFlags};
use crate::repositories::agent::{AgentListItem, AgentRepository};
use crate::repositories::enrollment_idempotency::EnrollmentIdempotencyRepository;
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
    allow_plaintext_host_nats: bool,
}

pub(crate) struct HostAgentEnrollmentService {
    pool: PgPool,
    agents: AgentRepository,
    workspaces: AgentWorkspaceService,
    settings: HostAgentEnrollmentSettings,
}

impl HostAgentEnrollmentService {
    pub(crate) fn from_runtime(pool: PgPool, config: &AppConfig, context_features: ContextFeatureFlags) -> Self {
        Self {
            agents: AgentRepository::new(pool.clone()),
            workspaces: AgentWorkspaceService::from_pool(pool.clone()),
            settings: HostAgentEnrollmentSettings {
                nats_agent_url: config.nats_agent_url.clone(),
                nats_url: config.nats_url.clone(),
                server_url: config.app_url.clone().or_else(|| config.container_server_url.clone()),
                codex_default_model: config.codex_default_model.clone(),
                context_injection_enabled: context_features.enabled(ContextFeature::Injection),
                allow_plaintext_host_nats: config.allow_plaintext_host_nats,
            },
            pool,
        }
    }

    pub(crate) async fn enroll(
        &self,
        scope: &TenantScope,
        idempotency_key: &str,
        input: HostAgentEnrollmentInput<'_>,
    ) -> AppResult<(AgentListItem, HostAgentEnrollment)> {
        // 1. Validate name + cli_tool + NATS URL.
        AgentName::validate(input.name)?;
        let cli_tool_str = HostAgentEnrollmentPolicy::require_cli_tool(input.cli_tool)?;
        let nats_base_url = HostAgentEnrollmentPolicy::require_nats_base_url(
            self.settings.nats_agent_url.as_deref(),
            self.settings.nats_url.as_deref(),
        )?;

        // 2. TLS gate.
        if !nats_base_url.starts_with("tls://") && !self.settings.allow_plaintext_host_nats {
            return Err(ErrorKind::Validation(
                "errors.agent.enroll.plaintext_nats_blocked: Host CLI enrollment requires a \
                 tls:// NATS URL. Configure NATS_AGENT_URL to use tls://, or set \
                 ALLOW_PLAINTEXT_HOST_NATS=true to permit plaintext (dev/test only)."
                    .into(),
            )
            .into());
        }

        let org_id = scope.org_id().as_uuid();
        let user_id = scope.user_id().as_uuid();
        let idem = EnrollmentIdempotencyRepository::new(self.pool.clone());

        // 3. Idempotency fast path.
        if let Some(existing_id) = idem.lookup(org_id, user_id, idempotency_key).await? {
            let agent = self
                .agents
                .find_with_owner_by_id(scope, AgentId::from(existing_id))
                .await?;
            let enrollment = self.rebuild_enrollment_view(scope, &agent, existing_id, &nats_base_url).await?;
            return Ok((agent, enrollment));
        }

        // 4. Cold path.
        let workspace_scope = self
            .workspaces
            .resolve_workspace_mount_scope(org_id, input.workspace_id, input.project_id)
            .await?;

        let identity = HostCliIdentity::generate();
        let cli_kind = CliToolKind::parse_legacy(cli_tool_str).map_err(|_| {
            AppError::from(ErrorKind::Validation(format!("unknown cli_tool: {cli_tool_str}")))
        })?;
        let new_agent = NewAgent::host_cli(
            scope,
            cli_kind,
            identity.clone(),
            input.name,
            input.model,
            input.cwd,
            workspace_scope.workspace_id,
            input.project_id,
        )?;

        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        let id = self.agents.create_aggregate_in_tx(&mut tx, scope, new_agent).await?;
        EnrollmentIdempotencyRepository::store_in_tx(&mut tx, org_id, user_id, idempotency_key, id).await?;
        tx.commit().await.map_err(AppError::from)?;

        let agent = self.agents.find_with_owner_by_id(scope, AgentId::from(id)).await?;
        let enrollment = self.build_enrollment_view(&agent, &identity, &nats_base_url);
        Ok((agent, enrollment))
    }

    fn build_enrollment_view(
        &self,
        agent: &AgentListItem,
        identity: &HostCliIdentity,
        nats_base_url: &str,
    ) -> HostAgentEnrollment {
        let env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: agent.id,
            org_id: agent.organization_id,
            cli_tool: agent.cli_tool.as_deref(),
            cli_model: agent.model.as_deref(),
            codex_default_model: Some(self.settings.codex_default_model.as_str()),
            nats_base_url: Some(nats_base_url),
            nats_connect_password: identity.nats_connect_password(),
            container_server_url: self.settings.server_url.as_deref(),
            workspace_host_path: None,
            hmac_secret: identity.hmac_secret(),
            context_injection_enabled: self.settings.context_injection_enabled,
        });
        let mut env_map: BTreeMap<String, String> = HostAgentEnrollmentPolicy::env_map(env);
        env_map.insert("AGENTFORGE_RUNTIME_KIND".to_string(), "cli".to_string());
        let shell_exports = HostAgentEnrollmentPolicy::shell_exports(&env_map);
        HostAgentEnrollment {
            agent_id: agent.id,
            runtime_id: identity.runtime_id().to_string(),
            cli_tool: agent.cli_tool.clone().unwrap_or_default(),
            env: env_map,
            shell_exports,
            sidecar_command: HostAgentEnrollmentPolicy::SIDECAR_COMMAND.to_string(),
            server_url: self.settings.server_url.clone(),
        }
    }

    /// Rebuild the enrollment env from a previously created agent row.
    ///
    /// Called on idempotent replay: the agent already exists and we must return
    /// the same credentials the operator received during the original enrollment.
    /// `hmac_secret` and `nats_connect_password` are stored on the agent row
    /// (not in `AgentListItem` to avoid accidental serialization); this method
    /// fetches them and reassembles the env block.
    async fn rebuild_enrollment_view(
        &self,
        scope: &TenantScope,
        agent: &AgentListItem,
        id: Uuid,
        nats_base_url: &str,
    ) -> AppResult<HostAgentEnrollment> {
        let runtime_id = agent.runtime_id.clone().ok_or_else(|| {
            AppError::from(ErrorKind::Internal(anyhow!("Host CLI agent missing runtime_id on replay")))
        })?;

        let (hmac_secret, nats_connect_password) =
            self.agents.fetch_host_cli_credentials(scope, id).await?;

        let env = AgentContainerEnvPolicy::build(AgentContainerEnvInput {
            agent_id: agent.id,
            org_id: agent.organization_id,
            cli_tool: agent.cli_tool.as_deref(),
            cli_model: agent.model.as_deref(),
            codex_default_model: Some(self.settings.codex_default_model.as_str()),
            nats_base_url: Some(nats_base_url),
            nats_connect_password: &nats_connect_password,
            container_server_url: self.settings.server_url.as_deref(),
            workspace_host_path: None,
            hmac_secret: &hmac_secret,
            context_injection_enabled: self.settings.context_injection_enabled,
        });
        let mut env_map: BTreeMap<String, String> = HostAgentEnrollmentPolicy::env_map(env);
        env_map.insert("AGENTFORGE_RUNTIME_KIND".to_string(), "cli".to_string());
        let shell_exports = HostAgentEnrollmentPolicy::shell_exports(&env_map);
        let cli_tool = agent.cli_tool.clone().unwrap_or_default();
        Ok(HostAgentEnrollment {
            agent_id: agent.id,
            runtime_id,
            cli_tool,
            env: env_map,
            shell_exports,
            sidecar_command: HostAgentEnrollmentPolicy::SIDECAR_COMMAND.to_string(),
            server_url: self.settings.server_url.clone(),
        })
    }
}
