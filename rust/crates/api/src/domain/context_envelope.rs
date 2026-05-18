//! Domain policies for runtime-neutral context envelopes.

use agentforge_core::context_envelope::{
    CONTEXT_ENVELOPE_VERSION_V1, ContextEnvelopeCapability, ContextEnvelopeItem, ContextEnvelopeItemKind,
    ContextEnvelopeSource, SUPPORTED_CONTEXT_ENVELOPE_VERSIONS,
};
use agentforge_core::{AppResult, ErrorKind, RuntimeCapability};
use uuid::Uuid;

use super::context_resolver::DegradationReason;

pub(crate) struct ContextEnvelopeVersionPolicy;

impl ContextEnvelopeVersionPolicy {
    pub(crate) fn ensure_v1_supported(supported_versions: &[String]) -> AppResult<()> {
        if supported_versions.iter().any(|version| version == CONTEXT_ENVELOPE_VERSION_V1) {
            return Ok(());
        }
        Err(ErrorKind::Validation(format!(
            "unsupported context envelope version; supported versions: {}",
            SUPPORTED_CONTEXT_ENVELOPE_VERSIONS.join(", ")
        ))
        .into())
    }
}

pub(crate) struct ContextEnvelopeCapabilityPolicy;

impl ContextEnvelopeCapabilityPolicy {
    pub(crate) fn snapshot(capability: &RuntimeCapability) -> ContextEnvelopeCapability {
        ContextEnvelopeCapability {
            cli_tool: capability
                .cli_tool
                .map(|cli_tool| cli_tool.as_str().to_string())
                .or_else(|| capability.provider_name.clone())
                .unwrap_or_else(|| "provider".to_string()),
            runtime_kind: capability.runtime_kind.as_str().to_string(),
            max_context_tokens: capability.max_context_tokens,
            supports_skills_mount: capability.supports_skills_mount,
            supports_hooks: capability.supports_hooks,
            supports_subagents: capability.supports_subagents,
        }
    }
}

pub(crate) struct ContextEnvelopeMemoryContentPolicy;

impl ContextEnvelopeMemoryContentPolicy {
    pub(crate) fn visible_content(
        content: &str,
        content_redacted: bool,
        content_encrypted: bool,
        sensitivity: &str,
    ) -> String {
        if content_redacted || content_encrypted { format!("[redacted: {sensitivity}]") } else { content.to_string() }
    }
}

pub(crate) struct ContextEnvelopeMemoryItem<'a> {
    pub(crate) id: Uuid,
    pub(crate) title: &'a str,
    pub(crate) content: &'a str,
    pub(crate) content_redacted: bool,
    pub(crate) content_encrypted: bool,
    pub(crate) sensitivity: &'a str,
}

impl ContextEnvelopeMemoryItem<'_> {
    pub(crate) fn to_envelope_item(&self) -> ContextEnvelopeItem {
        ContextEnvelopeItem {
            id: self.id,
            kind: ContextEnvelopeItemKind::Memory,
            title: self.title.to_string(),
            content: ContextEnvelopeMemoryContentPolicy::visible_content(
                self.content,
                self.content_redacted,
                self.content_encrypted,
                self.sensitivity,
            ),
            content_ref: format!("memory_items/{}", self.id),
            sensitivity: self.sensitivity.to_string(),
            source: ContextEnvelopeSource {
                source_type: "memory_item".to_string(),
                source_id: Some(self.id),
                title: Some(self.title.to_string()),
            },
        }
    }
}

pub(crate) struct ContextEnvelopeDegradationPolicy;

impl ContextEnvelopeDegradationPolicy {
    pub(crate) fn labels(reasons: &[DegradationReason]) -> Vec<String> {
        reasons.iter().map(DegradationReason::label).map(str::to_string).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_policy_requires_v1_support() {
        assert!(ContextEnvelopeVersionPolicy::ensure_v1_supported(&["v1".to_string()]).is_ok());

        let err = ContextEnvelopeVersionPolicy::ensure_v1_supported(&["v0".to_string()]).unwrap_err();
        assert!(matches!(
            err.kind,
            ErrorKind::Validation(message) if message.contains("unsupported context envelope version")
        ));
    }

    #[test]
    fn capability_snapshot_prefers_provider_for_provider_backed_runtime() {
        let capability = RuntimeCapability::api_provider_or_default("openai", 8192);

        let snapshot = ContextEnvelopeCapabilityPolicy::snapshot(&capability);

        assert_eq!(snapshot.cli_tool, "openai");
        assert_eq!(snapshot.runtime_kind, "api");
        assert_eq!(snapshot.max_context_tokens, 8192);
    }

    #[test]
    fn memory_content_policy_hides_redacted_or_encrypted_content() {
        assert_eq!(
            ContextEnvelopeMemoryContentPolicy::visible_content("secret", true, false, "restricted"),
            "[redacted: restricted]"
        );
        assert_eq!(
            ContextEnvelopeMemoryContentPolicy::visible_content("secret", false, true, "internal"),
            "[redacted: internal]"
        );
        assert_eq!(ContextEnvelopeMemoryContentPolicy::visible_content("plain", false, false, "internal"), "plain");
    }

    #[test]
    fn memory_item_owns_envelope_contract() {
        let id = Uuid::from_u128(0x11111111111141118111111111111111);
        let item = ContextEnvelopeMemoryItem {
            id,
            title: "Project notes",
            content: "secret",
            content_redacted: true,
            content_encrypted: false,
            sensitivity: "restricted",
        }
        .to_envelope_item();

        assert_eq!(item.id, id);
        assert_eq!(item.kind, ContextEnvelopeItemKind::Memory);
        assert_eq!(item.title, "Project notes");
        assert_eq!(item.content, "[redacted: restricted]");
        assert_eq!(item.content_ref, format!("memory_items/{id}"));
        assert_eq!(item.sensitivity, "restricted");
        assert_eq!(item.source.source_type, "memory_item");
        assert_eq!(item.source.source_id, Some(id));
        assert_eq!(item.source.title.as_deref(), Some("Project notes"));
    }

    #[test]
    fn degradation_policy_owns_protocol_labels() {
        assert_eq!(
            ContextEnvelopeDegradationPolicy::labels(&[
                DegradationReason::BudgetTruncated,
                DegradationReason::RuntimeCapabilityFallback,
            ]),
            vec!["budget_truncated".to_string(), "runtime_capability_fallback".to_string()]
        );
    }
}
