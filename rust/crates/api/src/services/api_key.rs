//! API key service — generation, validation, and lifecycle management.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::ApiKey;
use chrono::{DateTime, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

pub use crate::domain::credential::CreateApiKeyResult;
use crate::domain::credential::{
    ApiKeyAuthenticationPolicy, ApiKeyFormat, ApiKeyName, ApiKeyScopePolicy, CredentialListPage,
};
pub(crate) use crate::domain::credential::{
    api_key_create_response, api_key_list_response, credential_delete_response,
};
use crate::repositories::credential::api_key::ApiKeyRepository;

/// Business logic layer for API key operations.
pub struct ApiKeyService {
    repo: ApiKeyRepository,
}

impl ApiKeyService {
    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(ApiKeyRepository::new(pool))
    }

    pub fn new(repo: ApiKeyRepository) -> Self {
        Self { repo }
    }

    /// Generate a new API key, hash it, store the hash, and return the plaintext once.
    pub async fn create_key(
        &self,
        scope: &TenantScope,
        name: &str,
        scopes: &[String],
        expires_at: Option<DateTime<Utc>>,
    ) -> AppResult<CreateApiKeyResult> {
        let name = ApiKeyName::parse(name)?;
        ApiKeyScopePolicy::validate(scopes)?;

        // Generate random key: af_ + 64 hex chars (32 random bytes)
        let (plaintext_key, key_hash, _prefix) = generate_api_key_parts();
        let key_prefix = &plaintext_key[..11]; // "af_" + first 8 hex chars

        let key = self.repo.create(scope, name.value(), &key_hash, key_prefix, scopes, expires_at).await?;

        Ok(CreateApiKeyResult { key, plaintext_key })
    }

    /// List API keys (paginated, no plaintext).
    pub async fn list_keys(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<ApiKey>> {
        let page = CredentialListPage::new(limit, offset);
        self.repo.list(scope, page.limit(), page.offset()).await
    }

    /// Revoke an API key by ID.
    pub async fn revoke_key(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.revoke(scope, id).await
    }

    /// Validate a raw API key: hash, lookup, check revocation and expiry.
    pub async fn validate_key(&self, raw_key: &str) -> AppResult<ApiKey> {
        ApiKeyAuthenticationPolicy::ensure_format(raw_key)?;

        let key_hash = hash_key(raw_key);
        let key = ApiKeyAuthenticationPolicy::require_key(self.repo.find_by_hash(&key_hash).await?)?;

        ApiKeyAuthenticationPolicy::ensure_not_revoked(key.revoked_at.is_some())?;

        ApiKeyAuthenticationPolicy::ensure_not_expired(key.expires_at, Utc::now())?;

        // Update last_used (fire-and-forget, but log failures)
        if let Err(err) = self.repo.update_last_used(key.id).await {
            tracing::warn!(error = ?err, key_id = %key.id, "Failed to update last_used_at");
        }

        Ok(key)
    }
}

/// Generate a random API key: `af_` followed by 64 hex characters (32 random bytes).
/// Returns (plaintext_key, hash, prefix).
pub(crate) fn generate_api_key_parts() -> (String, String, String) {
    let key = generate_api_key();
    let hash = hash_key(&key);
    let prefix = key[3..11].to_string();
    (key, hash, prefix)
}

/// Generate a random API key: `af_` followed by 64 hex characters (32 random bytes).
pub(crate) fn generate_api_key() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 32] = rng.random();
    format!("{}{}", ApiKeyFormat::PREFIX, hex::encode(bytes))
}

/// SHA-256 hash an API key and return the hex digest.
pub(crate) fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}
