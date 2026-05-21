//! Agent prompt service.
//!
//! Coordinates Container CLI command publishing, provider-backed prompt
//! streaming, and in-flight stream cancellation outside HTTP handlers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentforge_core::{AgentId, AppResult, ErrorKind, TenantScope};
use agentforge_infra::NatsClient;
use agentforge_llm::LlmProviderFactory;
use futures::{StreamExt, stream::BoxStream};
use sqlx::PgPool;
use tokio::sync::oneshot;

use crate::domain::agent::PlainTextAgentPrompt;
use crate::domain::prompt::SseFrame;
use crate::repositories::agent::{AgentRepository, MessageRepository};
use crate::repositories::user::llm_config::UserLlmConfigRepository;
use crate::services::agent_commands::{AgentCommandBus, AgentCommandService};
use crate::services::prompt::{PromptService, UserLlmConfigKeyResolver};

pub(crate) type InflightPromptMap = Arc<Mutex<HashMap<AgentId, oneshot::Sender<()>>>>;

pub(crate) enum AgentPromptDispatch {
    Sidecar,
    ProviderStream { frames: BoxStream<'static, AppResult<SseFrame>> },
}

pub(crate) struct AgentPromptService {
    agents: Arc<AgentRepository>,
    messages: Arc<MessageRepository>,
    llm_configs: Arc<UserLlmConfigRepository>,
    llm_factory: Arc<LlmProviderFactory>,
    encryption_key: Option<[u8; 32]>,
    command_bus: Option<Arc<dyn AgentCommandBus>>,
    nats: Arc<NatsClient>,
    inflight_prompts: InflightPromptMap,
}

impl AgentPromptService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        agents: Arc<AgentRepository>,
        messages: Arc<MessageRepository>,
        llm_configs: Arc<UserLlmConfigRepository>,
        llm_factory: Arc<LlmProviderFactory>,
        encryption_key: Option<[u8; 32]>,
        command_bus: Option<Arc<dyn AgentCommandBus>>,
        nats: Arc<NatsClient>,
        inflight_prompts: InflightPromptMap,
    ) -> Self {
        Self { agents, messages, llm_configs, llm_factory, encryption_key, command_bus, nats, inflight_prompts }
    }

    pub(crate) fn from_runtime(
        pool: PgPool,
        llm_factory: Arc<LlmProviderFactory>,
        encryption_key: Option<[u8; 32]>,
        command_bus: Option<Arc<dyn AgentCommandBus>>,
        nats: Arc<NatsClient>,
        inflight_prompts: InflightPromptMap,
    ) -> Self {
        Self::new(
            Arc::new(AgentRepository::new(pool.clone())),
            Arc::new(MessageRepository::new(pool.clone())),
            Arc::new(UserLlmConfigRepository::new(pool)),
            llm_factory,
            encryption_key,
            command_bus,
            nats,
            inflight_prompts,
        )
    }

    pub(crate) async fn send_prompt(
        &self,
        scope: TenantScope,
        agent_id: AgentId,
        content: &str,
        images: Option<&[String]>,
    ) -> AppResult<AgentPromptDispatch> {
        let prompt = PlainTextAgentPrompt::new(content, images)?;
        let agent = self.agents.find_by_id(&scope, agent_id).await?;

        if agent.cli_tool.is_some() {
            self.send_sidecar_prompt(agent_id, prompt.content()).await?;
            return Ok(AgentPromptDispatch::Sidecar);
        }

        let model = agent.model.clone().ok_or_else(|| ErrorKind::Validation("agent has no model configured".into()))?;
        let system_prompt = agent.system_prompt.clone();
        let prompt_service = self.provider_prompt_service();

        // Build the stream before registering in-flight state. This preserves
        // the existing contract: early validation/provider errors return
        // without leaving a stuck busy entry.
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let frames = prompt_service
            .stream(scope, agent_id, model, system_prompt, prompt.content().to_owned(), cancel_rx)
            .await?;
        let frames = self.register_inflight_stream(agent_id, cancel_tx, frames)?;

        Ok(AgentPromptDispatch::ProviderStream { frames })
    }

    pub(crate) async fn interrupt_sidecar(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        self.agents.find_by_id(scope, agent_id).await?;
        self.send_sidecar_interrupt(agent_id).await
    }

    pub(crate) async fn interrupt_provider_stream(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        self.agents.find_by_id(scope, agent_id).await?;
        let mut map = self.inflight_prompts.lock().expect("inflight_prompts poisoned");
        if let Some(tx) = map.remove(&agent_id) {
            let _ = tx.send(());
        }
        Ok(())
    }

    fn provider_prompt_service(&self) -> PromptService {
        let keys = Arc::new(UserLlmConfigKeyResolver::new(self.llm_configs.clone(), self.encryption_key));
        PromptService::new(self.messages.clone(), self.agents.clone(), self.llm_factory.clone(), keys)
    }

    async fn send_sidecar_prompt(&self, agent_id: AgentId, content: &str) -> AppResult<()> {
        match self.command_bus.clone() {
            Some(bus) => AgentCommandService::new(bus).send_prompt(&agent_id.to_string(), content).await,
            None => AgentCommandService::new(self.nats.clone()).send_prompt(&agent_id.to_string(), content).await,
        }
    }

    async fn send_sidecar_interrupt(&self, agent_id: AgentId) -> AppResult<()> {
        match self.command_bus.clone() {
            Some(bus) => AgentCommandService::new(bus).interrupt(&agent_id.to_string()).await,
            None => AgentCommandService::new(self.nats.clone()).interrupt(&agent_id.to_string()).await,
        }
    }

    fn register_inflight_stream(
        &self,
        agent_id: AgentId,
        cancel_tx: oneshot::Sender<()>,
        frames: BoxStream<'static, AppResult<SseFrame>>,
    ) -> AppResult<BoxStream<'static, AppResult<SseFrame>>> {
        {
            let mut map = self.inflight_prompts.lock().expect("inflight_prompts poisoned");
            if map.contains_key(&agent_id) {
                return Err(ErrorKind::Conflict("agent_busy".into()).into());
            }
            map.insert(agent_id, cancel_tx);
        }

        let guard = Arc::new(InflightPromptGuard { map: self.inflight_prompts.clone(), agent_id });
        Ok(Box::pin(frames.map(move |frame| {
            let _hold = guard.clone();
            frame
        })))
    }
}

struct InflightPromptGuard {
    map: InflightPromptMap,
    agent_id: AgentId,
}

impl Drop for InflightPromptGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = self.map.lock() {
            map.remove(&self.agent_id);
        }
    }
}
