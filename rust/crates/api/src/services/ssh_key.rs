//! SSH key service — validation and management.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::SshKey;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::credential::{CredentialListPage, SshKeyName, SshKeyView, SshPublicKey};
pub(crate) use crate::domain::credential::{
    credential_delete_response, ssh_key_create_response, ssh_key_list_response,
};
use crate::repositories::credential::ssh_key::SshKeyRepository;

/// Business logic layer for SSH key operations.
pub struct SshKeyService {
    repo: SshKeyRepository,
}

impl SshKeyService {
    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(SshKeyRepository::new(pool))
    }

    pub fn new(repo: SshKeyRepository) -> Self {
        Self { repo }
    }

    /// Add a new SSH key after validating format and computing fingerprint.
    pub async fn add_key(&self, scope: &TenantScope, name: &str, public_key: &str) -> AppResult<SshKeyView> {
        let name = SshKeyName::parse(name)?;
        let public_key = SshPublicKey::parse(public_key)?;
        let fingerprint = public_key.fingerprint();

        let key =
            self.repo.create(scope, name.value(), public_key.value(), &fingerprint, public_key.kind().as_str()).await?;
        Ok(ssh_key_view(&key))
    }

    /// List SSH keys (paginated), projected to the non-secret view.
    pub async fn list_keys(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<SshKeyView>> {
        let page = CredentialListPage::new(limit, offset);
        let keys = self.repo.list(scope, page.limit(), page.offset()).await?;
        Ok(keys.iter().map(ssh_key_view).collect())
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

/// Project an `SshKey` row into the non-secret [`SshKeyView`] for responses.
/// SSH keys persist only the public half, so every field is carried through;
/// the view exists so the domain response builders never import the row.
fn ssh_key_view(key: &SshKey) -> SshKeyView {
    SshKeyView {
        id: key.id,
        organization_id: key.organization_id,
        user_id: key.user_id,
        name: key.name.clone(),
        public_key: key.public_key.clone(),
        fingerprint: key.fingerprint.clone(),
        key_type: key.key_type.clone(),
        created_at: key.created_at,
    }
}
