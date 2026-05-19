//! Resource profile service — validation and management.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::ResourceProfile;
use uuid::Uuid;

use crate::domain::configuration::ResourceProfilePolicy;
use crate::repositories::resource::profile::ResourceProfileRepository;

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
        ResourceProfilePolicy::validate_create(name, cpu_millicores, memory_mb, storage_mb, max_pids)?;
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
        ResourceProfilePolicy::validate_update(name, cpu_millicores, memory_mb, storage_mb, max_pids)?;
        self.repo.update(scope, id, name, cpu_millicores, memory_mb, storage_mb, max_pids).await
    }

    /// Delete a resource profile.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }
}
