//! PromptService — orchestrates the SSE streaming loop for provider+prompt agents.

use agentforge_core::{AgentId, AppResult, ErrorKind, MessageId, TenantScope};
use agentforge_db::entities::AgentMessage;
use agentforge_llm::provider::{ChatMessage, ChatRequest, StreamDelta, model_context_limit};
use agentforge_llm::{LlmProviderBuildConfig, LlmProviderFactory};
use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::domain::prompt::{
    PromptContent, PromptContextPolicy, PromptHistoryMessage, SseFrame, sse_error_for_llm_error,
};
use crate::repositories::agent::AgentRepository;
use crate::repositories::message::MessageRepository;

fn prompt_context_policy(model: &str) -> PromptContextPolicy {
    PromptContextPolicy::new(model, model_context_limit(model))
}

fn output_token_budget(model: &str) -> u32 {
    prompt_context_policy(model).output_token_budget()
}

/// Build the ordered message history to send to the LLM, bounded by the
/// model's context window. Drops oldest messages until the budget fits.
///
/// Truncation strategy: iterate newest-to-oldest and keep the contiguous
/// suffix that fits. The first oversized message encountered halts the
/// walk, so anything older than that message is dropped even if it would
/// individually fit — the alternative (skip-and-keep) would present an
/// incoherent conversation to the model.
pub fn build_history(all_msgs: &[AgentMessage], system_prompt: &str, model: &str) -> AppResult<Vec<ChatMessage>> {
    let policy = prompt_context_policy(model);
    let history: Vec<_> =
        all_msgs.iter().map(|message| PromptHistoryMessage::new(&message.role, &message.content)).collect();
    Ok(policy
        .select_history(&history, system_prompt)?
        .into_iter()
        .map(|message| ChatMessage { role: message.role().to_string(), content: message.content().to_string() })
        .collect())
}

#[cfg(test)]
mod build_history_tests {
    use super::*;
    use agentforge_core::{AgentId, MessageId, OrgId};
    use chrono::Utc;

    fn msg(role: &str, content: &str) -> AgentMessage {
        AgentMessage {
            id: MessageId::new(),
            organization_id: OrgId::from(uuid::Uuid::nil()),
            agent_id: AgentId::from(uuid::Uuid::nil()),
            run_id: None,
            role: role.into(),
            content: content.into(),
            tokens_in: None,
            tokens_out: None,
            model: None,
            finish_reason: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn empty_history_with_no_prompt_is_validation_error() {
        let r = build_history(&[], "", "claude-sonnet-4-6");
        assert!(r.is_err());
    }

    #[test]
    fn short_history_all_kept_in_order() {
        let h = vec![msg("user", "hi"), msg("assistant", "hello"), msg("user", "how are you")];
        let r = build_history(&h, "you are helpful", "claude-sonnet-4-6").unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].content, "hi");
        assert_eq!(r[2].content, "how are you");
    }

    #[test]
    fn old_messages_over_budget_are_dropped() {
        // llama3.2 → limit 8192, budget = 6553 - 2048 = 4505 tokens ≈ 18020 chars.
        let huge = "x".repeat(20_000);
        let h = vec![msg("user", &huge), msg("user", "short tail")];
        let r = build_history(&h, "", "llama3.2").unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].content, "short tail");
    }

    #[test]
    fn unknown_model_fallback_4k_keeps_short_history() {
        let h = vec![msg("user", "hi")];
        let r = build_history(&h, "", "mystery-model").unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].content, "hi");
    }

    #[test]
    fn output_budget_scales_down_for_small_or_unknown_models() {
        assert_eq!(output_token_budget("mystery-model"), 1_024);
        assert_eq!(output_token_budget("llama3.2"), 2_048);
        assert_eq!(output_token_budget("claude-sonnet-4-6"), 4_096);
    }

    #[test]
    fn system_prompt_over_budget_returns_error() {
        let sys = "y".repeat(20_000);
        let h = vec![msg("user", "hi")];
        let r = build_history(&h, &sys, "llama3.2");
        assert!(format!("{}", r.unwrap_err().kind).contains("system_prompt alone"));
    }

    #[test]
    fn single_user_message_over_budget_returns_error() {
        let huge = "x".repeat(50_000);
        let h = vec![msg("user", &huge)];
        let r = build_history(&h, "", "llama3.2");
        assert!(format!("{}", r.unwrap_err().kind).contains("exceeds context budget"));
    }
}

