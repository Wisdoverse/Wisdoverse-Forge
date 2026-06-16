//! AppState service factories.
//!
//! Keeps route modules from assembling runtime-heavy service dependencies
//! directly. Routes should extract HTTP input and call service methods; this
//! module owns the wiring from shared runtime state into application services.

use std::sync::Arc;

use agentforge_core::AppResult;

use crate::health::AppState;
use crate::services::admin::AdminService;
use crate::services::agent::AgentService;
use crate::services::agent_container_control::AgentContainerControlService;
use crate::services::agent_container_lifecycle::AgentContainerLifecycleService;
use crate::services::agent_enrollment::HostAgentEnrollmentService;
use crate::services::agent_message::AgentMessageService;
use crate::services::agent_prompt::AgentPromptService;
use crate::services::analytics::AnalyticsService;
use crate::services::api_key::ApiKeyService;
use crate::services::attachment::AttachmentService;
use crate::services::audit::AuditService;
use crate::services::auth::AuthService;
use crate::services::billing::BillingService;
use crate::services::cli_auth_proxy::CliAuthProxyService;
use crate::services::cli_credential::CliCredentialService;
use crate::services::cli_image::CliImageService;
use crate::services::cli_image_build::CliImageBuildService;
use crate::services::cli_image_roll::CliImageRollService;
use crate::services::context::{ContextApprovalService, ContextFeedbackService};
use crate::services::context_envelope::ContextEnvelopeService;
use crate::services::context_feature::ContextFeatureService;
use crate::services::context_preview::ContextPreviewService;
use crate::services::dev_environment::{DevEnvironmentRuntime, DevEnvironmentService, DockerDevEnvironmentRuntime};
use crate::services::event::EventService;
use crate::services::favorite::FavoriteService;
use crate::services::feature_flag::FeatureFlagService;
use crate::services::gateway_terminal::GatewayTerminalService;
use crate::services::git_credential::GitCredentialService;
use crate::services::governance_audit::GovernanceAuditService;
use crate::services::group::GroupService;
use crate::services::inbox::InboxService;
use crate::services::legacy_navigation::LegacyNavigationService;
use crate::services::license::LicenseService;
use crate::services::llm_provider::LlmProviderService;
use crate::services::memory::MemoryService;
use crate::services::orchestration::OrchestrationService;
use crate::services::organization::OrganizationService;
use crate::services::plugin::PluginService;
use crate::services::pool::PoolService;
use crate::services::project::ProjectService;
use crate::services::prompt_library::PromptLibraryService;
use crate::services::quota::QuotaService;
use crate::services::resource_member::ResourceMemberService;
use crate::services::resource_profile::ResourceProfileService;
use crate::services::setting::SettingService;
use crate::services::skill::SkillService;
use crate::services::ssh_key::SshKeyService;
use crate::services::task_context::TaskContextService;
use crate::services::team::TeamService;
use crate::services::tile::TileService;
use crate::services::turn::TurnService;
use crate::services::usage_analytics::UsageAnalyticsService;
use crate::services::user::UserService;
use crate::services::voice::VoiceService;
use crate::services::workspace::WorkspaceService;

impl AppState {
    pub(crate) fn admin_service(&self) -> AdminService {
        AdminService::from_runtime(self.pool.clone(), self.auth_callout.clone())
    }

    pub(crate) fn cli_image_service(&self) -> CliImageService {
        CliImageService::from_runtime(self.pool.clone(), self.cli_image_status.clone(), &self.config)
    }

    pub(crate) fn cli_image_build_service(&self) -> CliImageBuildService {
        CliImageBuildService::from_runtime(
            self.cli_image_status.clone(),
            self.docker.clone(),
            // Same toast sink the auto-updater uses (`broadcast.admin.cli_image`).
            self.nats.client().cloned(),
            &self.config,
        )
    }

    pub(crate) fn cli_image_roll_service(&self) -> CliImageRollService {
        CliImageRollService::from_runtime(
            self.pool.clone(),
            &self.config,
            self.context_features,
            self.encryption_key,
            self.docker.clone(),
            self.auth_callout.clone(),
            self.cli_image_roll_inflight.clone(),
        )
    }

    pub(crate) fn agent_service(&self) -> AgentService {
        AgentService::from_pool_with_workspace(self.pool.clone())
    }

    pub(crate) fn agent_message_service(&self) -> AgentMessageService {
        AgentMessageService::from_pool(self.pool.clone())
    }

    pub(crate) fn agent_prompt_service(&self) -> AgentPromptService {
        AgentPromptService::from_runtime(
            self.pool.clone(),
            self.llm_factory.clone(),
            self.encryption_key,
            self.agent_command_bus.clone(),
            self.nats.clone(),
            self.inflight_prompts.clone(),
        )
    }

