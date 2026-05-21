//! Resource profile service — validation and management.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::ResourceProfile;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::configuration::ResourceProfilePolicy;
pub(crate) use crate::domain::resource::{resource_data_response, resource_delete_response};
use crate::repositories::resource::profile::ResourceProfileRepository;

#[derive(Debug, Clone)]
pub struct CreateResourceProfileInput {
    pub name: String,
    pub cpu_millicores: i32,
    pub memory_mb: i32,
    pub storage_mb: i32,
    pub max_pids: i32,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateResourceProfileInput {
    pub name: Option<String>,
    pub cpu_millicores: Option<i32>,
    pub memory_mb: Option<i32>,
    pub storage_mb: Option<i32>,
    pub max_pids: Option<i32>,
}

/// Business logic layer for resource profile operations.
pub struct ResourceProfileService {
    repo: ResourceProfileRepository,
}

impl ResourceProfileService {
    pub fn new(repo: ResourceProfileRepository) -> Self {
        Self { repo }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(ResourceProfileRepository::new(pool))
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
    pub async fn create(&self, scope: &TenantScope, input: CreateResourceProfileInput) -> AppResult<ResourceProfile> {
        ResourceProfilePolicy::validate_create(
            &input.name,
            input.cpu_millicores,
            input.memory_mb,
            input.storage_mb,
            input.max_pids,
        )?;
        self.repo
            .create(scope, &input.name, input.cpu_millicores, input.memory_mb, input.storage_mb, input.max_pids)
            .await
    }

    /// Update a resource profile.
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: Uuid,
        input: UpdateResourceProfileInput,
    ) -> AppResult<ResourceProfile> {
        ResourceProfilePolicy::validate_update(
            input.name.as_deref(),
            input.cpu_millicores,
            input.memory_mb,
            input.storage_mb,
            input.max_pids,
        )?;
        self.repo
            .update(
                scope,
                id,
                input.name.as_deref(),
                input.cpu_millicores,
                input.memory_mb,
                input.storage_mb,
                input.max_pids,
            )
            .await
    }

    /// Delete a resource profile.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }
}
