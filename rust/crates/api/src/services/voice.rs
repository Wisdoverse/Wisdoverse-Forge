//! Voice service — provider management and status.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::VoiceProvider;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::voice::{VoiceProviderDraft, VoiceProviderType, VoiceStatusProjection};
pub(crate) use crate::domain::voice::{
    voice_data_response, voice_delete_response, voice_transcription_pending_response,
};
use crate::repositories::voice::VoiceRepository;

/// Business logic layer for voice operations.
pub struct VoiceService {
    repo: VoiceRepository,
}

impl VoiceService {
    pub fn new(repo: VoiceRepository) -> Self {
        Self { repo }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(VoiceRepository::new(pool))
    }

    /// Get voice service status (stub).
    pub(crate) async fn status(&self, scope: &TenantScope) -> AppResult<VoiceStatusProjection> {
        let providers = self.repo.list(scope).await?;
        let has_default = providers.iter().any(|p| p.is_default);
        Ok(VoiceStatusProjection::new(providers.len(), has_default))
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
        let draft = VoiceProviderDraft::parse(name, provider_type)?;
        self.repo.create(scope, draft.name(), draft.provider_type(), config).await
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
        let provider_type = provider_type.map(VoiceProviderType::parse).transpose()?.map(VoiceProviderType::value);
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
