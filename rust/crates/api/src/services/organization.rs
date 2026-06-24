//! Organization service — business logic and validation.

use agentforge_core::{AppResult, OrgId, TenantScope};
use agentforge_db::entities::Organization;
use sqlx::PgPool;

pub(crate) use crate::domain::resource::resource_data_response;
use crate::domain::resource::{OrganizationSlug, ResourceName};
use crate::repositories::identity::organization::OrganizationRepository;

#[derive(Debug, Clone)]
pub struct CreateOrganizationInput {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone)]
pub struct UpdateOrganizationInput {
    pub name: String,
}

/// Business logic layer for organization operations.
pub struct OrganizationService {
    repo: OrganizationRepository,
}

impl OrganizationService {
    pub fn new(repo: OrganizationRepository) -> Self {
        Self { repo }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(OrganizationRepository::new(pool))
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
    pub async fn create(&self, scope: &TenantScope, input: CreateOrganizationInput) -> AppResult<Organization> {
        let name = ResourceName::parse(&input.name)?;
        let slug = OrganizationSlug::parse(&input.slug)?;
        self.repo.create(scope.user_id(), name.value(), slug.value()).await
    }

    /// Update an organization's name.
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: OrgId,
        input: UpdateOrganizationInput,
    ) -> AppResult<Organization> {
        let name = ResourceName::parse(&input.name)?;
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
