//! Context feature-gate service.

use agentforge_core::{AppResult, TenantScope};
use sqlx::PgPool;

use crate::domain::context::{ContextFeature, ContextFeatureFlags, ContextFeaturePolicy, ContextFeatureSnapshot};
use crate::repositories::feature_flag::FeatureFlagRepository;

pub struct ContextFeatureService {
    repo: FeatureFlagRepository,
    deployment: ContextFeatureFlags,
}

impl ContextFeatureService {
    pub fn new(pool: PgPool, deployment: ContextFeatureFlags) -> Self {
        Self { repo: FeatureFlagRepository::new(pool), deployment }
    }

    pub fn from_runtime(pool: PgPool, deployment: ContextFeatureFlags) -> Self {
        Self::new(pool, deployment)
    }

    pub(crate) async fn snapshot(&self, scope: &TenantScope) -> AppResult<ContextFeatureSnapshot> {
        Ok(ContextFeatureSnapshot {
            governance: self.is_enabled(scope, ContextFeature::Governance).await?,
            preview: self.is_enabled(scope, ContextFeature::Preview).await?,
            injection: self.is_enabled(scope, ContextFeature::Injection).await?,
            analytics: self.is_enabled(scope, ContextFeature::Analytics).await?,
        })
    }

    pub(crate) async fn ensure_governance_enabled(&self, scope: &TenantScope) -> AppResult<()> {
        self.ensure_enabled(scope, ContextFeature::Governance).await
    }

    pub(crate) async fn ensure_preview_enabled(&self, scope: &TenantScope) -> AppResult<()> {
        self.ensure_enabled(scope, ContextFeature::Preview).await
    }

    pub(crate) async fn ensure_injection_enabled(&self, scope: &TenantScope) -> AppResult<()> {
        self.ensure_enabled(scope, ContextFeature::Injection).await
    }

    pub(crate) async fn ensure_analytics_enabled(&self, scope: &TenantScope) -> AppResult<()> {
        self.ensure_enabled(scope, ContextFeature::Analytics).await
    }

    pub(crate) async fn ensure_enabled(&self, scope: &TenantScope, feature: ContextFeature) -> AppResult<()> {
        if self.is_enabled(scope, feature).await? {
            return Ok(());
        }

        Err(ContextFeaturePolicy::disabled(feature).into())
    }

    pub(crate) async fn is_enabled(&self, scope: &TenantScope, feature: ContextFeature) -> AppResult<bool> {
        let deployment_enabled = self.deployment.enabled(feature);
        if !deployment_enabled {
            return Ok(false);
        }

        match self.repo.find_by_name(scope.org_id(), feature.key()).await {
            Ok(flag) => Ok(flag.enabled),
            Err(err) if ContextFeaturePolicy::missing_override_allows_deployment_default(&err.kind) => {
                Ok(deployment_enabled)
            }
            Err(err) => Err(err),
        }
    }
}
