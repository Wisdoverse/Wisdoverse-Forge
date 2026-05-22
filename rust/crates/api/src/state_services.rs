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
use crate::services::agent_message::AgentMessageService;
use crate::services::agent_prompt::AgentPromptService;
use crate::services::attachment::AttachmentService;
use crate::services::billing::BillingService;
use crate::services::cli_auth_proxy::CliAuthProxyService;
use crate::services::cli_credential::CliCredentialService;
use crate::services::context::{ContextApprovalService, ContextFeedbackService};
use crate::services::context_envelope::ContextEnvelopeService;
use crate::services::context_feature::ContextFeatureService;
use crate::services::context_preview::ContextPreviewService;
use crate::services::dev_environment::{DevEnvironmentRuntime, DevEnvironmentService, DockerDevEnvironmentRuntime};
use crate::services::gateway_terminal::GatewayTerminalService;
use crate::services::git_credential::GitCredentialService;
use crate::services::governance_audit::GovernanceAuditService;
use crate::services::llm_provider::LlmProviderService;
use crate::services::memory::MemoryService;
use crate::services::orchestration::OrchestrationService;
use crate::services::pool::PoolService;
use crate::services::task_context::TaskContextService;
use crate::services::usage_analytics::UsageAnalyticsService;
use crate::services::user::UserService;

impl AppState {
    pub(crate) fn admin_service(&self) -> AdminService {
        AdminService::from_runtime(self.pool.clone(), self.auth_callout.clone())
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

    pub(crate) fn analytics_usage_service(&self) -> UsageAnalyticsService {
        UsageAnalyticsService::new(self.pool.clone())
    }

    pub(crate) fn attachment_service(&self) -> AttachmentService {
        AttachmentService::from_pool_and_app_config(self.pool.clone(), self.object_storage.clone(), &self.config)
    }

    pub(crate) fn auth_user_service(&self) -> UserService {
        UserService::from_app_config(self.pool.clone(), self.jwt.clone(), self.email_sender.clone(), &self.config)
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

    pub(crate) fn git_credential_service(&self) -> GitCredentialService {
        GitCredentialService::from_pool(self.pool.clone(), self.encryption_key)
    }

    pub(crate) fn gateway_terminal_service(&self) -> GatewayTerminalService {
        GatewayTerminalService::from_pool(self.pool.clone())
    }

    pub(crate) fn governance_audit_service(&self) -> AppResult<GovernanceAuditService> {
        GovernanceAuditService::from_pool_and_app_config(self.pool.clone(), &self.config, self.encryption_key)
    }

    pub(crate) fn llm_provider_service(&self) -> LlmProviderService {
        LlmProviderService::from_pool(self.pool.clone(), self.encryption_key, self.llm_factory.clone())
    }

    pub(crate) fn memory_service(&self) -> MemoryService {
        MemoryService::new(self.pool.clone())
    }

    pub(crate) fn orchestration_service(&self) -> OrchestrationService {
        OrchestrationService::from_runtime(
            self.pool.clone(),
            self.context_features,
            self.context_resolver.clone(),
            self.nats.clone(),
        )
    }

    pub(crate) fn pool_service(&self) -> PoolService {
        PoolService::new(self.docker.clone())
    }

    pub(crate) fn task_context_service(&self) -> TaskContextService {
        TaskContextService::from_pool(self.pool.clone())
    }

    pub(crate) fn user_service(&self) -> UserService {
        UserService::from_pool(self.pool.clone(), self.jwt.clone())
    }
}
