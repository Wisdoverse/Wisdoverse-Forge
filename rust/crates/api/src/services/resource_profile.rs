//! Resource profile service — validation and management.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::ResourceProfile;
use uuid::Uuid;

use crate::repositories::resource_profile::ResourceProfileRepository;

/// Maximum name length.
const MAX_NAME_LEN: usize = 100;

/// Business logic layer for resource profile operations.
pub struct ResourceProfileService {
    repo: ResourceProfileRepository,
}

impl ResourceProfileService {
    pub fn new(repo: ResourceProfileRepository) -> Self {
        Self { repo }
    }

    /// List resource profiles (org + system defaults).
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<ResourceProfile>> {
        self.repo.list(scope).await
    }

    /// Get a single resource profile.
    pub async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<ResourceProfile> {
        self.repo.get(scope, id).await
    }

    /// Create a custom resource profile.
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        cpu_millicores: i32,
        memory_mb: i32,
        storage_mb: i32,
        max_pids: i32,
    ) -> AppResult<ResourceProfile> {
        if name.is_empty() || name.len() > MAX_NAME_LEN {
            return Err(ErrorKind::Validation(format!("name must be 1-{MAX_NAME_LEN} characters")).into());
        }
        if cpu_millicores <= 0 {
            return Err(ErrorKind::Validation("cpu_millicores must be positive".into()).into());
        }
        if memory_mb <= 0 {
            return Err(ErrorKind::Validation("memory_mb must be positive".into()).into());
        }
        if storage_mb <= 0 {
            return Err(ErrorKind::Validation("storage_mb must be positive".into()).into());
        }
        if max_pids <= 0 {
            return Err(ErrorKind::Validation("max_pids must be positive".into()).into());
        }
        self.repo.create(scope, name, cpu_millicores, memory_mb, storage_mb, max_pids).await
    }

    /// Update a resource profile.
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: Uuid,
        name: Option<&str>,
        cpu_millicores: Option<i32>,
        memory_mb: Option<i32>,
        storage_mb: Option<i32>,
        max_pids: Option<i32>,
    ) -> AppResult<ResourceProfile> {
        if let Some(n) = name
            && (n.is_empty() || n.len() > MAX_NAME_LEN)
        {
            return Err(ErrorKind::Validation(format!("name must be 1-{MAX_NAME_LEN} characters")).into());
        }
        if let Some(v) = cpu_millicores
            && v <= 0
        {
            return Err(ErrorKind::Validation("cpu_millicores must be positive".into()).into());
        }
        if let Some(v) = memory_mb
            && v <= 0
        {
            return Err(ErrorKind::Validation("memory_mb must be positive".into()).into());
        }
        if let Some(v) = storage_mb
            && v <= 0
        {
            return Err(ErrorKind::Validation("storage_mb must be positive".into()).into());
        }
        if let Some(v) = max_pids
            && v <= 0
        {
            return Err(ErrorKind::Validation("max_pids must be positive".into()).into());
        }
        self.repo.update(scope, id, name, cpu_millicores, memory_mb, storage_mb, max_pids).await
    }

    /// Delete a resource profile.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_length_limit() {
        assert_eq!(MAX_NAME_LEN, 100);
    }
}
