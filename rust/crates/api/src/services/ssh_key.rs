//! SSH key service — validation and management.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::SshKey;
use uuid::Uuid;

use crate::domain::credential::{CredentialListPage, SshKeyName, SshPublicKey};
pub(crate) use crate::domain::credential::{ssh_key_create_response, ssh_key_list_response};
use crate::repositories::credential::ssh_key::SshKeyRepository;

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
        let name = SshKeyName::parse(name)?;
        let public_key = SshPublicKey::parse(public_key)?;
        let fingerprint = public_key.fingerprint();

        self.repo.create(scope, name.value(), public_key.value(), &fingerprint, public_key.kind().as_str()).await
    }

    /// List SSH keys (paginated).
    pub async fn list_keys(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<SshKey>> {
        let page = CredentialListPage::new(limit, offset);
        self.repo.list(scope, page.limit(), page.offset()).await
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
