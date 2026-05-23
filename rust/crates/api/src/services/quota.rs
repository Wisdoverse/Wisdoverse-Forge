//! Quota service — usage tracking and limits.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::QuotaUsage;
use sqlx::PgPool;

use crate::domain::configuration::QuotaResourceType;
pub(crate) use crate::domain::configuration::configuration_data_response;
use crate::repositories::quota::QuotaRepository;

/// Business logic layer for quota operations.
pub struct QuotaService {
    repo: QuotaRepository,
}

impl QuotaService {
    pub fn new(repo: QuotaRepository) -> Self {
        Self { repo }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(QuotaRepository::new(pool))
    }

    /// Get all quota usage for the org.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<QuotaUsage>> {
        self.repo.list(scope).await
    }

    /// Get quota usage for a specific resource type.
    pub async fn get_by_type(&self, scope: &TenantScope, resource_type: &str) -> AppResult<QuotaUsage> {
        let resource_type = QuotaResourceType::parse(resource_type)?;
        self.repo.get_by_type(scope, resource_type.value()).await
    }
}
