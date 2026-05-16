//! Project service — business logic and validation.

use agentforge_core::{AppResult, ProjectId, TeamId, TenantScope, WorkspaceId};
use agentforge_db::entities::Project;

use crate::domain::resource::{ProjectRepositoryUrl, ResourceListPage, ResourceName};
use crate::repositories::project::ProjectRepository;

/// Business logic layer for project operations.
pub struct ProjectService {
    repo: ProjectRepository,
}

impl ProjectService {
    pub fn new(repo: ProjectRepository) -> Self {
        Self { repo }
    }

    /// List projects with pagination and optional workspace filter. Limit is capped at 100.
    pub async fn list(
        &self,
        scope: &TenantScope,
        workspace_id: Option<WorkspaceId>,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<Project>> {
        let page = ResourceListPage::new(limit, offset);
        self.repo.list(scope, workspace_id, page.limit(), page.offset()).await
    }

    /// Get a single project by ID.
    pub async fn get(&self, scope: &TenantScope, id: ProjectId) -> AppResult<Project> {
        self.repo.find_by_id(scope, id).await
    }

    /// Create a new project with validated fields. `team_id` defaults to
    /// the org's oldest team when absent — see `ProjectRepository::create`.
    pub async fn create(
        &self,
        scope: &TenantScope,
        workspace_id: WorkspaceId,
        team_id: Option<TeamId>,
        name: &str,
        repository_url: Option<&str>,
    ) -> AppResult<Project> {
        let name = ResourceName::parse(name)?;
        if let Some(url) = repository_url {
            ProjectRepositoryUrl::parse(url)?;
        }
        self.repo.create(scope, workspace_id, team_id, name.value(), repository_url).await
    }

    /// Update a project.
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: ProjectId,
        name: Option<&str>,
        repository_url: Option<Option<&str>>,
    ) -> AppResult<Project> {
        let name = name.map(ResourceName::parse).transpose()?.map(ResourceName::value);
        if let Some(Some(url)) = repository_url {
            ProjectRepositoryUrl::parse(url)?;
        }
        self.repo.update(scope, id, name, repository_url).await
    }

    /// Soft-delete a project.
    pub async fn delete(&self, scope: &TenantScope, id: ProjectId) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::resource::{ProjectRepositoryUrl, ResourceListPage, ResourceName};

    #[test]
    fn valid_names() {
        assert!(ResourceName::parse("A").is_ok());
        assert!(ResourceName::parse("My Project").is_ok());
        assert!(ResourceName::parse(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn invalid_names() {
        assert!(ResourceName::parse("").is_err());
        assert!(ResourceName::parse(&"a".repeat(256)).is_err());
    }

    #[test]
    fn valid_urls() {
        assert!(ProjectRepositoryUrl::parse("https://github.com/org/repo").is_ok());
        assert!(ProjectRepositoryUrl::parse("http://gitlab.com/org/repo").is_ok());
        assert!(ProjectRepositoryUrl::parse("git@github.com:org/repo.git").is_ok());
    }

    #[test]
    fn invalid_urls() {
        assert!(ProjectRepositoryUrl::parse("").is_err());
        assert!(ProjectRepositoryUrl::parse("ftp://example.com/repo").is_err());
        assert!(ProjectRepositoryUrl::parse("not-a-url").is_err());
        assert!(ProjectRepositoryUrl::parse(&format!("https://{}", "a".repeat(2048))).is_err());
    }

    #[test]
    fn limit_clamping() {
        assert_eq!(ResourceListPage::new(0, 0).limit(), 1);
        assert_eq!(ResourceListPage::new(200, 0).limit(), 100);
    }

    #[test]
    fn offset_floor() {
        assert_eq!(ResourceListPage::new(10, -10).offset(), 0);
        assert_eq!(ResourceListPage::new(10, 50).offset(), 50);
    }
}
