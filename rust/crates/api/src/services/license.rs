//! License service — validation and activation.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::License;
use uuid::Uuid;

pub(crate) use crate::domain::license::license_data_response;
use crate::domain::license::{LicenseKey, LicenseValidation, LicenseValidityPolicy};
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
    pub(crate) async fn validate(&self, license_key: &str) -> AppResult<LicenseValidation> {
        let license_key = LicenseKey::parse(license_key)?;
        match self.repo.find_by_key(license_key.value()).await? {
            Some(license) => {
                let valid = LicenseValidityPolicy::is_valid(license.is_active, license.valid_until, chrono::Utc::now());
                Ok(LicenseValidation::known(
                    valid,
                    license.plan_name,
                    license.max_agents,
                    license.max_users,
                    license.is_active,
                    license.valid_until,
                ))
            }
            None => Ok(LicenseValidation::unknown_key()),
        }
    }

    /// Activate a license for the org.
    pub async fn activate(&self, scope: &TenantScope, license_key: &str) -> AppResult<License> {
        let license_key = LicenseKey::parse(license_key)?;
        self.repo.activate(scope, license_key.value()).await
    }
}
