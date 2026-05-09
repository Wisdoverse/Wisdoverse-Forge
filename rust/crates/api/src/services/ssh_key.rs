//! SSH key service — validation and management.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::SshKey;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::repositories::ssh_key::SshKeyRepository;

/// Supported SSH key type prefixes.
const VALID_KEY_PREFIXES: &[&str] =
    &["ssh-ed25519", "ssh-rsa", "ecdsa-sha2-nistp256", "ecdsa-sha2-nistp384", "ecdsa-sha2-nistp521"];

/// Business logic layer for SSH key operations.
pub struct SshKeyService {
    repo: SshKeyRepository,
}

impl SshKeyService {
    pub fn new(repo: SshKeyRepository) -> Self {
        Self { repo }
    }

    /// Add a new SSH key after validating format and computing fingerprint.
    pub async fn add_key(&self, scope: &TenantScope, name: &str, public_key: &str) -> AppResult<SshKey> {
        let name = name.trim();
        if name.is_empty() || name.len() > 255 {
            return Err(ErrorKind::Validation("name must be 1-255 characters".into()).into());
        }

        let public_key = public_key.trim();
        let key_type = validate_public_key(public_key)?;
        let fingerprint = compute_fingerprint(public_key);

        self.repo.create(scope, name, public_key, &fingerprint, &key_type).await
    }

    /// List SSH keys (paginated).
    pub async fn list_keys(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<SshKey>> {
        let limit = limit.clamp(1, 100);
        let offset = offset.max(0);
        self.repo.list(scope, limit, offset).await
    }

    /// Get an SSH key by ID.
    pub async fn get_key(&self, scope: &TenantScope, id: Uuid) -> AppResult<SshKey> {
        self.repo.find_by_id(scope, id).await
    }

    /// Delete an SSH key by ID.
    pub async fn delete_key(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }
}

/// Validate that the public key starts with a recognized prefix.
/// Returns the key type (ed25519, rsa, ecdsa).
fn validate_public_key(public_key: &str) -> AppResult<String> {
    for prefix in VALID_KEY_PREFIXES {
        if public_key.starts_with(prefix) {
            let key_type = if prefix.starts_with("ecdsa") {
                "ecdsa"
            } else if *prefix == "ssh-rsa" {
                "rsa"
            } else {
                "ed25519"
            };
            return Ok(key_type.to_string());
        }
    }
    Err(ErrorKind::Validation(format!("unsupported SSH key type, expected one of: {:?}", VALID_KEY_PREFIXES)).into())
}

/// Compute a SHA-256 fingerprint of the public key's binary data.
///
/// Splits the key on whitespace, takes the second field (base64-encoded key data),
/// decodes it, and hashes the raw bytes. Falls back to hashing the whole string
/// if base64 decoding fails.
fn compute_fingerprint(public_key: &str) -> String {
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;

    let bytes_to_hash = match public_key.split_whitespace().nth(1) {
        Some(b64_data) => match engine.decode(b64_data) {
            Ok(decoded) => decoded,
            Err(err) => {
                tracing::warn!(error = %err, "Failed to base64-decode SSH key data, hashing raw string");
                public_key.as_bytes().to_vec()
            }
        },
        None => {
            tracing::warn!("SSH key missing base64 data field, hashing raw string");
            public_key.as_bytes().to_vec()
        }
    };

    let mut hasher = Sha256::new();
    hasher.update(&bytes_to_hash);
    let digest = hasher.finalize();
    format!("SHA256:{}", engine.encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_ed25519_key() {
        let result = validate_public_key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 user@host");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "ed25519");
    }

    #[test]
    fn validate_rsa_key() {
        let result = validate_public_key("ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQ user@host");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "rsa");
    }

    #[test]
    fn validate_ecdsa_key() {
        let result = validate_public_key("ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHA user@host");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "ecdsa");
    }

    #[test]
    fn validate_invalid_key_type() {
        let result = validate_public_key("ssh-dss AAAAB3NzaC1kc3M user@host");
        assert!(result.is_err());
    }

    #[test]
    fn validate_empty_key() {
        let result = validate_public_key("");
        assert!(result.is_err());
    }

    #[test]
    fn fingerprint_is_consistent() {
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 user@host";
        let fp1 = compute_fingerprint(key);
        let fp2 = compute_fingerprint(key);
        assert_eq!(fp1, fp2);
        assert!(fp1.starts_with("SHA256:"));
    }

    #[test]
    fn different_keys_different_fingerprints() {
        let fp1 = compute_fingerprint("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA user1@host");
        let fp2 = compute_fingerprint("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5BBBB user2@host");
        assert_ne!(fp1, fp2);
    }
}
