//! API key service — generation, validation, and lifecycle management.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::ApiKey;
use chrono::{DateTime, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::repositories::api_key::ApiKeyRepository;

/// The prefix for all generated API keys.
const KEY_PREFIX: &str = "af_";

/// Valid scopes for API keys.
const VALID_SCOPES: &[&str] = &["read", "write", "admin"];

/// Result of creating an API key — includes the plaintext key (shown once).
#[derive(Debug, serde::Serialize)]
pub struct CreateApiKeyResult {
    pub key: ApiKey,
    /// The plaintext API key — only returned at creation time.
    pub plaintext_key: String,
}

/// Business logic layer for API key operations.
pub struct ApiKeyService {
    repo: ApiKeyRepository,
}

impl ApiKeyService {
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
        // Validate name
        if let Err(msg) = validate_key_name(name) {
            return Err(ErrorKind::Validation(msg.into()).into());
        }
        let name = name.trim();

        // Validate scopes
        if let Err(msg) = validate_scopes(scopes) {
            return Err(ErrorKind::Validation(msg).into());
        }

        // Generate random key: af_ + 64 hex chars (32 random bytes)
        let (plaintext_key, key_hash, _prefix) = generate_api_key_parts();
        let key_prefix = &plaintext_key[..11]; // "af_" + first 8 hex chars

        let key = self.repo.create(scope, name, &key_hash, key_prefix, scopes, expires_at).await?;

        Ok(CreateApiKeyResult { key, plaintext_key })
    }

    /// List API keys (paginated, no plaintext).
    pub async fn list_keys(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<ApiKey>> {
        let limit = limit.clamp(1, 100);
        let offset = offset.max(0);
        self.repo.list(scope, limit, offset).await
    }

    /// Revoke an API key by ID.
    pub async fn revoke_key(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.revoke(scope, id).await
    }

    /// Validate a raw API key: hash, lookup, check revocation and expiry.
    pub async fn validate_key(&self, raw_key: &str) -> AppResult<ApiKey> {
        // Basic format check
        if validate_key_format(raw_key).is_err() {
            return Err(ErrorKind::Unauthorized.into());
        }

        let key_hash = hash_key(raw_key);
        let key = self.repo.find_by_hash(&key_hash).await?.ok_or(ErrorKind::Unauthorized)?;

        // Check if revoked
        if key.revoked_at.is_some() {
            return Err(ErrorKind::Unauthorized.into());
        }

        // Check if expired
        if let Some(expires_at) = key.expires_at
            && expires_at < Utc::now()
        {
            return Err(ErrorKind::Unauthorized.into());
        }

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
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.r#gen();
    format!("{}{}", KEY_PREFIX, hex::encode(bytes))
}

/// SHA-256 hash an API key and return the hex digest.
pub(crate) fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Validate API key format: must start with "af_" and be exactly 67 chars.
pub(crate) fn validate_key_format(key: &str) -> Result<(), &'static str> {
    if !key.starts_with(KEY_PREFIX) {
        return Err("key must start with 'af_'");
    }
    if key.len() != 67 {
        return Err("key must be exactly 67 characters");
    }
    // Verify hex portion
    if hex::decode(&key[3..]).is_err() {
        return Err("key must contain valid hex characters after prefix");
    }
    Ok(())
}

/// Validate that all scopes are in the allowed set.
pub(crate) fn validate_scopes(scopes: &[String]) -> Result<(), String> {
    for s in scopes {
        if !VALID_SCOPES.contains(&s.as_str()) {
            return Err(format!("invalid scope '{}', valid: {:?}", s, VALID_SCOPES));
        }
    }
    Ok(())
}

/// Validate API key name: must be 1-255 characters after trimming.
pub(crate) fn validate_key_name(name: &str) -> Result<(), &'static str> {
    let name = name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err("name must be 1-255 characters");
    }
    Ok(())
}