// ---------------------------------------------------------------------------
// KeyResolver trait + production impl
// ---------------------------------------------------------------------------

/// Per-request API-key resolver. Trait so tests can mock without touching
/// the real `user_llm_configs` table / encryption infra.
#[async_trait]
pub trait KeyResolver: Send + Sync {
    async fn resolve(&self, scope: &TenantScope, provider: &str) -> AppResult<LlmProviderCredential>;
}

#[derive(Debug, Clone)]
pub struct LlmProviderCredential {
    pub api_key: String,
    pub base_url: Option<String>,
}

/// Production impl: reads `user_llm_configs`, decrypts with `AppState.encryption_key`.
pub struct UserLlmConfigKeyResolver {
    repo: Arc<crate::repositories::user_llm_config::UserLlmConfigRepository>,
    encryption_key: Option<[u8; 32]>,
}

impl UserLlmConfigKeyResolver {
    pub fn new(
        repo: Arc<crate::repositories::user_llm_config::UserLlmConfigRepository>,
        encryption_key: Option<[u8; 32]>,
    ) -> Self {
        Self { repo, encryption_key }
    }
}

#[async_trait]
impl KeyResolver for UserLlmConfigKeyResolver {
    async fn resolve(&self, scope: &TenantScope, provider: &str) -> AppResult<LlmProviderCredential> {
        if provider == "ollama" {
            let base_url = self.repo.find_default_secret(scope, provider).await?.and_then(|secret| secret.base_url);
            return Ok(LlmProviderCredential { api_key: String::new(), base_url }); // keyless local
        }
        let secret = self.repo.find_default_secret(scope, provider).await?.ok_or_else(|| {
            ErrorKind::Validation(format!("no API key configured for provider '{provider}' — add one in LLM settings"))
        })?;
        let key = self
            .encryption_key
            .ok_or_else(|| ErrorKind::Internal(anyhow::anyhow!("LLM_ENCRYPTION_KEY not configured")))?;
        let api_key = agentforge_core::crypto::decrypt_base64(&key, &secret.encrypted_api_key)
            .map_err(|e| ErrorKind::Internal(anyhow::anyhow!("decrypt api_key failed: {e}")))?;
        Ok(LlmProviderCredential { api_key, base_url: secret.base_url })
    }
}

// ---------------------------------------------------------------------------
// PromptService
// ---------------------------------------------------------------------------

pub struct PromptService {
    messages: Arc<MessageRepository>,
    agents: Arc<AgentRepository>,
    factory: Arc<LlmProviderFactory>,
    keys: Arc<dyn KeyResolver>,
}

impl PromptService {
    pub fn new(
        messages: Arc<MessageRepository>,
        agents: Arc<AgentRepository>,
        factory: Arc<LlmProviderFactory>,
        keys: Arc<dyn KeyResolver>,
    ) -> Self {
        Self { messages, agents, factory, keys }
    }

    /// Run the LLM stream end-to-end. Returns a stream of SSE frames the route
    /// handler will serialize into the HTTP response body.
    ///
    /// **Cancellation semantics.** The finalize block (persist assistant row
    /// with `finish_reason = interrupted/error/stop`) runs when the inner
    /// `async_stream` generator is polled to its end — either because
    /// `llm_stream` drained, because `cancel_rx` fired, or because the
    /// provider returned an error. It does NOT run if the caller drops the
    /// returned stream future before it is polled to completion (e.g. HTTP
    /// client disconnects and the route handler drops the response body).
    /// In that case the partial assistant buffer is lost.
    ///
    /// The route layer (T12) is responsible for bridging client disconnect
    /// to `cancel_tx.send(())` so the finalize block still runs. Callers
    /// that need the flush guarantee MUST trigger cancel rather than drop.
    pub async fn stream(
        &self,
        scope: TenantScope,
        agent_id: AgentId,
        model: String,
        system_prompt: Option<String>,
        content: String,
        mut cancel_rx: oneshot::Receiver<()>,
    ) -> AppResult<BoxStream<'static, AppResult<SseFrame>>> {
        let content = PromptContent::parse(&content)?.value().to_string();
        self.messages.insert(&scope, agent_id, "user", &content, None, None, None, None).await?;

        let all = self.messages.list(&scope, agent_id, 1_000, None).await?;
        let sys = system_prompt.clone().unwrap_or_default();
        let history = build_history(&all, &sys, &model)?;

