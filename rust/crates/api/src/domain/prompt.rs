//! Prompt input and context-window policies.

use agentforge_core::{AppResult, ErrorKind};

/// Validated prompt content accepted by provider-backed prompt agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PromptContent<'a> {
    value: &'a str,
}

impl<'a> PromptContent<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        let value = value.trim();
        if value.is_empty() {
            return Err(ErrorKind::Validation("prompt content is required".into()).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

pub(crate) struct PromptAgentPolicy;

impl PromptAgentPolicy {
    pub(crate) fn required_model(model: Option<String>) -> AppResult<String> {
        model.ok_or_else(|| ErrorKind::Validation("agent has no model configured".into()).into())
    }

    pub(crate) fn required_provider(provider: Option<String>) -> AppResult<String> {
        provider.ok_or_else(|| ErrorKind::Validation("agent has no provider configured".into()).into())
    }

    pub(crate) fn ensure_not_busy(is_busy: bool) -> AppResult<()> {
        if is_busy {
            return Err(ErrorKind::Conflict("agent_busy".into()).into());
        }
        Ok(())
    }
}

pub(crate) struct PromptProviderPolicy;

impl PromptProviderPolicy {
    pub(crate) fn missing_api_key(provider: &str) -> ErrorKind {
        ErrorKind::Validation(format!("no API key configured for provider '{provider}' — add one in LLM settings"))
    }

    pub(crate) fn build_error(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Validation(format!("{err}"))
    }
}

/// Borrowed chat history item used by the context-window selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PromptHistoryMessage<'a> {
    role: &'a str,
    content: &'a str,
}

impl<'a> PromptHistoryMessage<'a> {
    pub(crate) fn new(role: &'a str, content: &'a str) -> Self {
        Self { role, content }
    }

    pub(crate) fn role(self) -> &'a str {
        self.role
    }

    pub(crate) fn content(self) -> &'a str {
        self.content
    }
}

/// Conservative character-to-token estimator. Provider-specific tokenizers can
/// override this in a later follow-up; this is the bounded baseline.
fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 4 + 1
}

/// Context-window budget and history truncation policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PromptContextPolicy {
    model: String,
    context_limit: usize,
    output_token_budget: u32,
}

impl PromptContextPolicy {
    pub(crate) fn new(model: &str, context_limit: usize) -> Self {
        Self {
            model: model.to_string(),
            context_limit,
            output_token_budget: Self::output_token_budget_for_limit(context_limit),
        }
    }

    pub(crate) fn output_token_budget(&self) -> u32 {
        self.output_token_budget
    }

    pub(crate) fn select_history<'a>(
        &self,
        all_msgs: &[PromptHistoryMessage<'a>],
        system_prompt: &str,
    ) -> AppResult<Vec<PromptHistoryMessage<'a>>> {
        let budget = self.history_budget()?;
        let sys_tokens = estimate_tokens(system_prompt);
        if sys_tokens >= budget {
            return Err(ErrorKind::Validation("system_prompt alone exceeds context budget".into()).into());
        }

        let mut used = sys_tokens;
        let mut kept = Vec::new();
        for message in all_msgs.iter().rev() {
            let tokens = estimate_tokens(message.content());
            if used + tokens > budget {
                break;
            }
            used += tokens;
            kept.push(*message);
        }
        kept.reverse();

        if kept.is_empty() {
            return Err(ErrorKind::Validation("message exceeds context budget; clear chat to continue".into()).into());
        }

        Ok(kept)
    }

    fn output_token_budget_for_limit(context_limit: usize) -> u32 {
        ((context_limit / 4).clamp(256, 4_096)) as u32
    }

    fn history_budget(&self) -> AppResult<usize> {
        let raw = ((self.context_limit as f32) * 0.8) as usize;
        let budget = raw.saturating_sub(self.output_token_budget as usize);
        if budget == 0 {
            return Err(ErrorKind::Validation(format!(
                "model '{}' context window too small for reserved output budget",
                self.model
            ))
            .into());
        }
        Ok(budget)
    }
}

/// Server-sent SSE frame for the chat stream. Serialized to
/// `event: <name>\ndata: <json>` by the route handler.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "event", content = "data")]
pub enum SseFrame {
    #[serde(rename = "message_start")]
    MessageStart { message_id: uuid::Uuid, model: String },
    #[serde(rename = "delta")]
    Delta { text: String },
    #[serde(rename = "message_stop")]
    MessageStop { tokens_in: u32, tokens_out: u32, finish_reason: String },
    #[serde(rename = "error")]
    Error { code: String, message: String, retryable: bool },
}

