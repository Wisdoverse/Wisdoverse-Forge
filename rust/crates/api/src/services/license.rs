//! License service — validation and activation.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::License;
use uuid::Uuid;

use crate::domain::license::{LicenseKey, LicenseValidityPolicy};
use crate::repositories::license::LicenseRepository;

/// Business logic layer for license operations.
pub struct LicenseService {
    repo: LicenseRepository,
}

impl LicenseService {
    pub fn new(repo: LicenseRepository) -> Self {
        Self { repo }
    }

    /// List licenses for the org.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<License>> {
        self.repo.list(scope).await
    }

    /// Get a license by ID.
    pub async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<License> {
        self.repo.get_by_id(scope, id).await
    }

    /// Validate a license key — check if it exists and is active.
    pub async fn validate(&self, license_key: &str) -> AppResult<serde_json::Value> {
        let license_key = LicenseKey::parse(license_key)?;
        match self.repo.find_by_key(license_key.value()).await? {
            Some(license) => {
                let valid = LicenseValidityPolicy::is_valid(license.is_active, license.valid_until, chrono::Utc::now());
                Ok(serde_json::json!({
                    "valid": valid,
                    "plan_name": license.plan_name,
                    "max_agents": license.max_agents,
                    "max_users": license.max_users,
                    "is_active": license.is_active,
                    "valid_until": license.valid_until,
                }))
            }
            None => Ok(serde_json::json!({ "valid": false, "reason": "unknown_key" })),
        }
    }

    /// Activate a license for the org.
    pub async fn activate(&self, scope: &TenantScope, license_key: &str) -> AppResult<License> {
        let license_key = LicenseKey::parse(license_key)?;
        self.repo.activate(scope, license_key.value()).await
    }
}
