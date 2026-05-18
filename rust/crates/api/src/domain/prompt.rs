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