impl SseFrame {
    /// Split into `(event_name, data_payload)` for SSE transport.
    /// This is compiler-enforced coverage of all variants — if a new variant
    /// is added without updating this match, the match becomes non-exhaustive
    /// and the build fails. The `#[serde(tag, content)]` attribute is an
    /// implementation detail of direct JSON serialization and should not
    /// influence the SSE transport layer.
    pub fn split(&self) -> (&'static str, serde_json::Value) {
        match self {
            SseFrame::MessageStart { message_id, model } => {
                ("message_start", serde_json::json!({ "message_id": message_id, "model": model }))
            }
            SseFrame::Delta { text } => ("delta", serde_json::json!({ "text": text })),
            SseFrame::MessageStop { tokens_in, tokens_out, finish_reason } => (
                "message_stop",
                serde_json::json!({
                    "tokens_in": tokens_in,
                    "tokens_out": tokens_out,
                    "finish_reason": finish_reason,
                }),
            ),
            SseFrame::Error { code, message, retryable } => {
                ("error", serde_json::json!({ "code": code, "message": message, "retryable": retryable }))
            }
        }
    }
}

/// Maps an `LlmError` to the client-safe SSE error frame triple
/// `(code, message, retryable)`. Upstream provider response bodies are never
/// echoed because they may contain snippets of the original request (user
/// content, model name, organization id). The triple is consumed by the
/// `SseFrame::Error` constructor in the prompt service.
pub(crate) fn sse_error_for_llm_error(err: &agentforge_llm::LlmError) -> (&'static str, &'static str, bool) {
    use agentforge_llm::LlmError;
    match err {
        LlmError::Api { status: 401, .. } | LlmError::Api { status: 403, .. } => {
            ("unauthorized", "provider rejected the API key — check LLM settings", false)
        }
        LlmError::Api { status: 429, .. } => ("rate_limited", "provider rate limit reached — try again shortly", true),
        LlmError::Api { status: 400, .. } | LlmError::Api { status: 404, .. } => {
            ("bad_request", "provider rejected the request — check model name", false)
        }
        LlmError::Api { status: 500..=599, .. } => ("provider_error", "provider server error — try again", true),
        LlmError::Http(_) => ("network", "network error reaching provider — try again", true),
        LlmError::NotConfigured(_) => ("not_configured", "provider not configured — check LLM settings", false),
        LlmError::NotImplemented(_) => ("not_implemented", "provider feature not available", false),
        LlmError::Parse(_) | LlmError::Api { .. } => {
            ("provider_error", "provider returned an unexpected response", true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg<'a>(role: &'a str, content: &'a str) -> PromptHistoryMessage<'a> {
        PromptHistoryMessage::new(role, content)
    }

    #[test]
    fn prompt_content_trims_and_rejects_empty() {
        assert_eq!(PromptContent::parse("  hi  ").unwrap().value(), "hi");
        assert!(PromptContent::parse("   ").is_err());
    }

    #[test]
    fn prompt_agent_policy_requires_model_provider_and_idle_state() {
        assert_eq!(PromptAgentPolicy::required_model(Some("gpt-5.5".to_string())).unwrap(), "gpt-5.5");
        assert!(PromptAgentPolicy::required_model(None).is_err());
        assert_eq!(PromptAgentPolicy::required_provider(Some("openai".to_string())).unwrap(), "openai");
        assert!(PromptAgentPolicy::required_provider(None).is_err());
        assert!(PromptAgentPolicy::ensure_not_busy(false).is_ok());
        assert!(PromptAgentPolicy::ensure_not_busy(true).is_err());
    }

    #[test]
    fn prompt_provider_policy_owns_user_visible_errors() {
        assert!(format!("{}", PromptProviderPolicy::missing_api_key("openai")).contains("provider 'openai'"));
        assert!(format!("{}", PromptProviderPolicy::build_error("bad model")).contains("bad model"));
    }

    #[test]
    fn empty_history_with_no_prompt_is_validation_error() {
        let policy = PromptContextPolicy::new("claude-sonnet-4-6", 200_000);
        assert!(policy.select_history(&[], "").is_err());
    }

    #[test]
    fn short_history_all_kept_in_order() {
        let policy = PromptContextPolicy::new("claude-sonnet-4-6", 200_000);
        let history = [msg("user", "hi"), msg("assistant", "hello"), msg("user", "how are you")];
        let selected = policy.select_history(&history, "you are helpful").unwrap();

        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].content(), "hi");
        assert_eq!(selected[2].content(), "how are you");
    }

    #[test]
    fn old_messages_over_budget_are_dropped() {
        let policy = PromptContextPolicy::new("llama3.2", 8_192);
        let huge = "x".repeat(20_000);
        let history = [msg("user", &huge), msg("user", "short tail")];
        let selected = policy.select_history(&history, "").unwrap();

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].content(), "short tail");
    }

    #[test]
    fn unknown_model_fallback_4k_keeps_short_history() {
        let policy = PromptContextPolicy::new("mystery-model", 4_096);
        let history = [msg("user", "hi")];
        let selected = policy.select_history(&history, "").unwrap();

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].content(), "hi");
    }

    #[test]
    fn output_budget_scales_down_for_small_or_unknown_models() {
        assert_eq!(PromptContextPolicy::new("mystery-model", 4_096).output_token_budget(), 1_024);
        assert_eq!(PromptContextPolicy::new("llama3.2", 8_192).output_token_budget(), 2_048);
        assert_eq!(PromptContextPolicy::new("claude-sonnet-4-6", 200_000).output_token_budget(), 4_096);
    }

    #[test]
    fn system_prompt_over_budget_returns_error() {
        let policy = PromptContextPolicy::new("llama3.2", 8_192);
        let sys = "y".repeat(20_000);
        let history = [msg("user", "hi")];
        let err = policy.select_history(&history, &sys).unwrap_err();

        assert!(format!("{}", err.kind).contains("system_prompt alone"));
    }

    #[test]
    fn sse_frame_split_returns_protocol_event_name_and_payload_for_each_variant() {
        let message_id = uuid::Uuid::from_u128(0xBBBB_BBBB_BBBB_BBBB_BBBB_BBBB_BBBB_BBBB);

        let (event, data) = SseFrame::MessageStart { message_id, model: "claude-sonnet-4-6".to_string() }.split();
        assert_eq!(event, "message_start");
        assert_eq!(data["message_id"], serde_json::json!(message_id));
        assert_eq!(data["model"], "claude-sonnet-4-6");

        let (event, data) = SseFrame::Delta { text: "hi".to_string() }.split();
        assert_eq!(event, "delta");
        assert_eq!(data["text"], "hi");

        let (event, data) =
            SseFrame::MessageStop { tokens_in: 12, tokens_out: 34, finish_reason: "stop".to_string() }.split();
        assert_eq!(event, "message_stop");
        assert_eq!(data["tokens_in"], 12);
        assert_eq!(data["tokens_out"], 34);
        assert_eq!(data["finish_reason"], "stop");

        let (event, data) =
            SseFrame::Error { code: "rate_limited".to_string(), message: "slow down".to_string(), retryable: true }
                .split();
        assert_eq!(event, "error");
        assert_eq!(data["code"], "rate_limited");
        assert_eq!(data["message"], "slow down");
        assert_eq!(data["retryable"], true);
    }

    #[test]
    fn sse_error_for_llm_error_redacts_provider_message_per_status() {
        use agentforge_llm::LlmError;

        assert_eq!(
            sse_error_for_llm_error(&LlmError::Api { status: 401, message: "secret model leak".to_string() }),
            ("unauthorized", "provider rejected the API key — check LLM settings", false),
        );
        assert_eq!(
            sse_error_for_llm_error(&LlmError::Api { status: 403, message: "secret org id".to_string() }),
            ("unauthorized", "provider rejected the API key — check LLM settings", false),
        );
        assert_eq!(
            sse_error_for_llm_error(&LlmError::Api { status: 429, message: "slow down".to_string() }),
            ("rate_limited", "provider rate limit reached — try again shortly", true),
        );
        assert_eq!(
            sse_error_for_llm_error(&LlmError::Api { status: 400, message: "bad model".to_string() }),
            ("bad_request", "provider rejected the request — check model name", false),
        );
        assert_eq!(
            sse_error_for_llm_error(&LlmError::Api { status: 404, message: "no such model".to_string() }),
            ("bad_request", "provider rejected the request — check model name", false),
        );
        assert_eq!(
            sse_error_for_llm_error(&LlmError::Api { status: 502, message: "upstream down".to_string() }),
            ("provider_error", "provider server error — try again", true),
        );
        assert_eq!(
            sse_error_for_llm_error(&LlmError::Api { status: 418, message: "teapot".to_string() }),
            ("provider_error", "provider returned an unexpected response", true),
        );
        assert_eq!(
            sse_error_for_llm_error(&LlmError::NotConfigured("missing key".to_string())),
            ("not_configured", "provider not configured — check LLM settings", false),
        );
        assert_eq!(
            sse_error_for_llm_error(&LlmError::NotImplemented("streaming".to_string())),
            ("not_implemented", "provider feature not available", false),
        );
        assert_eq!(
            sse_error_for_llm_error(&LlmError::Parse("truncated json".to_string())),
            ("provider_error", "provider returned an unexpected response", true),
        );
    }

    #[test]
    fn single_user_message_over_budget_returns_error() {
        let policy = PromptContextPolicy::new("llama3.2", 8_192);
        let huge = "x".repeat(50_000);
        let history = [msg("user", &huge)];
        let err = policy.select_history(&history, "").unwrap_err();

        assert!(format!("{}", err.kind).contains("exceeds context budget"));
    }
}