    pub(crate) fn agent_container_lifecycle_service(&self) -> AgentContainerLifecycleService {
        AgentContainerLifecycleService::from_runtime(self.pool.clone(), self.docker.clone())
    }

    pub(crate) fn agent_container_control_service(&self) -> AgentContainerControlService {
        AgentContainerControlService::from_runtime(
            self.pool.clone(),
            &self.config,
            self.context_features,
            self.encryption_key,
            self.docker.clone(),
            self.auth_callout.clone(),
        )
    }

    pub(crate) fn host_agent_enrollment_service(&self) -> HostAgentEnrollmentService {
        HostAgentEnrollmentService::from_runtime(self.pool.clone(), &self.config, self.context_features)
    }

    pub(crate) fn analytics_service(&self) -> AnalyticsService {
        AnalyticsService::from_pool(self.pool.clone())
    }

    pub(crate) fn analytics_usage_service(&self) -> UsageAnalyticsService {
        UsageAnalyticsService::new(self.pool.clone())
    }

    pub(crate) fn api_key_service(&self) -> ApiKeyService {
        ApiKeyService::from_pool(self.pool.clone())
    }

    pub(crate) fn attachment_service(&self) -> AttachmentService {
        AttachmentService::from_pool_and_app_config(self.pool.clone(), self.object_storage.clone(), &self.config)
    }

    pub(crate) fn audit_service(&self) -> AuditService {
        AuditService::from_pool(self.pool.clone())
    }

    pub(crate) fn auth_user_service(&self) -> UserService {
        UserService::from_app_config(self.pool.clone(), self.jwt.clone(), self.email_sender.clone(), &self.config)
    }

    pub(crate) fn auth_service(&self) -> AuthService {
        AuthService::from_pool(self.pool.clone(), self.jwt.clone())
    }

    pub(crate) fn billing_service(&self) -> BillingService {
        BillingService::from_runtime(self.pool.clone(), self.billing_gateway.clone())
    }

    pub(crate) fn cli_auth_proxy_service(&self) -> CliAuthProxyService {
        CliAuthProxyService::from_pool_and_app_config(
            self.pool.clone(),
            &self.config,
            self.encryption_key,
            self.redis.clone(),
            self.cli_auth_memory_store.clone(),
        )
    }

    pub(crate) fn cli_credential_service(&self) -> CliCredentialService {
        CliCredentialService::from_pool_and_app_config(self.pool.clone(), self.encryption_key, &self.config)
    }

    pub(crate) fn context_approval_service(&self) -> ContextApprovalService {
        ContextApprovalService::from_runtime(self.pool.clone(), self.nats.clone())
    }

    pub(crate) fn context_feedback_service(&self) -> ContextFeedbackService {
        ContextFeedbackService::new(self.pool.clone())
    }

    pub(crate) fn context_envelope_service(&self) -> ContextEnvelopeService {
        ContextEnvelopeService::from_runtime(self.pool.clone(), self.context_resolver.clone())
    }

    pub(crate) fn context_preview_service(&self) -> ContextPreviewService {
        ContextPreviewService::from_runtime(self.pool.clone(), self.context_resolver.clone())
    }

    pub(crate) fn context_feature_service(&self) -> ContextFeatureService {
        ContextFeatureService::from_runtime(self.pool.clone(), self.context_features)
    }

    pub(crate) fn dev_environment_service(&self) -> DevEnvironmentService {
        let runtime = self
            .docker
            .as_ref()
            .map(|docker| Arc::new(DockerDevEnvironmentRuntime::new(docker.clone())) as Arc<dyn DevEnvironmentRuntime>);
        DevEnvironmentService::from_runtime(self.pool.clone(), runtime)
    }

    pub(crate) fn event_service(&self) -> EventService {
        EventService::from_pool(self.pool.clone())
    }

    pub(crate) fn favorite_service(&self) -> FavoriteService {
        FavoriteService::from_pool(self.pool.clone())
    }

    pub(crate) fn feature_flag_service(&self) -> FeatureFlagService {
        FeatureFlagService::from_pool(self.pool.clone())
    }

    pub(crate) fn git_credential_service(&self) -> GitCredentialService {
        GitCredentialService::from_pool(self.pool.clone(), self.encryption_key)
    }

    pub(crate) fn gateway_terminal_service(&self) -> GatewayTerminalService {
        GatewayTerminalService::from_pool(self.pool.clone())
    }