        let req = ChatRequest {
            model: model.clone(),
            messages: {
                let mut v = Vec::with_capacity(history.len() + 1);
                if !sys.is_empty() {
                    v.push(ChatMessage { role: "system".into(), content: sys.clone() });
                }
                v.extend(history);
                v
            },
            max_tokens: Some(output_token_budget(&model)),
            temperature: None,
        };

        let provider_name = self
            .agents
            .find_by_id(&scope, agent_id)
            .await?
            .provider
            .ok_or_else(|| ErrorKind::Validation("agent has no provider configured".into()))?;
        let credential = self.keys.resolve(&scope, &provider_name).await?;
        let provider_instance = self
            .factory
            .build_with_config(LlmProviderBuildConfig {
                provider_key: provider_name.clone(),
                api_key: credential.api_key,
                base_url: credential.base_url,
            })
            .map_err(|e| ErrorKind::Validation(format!("{e}")))?;
        // TODO(T12): remap `LlmError::Api { status, .. }` to user-remediable `ErrorKind`
        // (Validation / Unauthorized / Conflict / RateLimited) at the route layer so
        // the frontend can distinguish "your API key is wrong" (401) from a real 500.
        let mut llm_stream =
            provider_instance.stream(req).await.map_err(|e| ErrorKind::Internal(anyhow::anyhow!(e)))?;

        let messages_repo = self.messages.clone();
        let message_id = MessageId::new();
        let scope_for_finalize = scope.clone();
        let model_for_stream = model.clone();

        let ssestream = async_stream::stream! {
            yield Ok(SseFrame::MessageStart { message_id: message_id.as_uuid(), model: model_for_stream.clone() });
            let mut buffer = String::new();
            let mut tokens_in: u32 = 0;
            let mut tokens_out: u32 = 0;
            let mut finish_reason = "stop".to_string();
            let mut errored = false;
            let mut cancelled = false;

            loop {
                tokio::select! {
                    biased;
                    _ = &mut cancel_rx => {
                        cancelled = true;
                        finish_reason = "interrupted".into();
                        break;
                    }
                    next = llm_stream.next() => match next {
                        None => break,
                        Some(Ok(StreamDelta::Text(t))) => {
                            buffer.push_str(&t);
                            yield Ok(SseFrame::Delta { text: t });
                        }
                        Some(Ok(StreamDelta::Usage { input_tokens, output_tokens })) => {
                            tokens_in = input_tokens;
                            tokens_out = output_tokens;
                        }
                        Some(Ok(StreamDelta::Done { finish_reason: fr })) => {
                            finish_reason = fr;
                        }
                        Some(Err(e)) => {
                            errored = true;
                            finish_reason = "error".into();
                            let (code, message, retryable) = sse_error_for_llm_error(&e);
                            tracing::error!(
                                error = %e,
                                agent_id = %agent_id,
                                message_id = %message_id,
                                "LLM stream error — redacted to client"
                            );
                            yield Ok(SseFrame::Error {
                                code: code.into(),
                                message: message.into(),
                                retryable,
                            });
                            break;
                        }
                    }
                }
            }

            if !buffer.is_empty() {
                let persist_reason = if errored {
                    "error"
                } else if cancelled {
                    "interrupted"
                } else {
                    finish_reason.as_str()
                };
                // Persist under the SAME UUID that `message_start` emitted.
                if let Err(e) = messages_repo
                    .insert_with_id(
                        message_id,
                        &scope_for_finalize, agent_id, "assistant", &buffer,
                        Some(tokens_in as i32), Some(tokens_out as i32),
                        Some(&model_for_stream), Some(persist_reason),
                    )
                    .await
                {
                    tracing::error!(
                        error = %e.kind,
                        message_id = %message_id,
                        agent_id = %agent_id,
                        "failed to persist assistant message — client received message_start with this UUID but no DB row exists"
                    );
                }
            }
            if !errored && !cancelled {
                yield Ok(SseFrame::MessageStop { tokens_in, tokens_out, finish_reason });
            }
        };

        Ok(Box::pin(ssestream))
    }
}

