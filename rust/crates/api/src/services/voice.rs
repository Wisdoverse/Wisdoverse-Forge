//! Voice service — provider management and status.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::VoiceProvider;
use uuid::Uuid;

use crate::repositories::voice::VoiceRepository;

/// Valid voice provider types.
const VALID_PROVIDER_TYPES: &[&str] = &["openai", "deepgram", "elevenlabs", "custom"];

/// Business logic layer for voice operations.
pub struct VoiceService {
    repo: VoiceRepository,
}

impl VoiceService {
    pub fn new(repo: VoiceRepository) -> Self {
        Self { repo }
    }

    /// Get voice service status (stub).
    pub async fn status(&self, scope: &TenantScope) -> AppResult<serde_json::Value> {
        let providers = self.repo.list(scope).await?;
        let has_default = providers.iter().any(|p| p.is_default);
        Ok(serde_json::json!({
            "enabled": !providers.is_empty(),
            "provider_count": providers.len(),
            "has_default": has_default,
        }))
    }

    /// List voice providers.
    pub async fn list_providers(&self, scope: &TenantScope) -> AppResult<Vec<VoiceProvider>> {
        self.repo.list(scope).await
    }

    /// Add a voice provider.
    pub async fn add_provider(
        &self,
        scope: &TenantScope,
        name: &str,
        provider_type: &str,
        config: &serde_json::Value,
    ) -> AppResult<VoiceProvider> {
        let name = name.trim();
        if name.is_empty() || name.len() > 255 {
            return Err(ErrorKind::Validation("name must be 1-255 characters".into()).into());
        }
        if !VALID_PROVIDER_TYPES.contains(&provider_type) {
            return Err(
                ErrorKind::Validation(format!("provider_type must be one of: {:?}", VALID_PROVIDER_TYPES)).into()
            );
        }
        self.repo.create(scope, name, provider_type, config).await
    }

    /// Update a voice provider.
    pub async fn update_provider(
        &self,
        scope: &TenantScope,
        id: Uuid,
        name: Option<&str>,
        provider_type: Option<&str>,
        config: Option<&serde_json::Value>,
    ) -> AppResult<VoiceProvider> {
        if let Some(pt) = provider_type
            && !VALID_PROVIDER_TYPES.contains(&pt)
        {
            return Err(
                ErrorKind::Validation(format!("provider_type must be one of: {:?}", VALID_PROVIDER_TYPES)).into()
            );
        }
        self.repo.update(scope, id, name, provider_type, config).await
    }

    /// Remove a voice provider.
    pub async fn remove_provider(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }

    /// Set a provider as default.
    pub async fn set_default(&self, scope: &TenantScope, id: Uuid) -> AppResult<VoiceProvider> {
        self.repo.set_default(scope, id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_provider_types() {
        assert!(VALID_PROVIDER_TYPES.contains(&"openai"));
        assert!(VALID_PROVIDER_TYPES.contains(&"deepgram"));
        assert!(VALID_PROVIDER_TYPES.contains(&"elevenlabs"));
        assert!(VALID_PROVIDER_TYPES.contains(&"custom"));
    }

    #[test]
    fn invalid_provider_type_rejected() {
        assert!(!VALID_PROVIDER_TYPES.contains(&"azure"));
        assert!(!VALID_PROVIDER_TYPES.contains(&""));
    }
}