    pub(crate) fn governance_audit_service(&self) -> AppResult<GovernanceAuditService> {
        GovernanceAuditService::from_pool_and_app_config(self.pool.clone(), &self.config, self.encryption_key)
    }

    pub(crate) fn group_service(&self) -> GroupService {
        GroupService::from_pool(self.pool.clone())
    }

    pub(crate) fn inbox_service(&self) -> InboxService {
        InboxService::from_pool(self.pool.clone())
    }

    pub(crate) fn legacy_navigation_service(&self) -> LegacyNavigationService {
        LegacyNavigationService::from_pool(self.pool.clone())
    }

    pub(crate) fn license_service(&self) -> LicenseService {
        LicenseService::from_pool(self.pool.clone())
    }

    pub(crate) fn llm_provider_service(&self) -> LlmProviderService {
        LlmProviderService::from_pool(self.pool.clone(), self.encryption_key, self.llm_factory.clone())
    }

    pub(crate) fn memory_service(&self) -> MemoryService {
        MemoryService::new(self.pool.clone())
    }

    pub(crate) fn organization_service(&self) -> OrganizationService {
        OrganizationService::from_pool(self.pool.clone())
    }

    pub(crate) fn orchestration_service(&self) -> OrchestrationService {
        OrchestrationService::from_runtime(
            self.pool.clone(),
            self.context_features,
            self.context_resolver.clone(),
            self.nats.clone(),
        )
    }

    pub(crate) fn plugin_service(&self) -> PluginService {
        PluginService::from_pool(self.pool.clone())
    }

    pub(crate) fn pool_service(&self) -> PoolService {
        PoolService::new(self.docker.clone())
    }

    pub(crate) fn project_service(&self) -> ProjectService {
        ProjectService::from_pool(self.pool.clone())
    }

    pub(crate) fn prompt_library_service(&self) -> PromptLibraryService {
        PromptLibraryService::from_pool(self.pool.clone())
    }

    pub(crate) fn quota_service(&self) -> QuotaService {
        QuotaService::from_pool(self.pool.clone())
    }

    pub(crate) fn resource_member_service(&self) -> ResourceMemberService {
        ResourceMemberService::from_pool(self.pool.clone())
    }

    pub(crate) fn resource_profile_service(&self) -> ResourceProfileService {
        ResourceProfileService::from_pool(self.pool.clone())
    }

    pub(crate) fn setting_service(&self) -> SettingService {
        SettingService::from_runtime(self.pool.clone(), self.docker.clone())
    }

    pub(crate) fn skill_service(&self) -> SkillService {
        SkillService::from_pool(self.pool.clone())
    }

    pub(crate) fn ssh_key_service(&self) -> SshKeyService {
        SshKeyService::from_pool(self.pool.clone())
    }

    pub(crate) fn task_context_service(&self) -> TaskContextService {
        TaskContextService::from_pool(self.pool.clone())
    }

    pub(crate) fn team_service(&self) -> TeamService {
        TeamService::from_pool(self.pool.clone())
    }

    pub(crate) fn tile_service(&self) -> TileService {
        TileService::from_pool(self.pool.clone())
    }

    pub(crate) fn turn_service(&self) -> TurnService {
        TurnService::from_pool(self.pool.clone())
    }

    pub(crate) fn user_service(&self) -> UserService {
        UserService::from_pool(self.pool.clone(), self.jwt.clone())
    }

    pub(crate) fn voice_service(&self) -> VoiceService {
        VoiceService::from_pool(self.pool.clone())
    }

    pub(crate) fn workspace_service(&self) -> WorkspaceService {
        WorkspaceService::from_pool(self.pool.clone())
    }

    /// Build a `GithubAppClient` from the four `github_app_*` config fields,
    /// or `None` if the GitHub App integration is not configured.
    #[allow(dead_code)]
    pub(crate) fn github_app_client(&self) -> Option<crate::services::github_app::GithubAppClient> {
        crate::services::github_app::build_github_app_client(&self.config)
    }

    /// Build the self-fix PR Bridge service. Carries the (optional) GitHub App
    /// client; `open_pr` fails with a visible error when it is `None`.
    #[allow(dead_code)]
    pub(crate) fn self_fix_service(&self) -> crate::services::self_fix::SelfFixService {
        crate::services::self_fix::SelfFixService::new(
            crate::repositories::orchestration::OrchestrationTaskRepository::new(self.pool.clone()),
            crate::repositories::agent::AgentRepository::new(self.pool.clone()),
            self.agent_container_control_service(),
            self.github_app_client(),
            crate::services::agent_workspace::workspace_root_from_env(),
            crate::services::self_fix::import::ImportLimits::default(),
        )
    }
}
