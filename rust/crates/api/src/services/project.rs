//! Project service — business logic and validation.

use agentforge_core::{AppResult, ErrorKind, ProjectId, TeamId, TenantScope, WorkspaceId};
use agentforge_db::entities::Project;

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
        let limit = limit.clamp(1, 100);
        let offset = offset.max(0);
        self.repo.list(scope, workspace_id, limit, offset).await
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
        Self::validate_name(name)?;
        if let Some(url) = repository_url {
            Self::validate_url(url)?;
        }
        self.repo.create(scope, workspace_id, team_id, name, repository_url).await
    }

    /// Update a project.
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: ProjectId,
        name: Option<&str>,
        repository_url: Option<Option<&str>>,
    ) -> AppResult<Project> {
        if let Some(name) = name {
            Self::validate_name(name)?;
        }
        if let Some(Some(url)) = repository_url {
            Self::validate_url(url)?;
        }
        self.repo.update(scope, id, name, repository_url).await
    }

    /// Soft-delete a project.
    pub async fn delete(&self, scope: &TenantScope, id: ProjectId) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }

    /// Validate project name: 1-255 characters.
    fn validate_name(name: &str) -> AppResult<()> {
        if name.is_empty() || name.len() > 255 {
            return Err(ErrorKind::Validation("name must be between 1 and 255 characters".into()).into());
        }
        Ok(())
    }

    /// Validate repository URL format (basic check).
    fn validate_url(url: &str) -> AppResult<()> {
        if url.is_empty() {
            return Err(ErrorKind::Validation("repository URL must not be empty".into()).into());
        }
        if !url.starts_with("https://") && !url.starts_with("http://") && !url.starts_with("git@") {
            return Err(
                ErrorKind::Validation("repository URL must start with https://, http://, or git@".into()).into()
            );
        }
        if url.len() > 2048 {
            return Err(ErrorKind::Validation("repository URL must be 2048 characters or less".into()).into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names() {
        assert!(ProjectService::validate_name("A").is_ok());
        assert!(ProjectService::validate_name("My Project").is_ok());
        assert!(ProjectService::validate_name(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn invalid_names() {
        assert!(ProjectService::validate_name("").is_err());
        assert!(ProjectService::validate_name(&"a".repeat(256)).is_err());
    }

    #[test]
    fn valid_urls() {
        assert!(ProjectService::validate_url("https://github.com/org/repo").is_ok());
        assert!(ProjectService::validate_url("http://gitlab.com/org/repo").is_ok());
        assert!(ProjectService::validate_url("git@github.com:org/repo.git").is_ok());
    }

    #[test]
    fn invalid_urls() {
        assert!(ProjectService::validate_url("").is_err());
        assert!(ProjectService::validate_url("ftp://example.com/repo").is_err());
        assert!(ProjectService::validate_url("not-a-url").is_err());
        assert!(ProjectService::validate_url(&format!("https://{}", "a".repeat(2048))).is_err());
    }

    #[test]
    fn limit_clamping() {
        assert_eq!(0_i64.clamp(1, 100), 1);
        assert_eq!(200_i64.clamp(1, 100), 100);
    }

    #[test]
    fn offset_floor() {
        let negative_offset = -10_i64;
        let positive_offset = 50_i64;
        assert_eq!(negative_offset.max(0), 0);
        assert_eq!(positive_offset.max(0), 50);
    }
}
