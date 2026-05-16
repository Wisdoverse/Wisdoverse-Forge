//! Voice domain rules.
//!
//! This module owns voice provider catalog policies that are independent of
//! repositories, HTTP route DTOs, and persistence details.

use agentforge_core::{AppResult, ErrorKind};

const VALID_PROVIDER_TYPES: &[&str] = &["openai", "deepgram", "elevenlabs", "custom"];
const MAX_PROVIDER_NAME_LEN: usize = 255;

/// Voice provider display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VoiceProviderName<'a> {
    value: &'a str,
}

impl<'a> VoiceProviderName<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        let value = value.trim();
        if value.is_empty() || value.len() > MAX_PROVIDER_NAME_LEN {
            return Err(ErrorKind::Validation(format!("name must be 1-{MAX_PROVIDER_NAME_LEN} characters")).into());
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Supported voice provider type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VoiceProviderType<'a> {
    value: &'a str,
}

impl<'a> VoiceProviderType<'a> {
    pub(crate) fn parse(value: &'a str) -> AppResult<Self> {
        if !VALID_PROVIDER_TYPES.contains(&value) {
            return Err(
                ErrorKind::Validation(format!("provider_type must be one of: {:?}", VALID_PROVIDER_TYPES)).into()
            );
        }
        Ok(Self { value })
    }

    pub(crate) fn value(self) -> &'a str {
        self.value
    }
}

/// Validated voice provider creation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VoiceProviderDraft<'a> {
    name: VoiceProviderName<'a>,
    provider_type: VoiceProviderType<'a>,
}

impl<'a> VoiceProviderDraft<'a> {
    pub(crate) fn parse(name: &'a str, provider_type: &'a str) -> AppResult<Self> {
        Ok(Self { name: VoiceProviderName::parse(name)?, provider_type: VoiceProviderType::parse(provider_type)? })
    }

    pub(crate) fn name(self) -> &'a str {
        self.name.value()
    }

    pub(crate) fn provider_type(self) -> &'a str {
        self.provider_type.value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_provider_type_accepts_known_types() {
        assert_eq!(VoiceProviderType::parse("openai").unwrap().value(), "openai");
        assert_eq!(VoiceProviderType::parse("deepgram").unwrap().value(), "deepgram");
        assert_eq!(VoiceProviderType::parse("elevenlabs").unwrap().value(), "elevenlabs");
        assert_eq!(VoiceProviderType::parse("custom").unwrap().value(), "custom");
    }

    #[test]
    fn voice_provider_type_rejects_unknown_types() {
        assert!(VoiceProviderType::parse("azure").is_err());
        assert!(VoiceProviderType::parse("").is_err());
    }

    #[test]
    fn voice_provider_draft_trims_and_validates_name() {
        let draft = VoiceProviderDraft::parse("  OpenAI TTS  ", "openai").unwrap();
        assert_eq!(draft.name(), "OpenAI TTS");
        assert_eq!(draft.provider_type(), "openai");
    }

    #[test]
    fn voice_provider_draft_rejects_invalid_names() {
        assert!(VoiceProviderDraft::parse("", "openai").is_err());
        assert!(VoiceProviderDraft::parse("   ", "openai").is_err());
        assert!(VoiceProviderDraft::parse(&"a".repeat(256), "openai").is_err());
    }
}