// ---------------------------------------------------------------------------
// stream_tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod stream_tests {
    use super::*;
    use agentforge_llm::LlmProviderFactory;
    use sqlx::PgPool;
    use std::sync::Arc;
    use uuid::Uuid;

    /// Test-only KeyResolver that returns a fixed string without touching the DB.
    struct MockKeyResolver {
        key: String,
    }
    impl MockKeyResolver {
        fn with_key(k: &str) -> Self {
            Self { key: k.into() }
        }
    }
    #[async_trait]
    impl KeyResolver for MockKeyResolver {
        async fn resolve(&self, _scope: &TenantScope, _provider: &str) -> AppResult<LlmProviderCredential> {
            Ok(LlmProviderCredential { api_key: self.key.clone(), base_url: None })
        }
    }

    /// Seed one org + workspace + user + membership, then an agent with the
    /// supplied `provider`. Mirrors the private helper in
    /// `repositories/message.rs` tests; duplicated here to keep T11 scoped.
    async fn seed_agent_with_provider(pool: &PgPool, provider: &str) -> (TenantScope, AgentId) {
        let org_uuid = Uuid::new_v4();
        let user_uuid = Uuid::new_v4();
        let agent_uuid = Uuid::new_v4();

        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
            .bind(org_uuid)
            .bind(format!("Org {org_uuid}"))
            .bind(format!("org-{org_uuid}"))
            .execute(pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_uuid)
            .execute(pool)
            .await
            .expect("seed workspace");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(user_uuid)
            .bind(format!("u-{user_uuid}@example.com"))
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
            .bind(org_uuid)
            .bind(user_uuid)
            .execute(pool)
            .await
            .expect("seed membership");
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, user_id, provider, status)
             VALUES ($1, $2, $2, $3, $4, 'idle')",
        )
        .bind(agent_uuid)
        .bind(org_uuid)
        .bind(user_uuid)
        .bind(provider)
        .execute(pool)
        .await
        .expect("seed agent");

        let scope = crate::test_support::tenant_scope_for_ids(org_uuid, user_uuid);
        (scope, AgentId::from(agent_uuid))
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn happy_path_inserts_user_and_assistant(pool: PgPool) {
        let (scope, agent_id) = seed_agent_with_provider(&pool, "mock").await;
        let messages = Arc::new(MessageRepository::new(pool.clone()));
        let agents = Arc::new(AgentRepository::new(pool.clone()));
        let factory = Arc::new(LlmProviderFactory::with_mock("mock", "hello world"));
        let keys: Arc<dyn KeyResolver> = Arc::new(MockKeyResolver::with_key("k"));
        let svc = PromptService::new(messages.clone(), agents, factory, keys);
        let repo = messages.clone();

        let (_cancel_tx, cancel_rx) = oneshot::channel::<()>();
        // Keep _cancel_tx alive so the channel stays open (not cancelled).

        let mut s = svc
            .stream(scope.clone(), agent_id, "claude-sonnet-4-6".into(), None, "hi".into(), cancel_rx)
            .await
            .expect("build stream");
        let mut frames = Vec::new();
        while let Some(frame) = s.next().await {
            frames.push(frame.expect("frame ok"));
        }

        // Expect: message_start, delta("hello world"), message_stop.
        assert!(matches!(frames.first(), Some(SseFrame::MessageStart { .. })), "first frame is message_start");
        assert!(
            frames.iter().any(|f| matches!(f, SseFrame::Delta { text } if text == "hello world")),
            "delta frame carries mock reply"
        );
        assert!(
            matches!(frames.last(), Some(SseFrame::MessageStop { finish_reason, .. }) if finish_reason == "stop"),
            "last frame is message_stop finish=stop"
        );

        let msgs = repo.list(&scope, agent_id, 50, None).await.expect("list");
        assert_eq!(msgs.len(), 2, "one user + one assistant persisted");
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "hi");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].content, "hello world");
        assert_eq!(msgs[1].finish_reason.as_deref(), Some("stop"));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn empty_content_is_validation_error(pool: PgPool) {
        let (scope, agent_id) = seed_agent_with_provider(&pool, "mock").await;
        let messages = Arc::new(MessageRepository::new(pool.clone()));
        let agents = Arc::new(AgentRepository::new(pool.clone()));
        let factory = Arc::new(LlmProviderFactory::with_mock("mock", ""));
        let keys: Arc<dyn KeyResolver> = Arc::new(MockKeyResolver::with_key("k"));
        let svc = PromptService::new(messages, agents, factory, keys);
        let (_tx, rx) = oneshot::channel();
        let result = svc.stream(scope, agent_id, "claude-sonnet-4-6".into(), None, "   ".into(), rx).await;
        assert!(result.is_err(), "whitespace content should fail validation");
        let err = result.err().expect("already checked is_err");
        assert!(
            format!("{}", err.kind).contains("prompt content is required"),
            "error message mentions prompt content, got: {}",
            err.kind
        );
    }
}
