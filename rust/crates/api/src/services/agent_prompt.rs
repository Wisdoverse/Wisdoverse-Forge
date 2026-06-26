//! Agent prompt service.
//!
//! Coordinates Container CLI command publishing, provider-backed prompt
//! streaming, and in-flight stream cancellation outside HTTP handlers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentforge_core::{AgentId, AppResult, TenantScope};
use agentforge_infra::{NatsClient, ObjectStorageClient};
use agentforge_llm::LlmProviderFactory;
use agentforge_llm::provider::ContentBlock;
use base64::Engine;
use futures::{StreamExt, stream::BoxStream};
use sqlx::PgPool;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::domain::agent::PlainTextAgentPrompt;
use crate::domain::instruction_image;
use crate::domain::prompt::{PromptAgentPolicy, SseFrame};
use crate::repositories::agent::{AgentRepository, MessageRepository};
use crate::repositories::attachment::AttachmentRepository;
use crate::repositories::user::llm_config::UserLlmConfigRepository;
use crate::services::agent_commands::{AgentCommandBus, AgentCommandService};
use crate::services::prompt::{PromptService, UserLlmConfigKeyResolver};

/// Max images attachable to one instruction (bounds request size + provider cost).
const MAX_INSTRUCTION_IMAGES: usize = 8;

pub(crate) type InflightPromptMap = Arc<Mutex<HashMap<AgentId, oneshot::Sender<()>>>>;

pub(crate) enum AgentPromptDispatch {
    Sidecar,
    ProviderStream { frames: BoxStream<'static, AppResult<SseFrame>> },
}

pub(crate) struct AgentPromptService {
    agents: Arc<AgentRepository>,
    messages: Arc<MessageRepository>,
    attachments: Arc<AttachmentRepository>,
    object_storage: Arc<ObjectStorageClient>,
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
        attachments: Arc<AttachmentRepository>,
        object_storage: Arc<ObjectStorageClient>,
        llm_configs: Arc<UserLlmConfigRepository>,
        llm_factory: Arc<LlmProviderFactory>,
        encryption_key: Option<[u8; 32]>,
        command_bus: Option<Arc<dyn AgentCommandBus>>,
        nats: Arc<NatsClient>,
        inflight_prompts: InflightPromptMap,
    ) -> Self {
        Self {
            agents,
            messages,
            attachments,
            object_storage,
            llm_configs,
            llm_factory,
            encryption_key,
            command_bus,
            nats,
            inflight_prompts,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_runtime(
        pool: PgPool,
        object_storage: Arc<ObjectStorageClient>,
        llm_factory: Arc<LlmProviderFactory>,
        encryption_key: Option<[u8; 32]>,
        command_bus: Option<Arc<dyn AgentCommandBus>>,
        nats: Arc<NatsClient>,
        inflight_prompts: InflightPromptMap,
    ) -> Self {
        Self::new(
            Arc::new(AgentRepository::new(pool.clone())),
            Arc::new(MessageRepository::new(pool.clone())),
            Arc::new(AttachmentRepository::new(pool.clone())),
            object_storage,
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
        let has_images = images.is_some_and(|ids| !ids.is_empty());

        if agent.cli_tool.is_some() {
            if has_images {
                // The CLI quick-message path does not execute the CLI (it only
                // acks); images for CLI agents ride the task-dispatch path.
                return Err(instruction_image::cli_quick_message_images_unsupported());
            }
            self.send_sidecar_prompt(agent_id, prompt.content()).await?;
            return Ok(AgentPromptDispatch::Sidecar);
        }

        // Resolve + authorize (capability + org + workspace + kind) + fetch images
        // BEFORE persisting the user turn, so a rejected multimodal request never
        // leaves an orphaned text-only history row. `required_model` is pure
        // pre-persist validation, so resolving it first (to gate images on the
        // model, not just the provider) keeps the fail-closed-before-persist
        // contract intact.
        let model = PromptAgentPolicy::required_model(agent.model.clone())?;
        let image_blocks = if has_images {
            self.resolve_image_blocks(&scope, &agent, &model, images.unwrap_or_default()).await?
        } else {
            Vec::new()
        };

        let system_prompt = agent.system_prompt.clone();
        let prompt_service = self.provider_prompt_service();

        // Build the stream before registering in-flight state. This preserves
        // the existing contract: early validation/provider errors return
        // without leaving a stuck busy entry.
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let frames = prompt_service
            .stream(scope, agent_id, model, system_prompt, prompt.content().to_owned(), image_blocks, cancel_rx)
            .await?;
        let frames = self.register_inflight_stream(agent_id, cancel_tx, frames)?;

        Ok(AgentPromptDispatch::ProviderStream { frames })
    }

    /// Resolve instruction image attachment IDs to base64 content blocks for a
    /// provider/API agent, enforcing the capability + tenant + workspace + kind
    /// boundary. Fails closed on any violation before the prompt is persisted.
    async fn resolve_image_blocks(
        &self,
        scope: &TenantScope,
        agent: &agentforge_db::entities::Agent,
        model: &str,
        image_ids: &[String],
    ) -> AppResult<Vec<ContentBlock>> {
        let provider = agent.provider.clone().ok_or_else(instruction_image::agent_has_no_provider)?;
        // Gate on the specific (provider, model): the provider-level
        // `capability_profile` reports vision for every first-party model, so a
        // text-only model on a vision-capable provider would otherwise pass here
        // and be rejected upstream only after the user uploaded.
        if !agentforge_llm::vision::model_supports_image(&provider, model) {
            return Err(instruction_image::model_does_not_support_images(model, &provider));
        }
        if image_ids.len() > MAX_INSTRUCTION_IMAGES {
            return Err(instruction_image::too_many_instruction_images(MAX_INSTRUCTION_IMAGES));
        }

        let mut blocks = Vec::with_capacity(image_ids.len());
        for id in image_ids {
            let uuid = Uuid::parse_str(id.trim()).map_err(|_| instruction_image::invalid_attachment_id())?;
            let attachment = self.attachments.get(scope, uuid).await?; // org-scoped
            if attachment.kind != "image" {
                return Err(instruction_image::attachment_not_an_image(uuid));
            }
            // CLAUDE.md execution boundary: an image may only be used by an agent
            // in the same workspace. A cross-workspace reference is a 404.
            if attachment.workspace_id != Some(agent.workspace_id.as_uuid()) {
                return Err(instruction_image::image_not_found(uuid));
            }
            let bytes = self.object_storage.get_bytes(&attachment.storage_path).await?;
            let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
            blocks.push(ContentBlock::Image { media_type: attachment.content_type, data });
        }
        Ok(blocks)
    }

    pub(crate) async fn interrupt_sidecar(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        self.agents.find_by_id(scope, agent_id).await?;
        self.send_sidecar_interrupt(agent_id).await
    }

    pub(crate) async fn interrupt_provider_stream(&self, scope: &TenantScope, agent_id: AgentId) -> AppResult<()> {
        self.agents.find_by_id(scope, agent_id).await?;
        let mut map = self.inflight_prompts.lock().unwrap_or_else(|e| e.into_inner());
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
            let mut map = self.inflight_prompts.lock().unwrap_or_else(|e| e.into_inner());
            PromptAgentPolicy::ensure_not_busy(map.contains_key(&agent_id))?;
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
