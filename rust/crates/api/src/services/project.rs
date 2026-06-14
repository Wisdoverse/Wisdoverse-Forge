//! Project service — business logic and validation.

use agentforge_core::{AppResult, ProjectId, TeamId, TenantScope, WorkspaceId};
use agentforge_db::entities::Project;
use sqlx::PgPool;

use crate::domain::resource::{ProjectRepositoryUrl, ResourceListPage, ResourceName};
pub(crate) use crate::domain::resource::{resource_data_response, resource_delete_response};
use crate::repositories::project::{CloneRequest, ProjectCreateTx, ProjectRepository};
use crate::repositories::resource::permission::ResourcePermissionRepository;
use crate::services::resource_permission::ResourcePermissionService;

#[derive(Debug, Clone)]
pub struct CreateProjectInput {
    pub workspace_id: WorkspaceId,
    pub team_id: Option<TeamId>,
    pub name: String,
    pub repository_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateProjectInput {
    pub name: Option<String>,
    pub repository_url: Option<Option<String>>,
}

/// Business logic layer for project operations.
pub struct ProjectService {
    repo: ProjectRepository,
    permissions: ResourcePermissionService,
}

impl ProjectService {
    pub fn new(repo: ProjectRepository, permission_repo: ResourcePermissionRepository) -> Self {
        Self { repo, permissions: ResourcePermissionService::new(permission_repo) }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(ProjectRepository::new(pool.clone()), ResourcePermissionRepository::new(pool))
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

    /// Create a new project with validated fields, transactionally.
    ///
    /// Permission + workspace-ownership are validated, then the project row,
    /// its default group, and (when a repository URL is present) the first clone
    /// attempt + transactional-outbox row are written in ONE transaction
    /// (`ProjectRepository::create_with_clone`), so there is never a project
    /// without its clone job, nor a clone job without a committed project.
    /// `team_id` defaults to the org's oldest team when absent.
    pub async fn create(&self, scope: &TenantScope, input: CreateProjectInput) -> AppResult<Project> {
        if let Some(team_id) = input.team_id {
            self.permissions.require_project_creator(scope, team_id).await?;
        } else {
            self.permissions.require_org_manager(scope).await?;
        }

        let name = ResourceName::parse(&input.name)?;
        let clone = match input.repository_url.as_deref() {
            Some(url) => Some(CloneRequest::parse(url)?),
            None => None,
        };
        // Resolve the parent team before the transaction so the repository takes
        // a concrete team id (the workspace-ownership + dir-allocation tx owns
        // only persistence). `require_project_creator`/`require_org_manager`
        // above already authorized the create.
        let team_id = match input.team_id {
            Some(team_id) => team_id.as_uuid(),
            None => self.repo.default_team_for_org(scope).await?,
        };

        self.repo
            .create_with_clone(
                scope,
                ProjectCreateTx {
                    workspace_id: input.workspace_id,
                    team_id,
                    name: name.value().to_string(),
                    color: None,
                    description: None,
                    clone,
                },
            )
            .await
    }

    /// Update a project.
    pub async fn update(&self, scope: &TenantScope, id: ProjectId, input: UpdateProjectInput) -> AppResult<Project> {
        self.permissions.require_project_manager(scope, id).await?;
        let name = input.name.as_deref().map(ResourceName::parse).transpose()?.map(ResourceName::value);
        let repository_url = input.repository_url.as_ref().map(|opt| opt.as_deref());
        if let Some(Some(url)) = repository_url {
            ProjectRepositoryUrl::parse(url)?;
        }
        self.repo.update(scope, id, name, repository_url).await
    }

    /// Soft-delete a project.
    pub async fn delete(&self, scope: &TenantScope, id: ProjectId) -> AppResult<()> {
        self.permissions.require_project_manager(scope, id).await?;
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
        // v1 clone is HTTPS-only (token auth); non-https schemes are rejected.
        assert!(ProjectRepositoryUrl::parse("https://github.com/org/repo").is_ok());
        assert!(ProjectRepositoryUrl::parse("https://gitlab.com/org/repo.git").is_ok());
    }

    #[test]
    fn invalid_urls() {
        assert!(ProjectRepositoryUrl::parse("").is_err());
        assert!(ProjectRepositoryUrl::parse("http://gitlab.com/org/repo").is_err());
        assert!(ProjectRepositoryUrl::parse("git@github.com:org/repo.git").is_err());
        assert!(ProjectRepositoryUrl::parse("ftp://example.com/repo").is_err());
        assert!(ProjectRepositoryUrl::parse("not-a-url").is_err());
        assert!(ProjectRepositoryUrl::parse("https://").is_err());
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
