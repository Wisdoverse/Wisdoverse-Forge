//! Organization service — business logic and validation.

use agentforge_core::{AppResult, OrgId, TenantScope};
use agentforge_db::entities::Organization;

use crate::domain::resource::{OrganizationSlug, ResourceName};
use crate::repositories::identity::organization::OrganizationRepository;

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
        let name = ResourceName::parse(name)?;
        let slug = OrganizationSlug::parse(slug)?;
        self.repo.create(scope.user_id(), name.value(), slug.value()).await
    }

    /// Update an organization's name.
    pub async fn update(&self, scope: &TenantScope, id: OrgId, name: &str) -> AppResult<Organization> {
        let name = ResourceName::parse(name)?;
        self.repo.update(scope, id, name.value()).await
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::resource::{OrganizationSlug, ResourceName};

    #[test]
    fn valid_slugs() {
        assert!(OrganizationSlug::parse("abc").is_ok());
        assert!(OrganizationSlug::parse("my-org").is_ok());
        assert!(OrganizationSlug::parse("org-123").is_ok());
        assert!(OrganizationSlug::parse("a".repeat(50).as_str()).is_ok());
    }

    #[test]
    fn invalid_slugs() {
        // Too short
        assert!(OrganizationSlug::parse("ab").is_err());
        // Too long
        assert!(OrganizationSlug::parse(&"a".repeat(51)).is_err());
        // Uppercase
        assert!(OrganizationSlug::parse("My-Org").is_err());
        // Spaces
        assert!(OrganizationSlug::parse("my org").is_err());
        // Starts with hyphen
        assert!(OrganizationSlug::parse("-my-org").is_err());
        // Ends with hyphen
        assert!(OrganizationSlug::parse("my-org-").is_err());
        // Special characters
        assert!(OrganizationSlug::parse("my_org").is_err());
        // Empty
        assert!(OrganizationSlug::parse("").is_err());
    }

    #[test]
    fn valid_names() {
        assert!(ResourceName::parse("A").is_ok());
        assert!(ResourceName::parse("My Organization").is_ok());
        assert!(ResourceName::parse(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn invalid_names() {
        // Empty
        assert!(ResourceName::parse("").is_err());
        // Too long
        assert!(ResourceName::parse(&"a".repeat(256)).is_err());
    }
}
