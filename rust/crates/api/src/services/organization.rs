//! Organization service — business logic and validation.

use agentforge_core::{AppResult, ErrorKind, OrgId, TenantScope};
use agentforge_db::entities::Organization;

use crate::repositories::organization::OrganizationRepository;

/// Business logic layer for organization operations.
pub struct OrganizationService {
    repo: OrganizationRepository,
}

impl OrganizationService {
    pub fn new(repo: OrganizationRepository) -> Self {
        Self { repo }
    }

    /// List organizations the user belongs to.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<Organization>> {
        self.repo.list(scope).await
    }

    /// Get a single organization by ID.
    pub async fn get(&self, scope: &TenantScope, id: OrgId) -> AppResult<Organization> {
        self.repo.find_by_id(scope, id).await
    }

    /// Create a new organization with validated name and slug.
    pub async fn create(&self, scope: &TenantScope, name: &str, slug: &str) -> AppResult<Organization> {
        Self::validate_name(name)?;
        Self::validate_slug(slug)?;
        self.repo.create(scope.user_id(), name, slug).await
    }

    /// Update an organization's name.
    pub async fn update(&self, scope: &TenantScope, id: OrgId, name: &str) -> AppResult<Organization> {
        Self::validate_name(name)?;
        self.repo.update(scope, id, name).await
    }

    /// Validate organization name: 1-255 characters.
    fn validate_name(name: &str) -> AppResult<()> {
        if name.is_empty() || name.len() > 255 {
            return Err(ErrorKind::Validation("name must be between 1 and 255 characters".into()).into());
        }
        Ok(())
    }

    /// Validate slug: lowercase alphanumeric + hyphens, 3-50 characters.
    fn validate_slug(slug: &str) -> AppResult<()> {
        if slug.len() < 3 || slug.len() > 50 {
            return Err(ErrorKind::Validation("slug must be between 3 and 50 characters".into()).into());
        }
        if !slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(ErrorKind::Validation(
                "slug must contain only lowercase alphanumeric characters and hyphens".into(),
            )
            .into());
        }
        if slug.starts_with('-') || slug.ends_with('-') {
            return Err(ErrorKind::Validation("slug must not start or end with a hyphen".into()).into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_slugs() {
        assert!(OrganizationService::validate_slug("abc").is_ok());
        assert!(OrganizationService::validate_slug("my-org").is_ok());
        assert!(OrganizationService::validate_slug("org-123").is_ok());
        assert!(OrganizationService::validate_slug("a".repeat(50).as_str()).is_ok());
    }

    #[test]
    fn invalid_slugs() {
        // Too short
        assert!(OrganizationService::validate_slug("ab").is_err());
        // Too long
        assert!(OrganizationService::validate_slug(&"a".repeat(51)).is_err());
        // Uppercase
        assert!(OrganizationService::validate_slug("My-Org").is_err());
        // Spaces
        assert!(OrganizationService::validate_slug("my org").is_err());
        // Starts with hyphen
        assert!(OrganizationService::validate_slug("-my-org").is_err());
        // Ends with hyphen
        assert!(OrganizationService::validate_slug("my-org-").is_err());
        // Special characters
        assert!(OrganizationService::validate_slug("my_org").is_err());
        // Empty
        assert!(OrganizationService::validate_slug("").is_err());
    }

    #[test]
    fn valid_names() {
        assert!(OrganizationService::validate_name("A").is_ok());
        assert!(OrganizationService::validate_name("My Organization").is_ok());
        assert!(OrganizationService::validate_name(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn invalid_names() {
        // Empty
        assert!(OrganizationService::validate_name("").is_err());
        // Too long
        assert!(OrganizationService::validate_name(&"a".repeat(256)).is_err());
    }
}
