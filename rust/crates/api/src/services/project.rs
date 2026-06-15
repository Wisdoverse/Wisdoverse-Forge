//! Project service — business logic and validation.

use agentforge_core::{AppResult, ProjectId, TeamId, TenantScope, WorkspaceId};
use agentforge_db::entities::{Project, ProjectCloneAttempt};
use sqlx::PgPool;

pub(crate) use crate::domain::project_clone::CloneSummary;
use crate::domain::project_clone::{CloneApiPolicy, CloneAttemptStatus};
use crate::domain::resource::{ProjectRepositoryUrl, ResourceListPage, ResourceName};
pub(crate) use crate::domain::resource::{resource_data_response, resource_delete_response};
use crate::repositories::project::{CloneRequest, ProjectCreateTx, ProjectRepository};
use crate::repositories::project_clone::ProjectCloneRepository;
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

/// A project plus its latest clone-attempt summary, the projection the API
/// surfaces (M6). `clone` is `None` when the project has no attempt yet
/// (`clone_status='none'`); the flat `Project` already carries the denormalized
/// `clone_status` column, so the summary is purely additive detail.
///
/// `#[serde(flatten)]` keeps the existing flat `Project` JSON shape intact (all
/// of `clone_status`, `repository_url`, `workspace_dir_name`, …) and ADDS a
/// `clone` key — so the M7 frontend reads `project.cloneStatus` plus
/// `project.clone.{status,resolvedBranch,headSha,errorClass,errorMessage,…}`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProjectWithClone {
    #[serde(flatten)]
    pub project: Project,
    pub clone: Option<CloneSummary>,
}

impl ProjectWithClone {
    fn new(project: Project, clone: Option<CloneSummary>) -> Self {
        Self { project, clone }
    }
}

/// Business logic layer for project operations.
pub struct ProjectService {
    repo: ProjectRepository,
    clones: ProjectCloneRepository,
    permissions: ResourcePermissionService,
}

impl ProjectService {
    pub fn new(
        repo: ProjectRepository,
        clones: ProjectCloneRepository,
        permission_repo: ResourcePermissionRepository,
    ) -> Self {
        Self { repo, clones, permissions: ResourcePermissionService::new(permission_repo) }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(
            ProjectRepository::new(pool.clone()),
            ProjectCloneRepository::new(pool.clone()),
            ResourcePermissionRepository::new(pool),
        )
    }

    /// List projects with pagination and optional workspace filter, each with its
    /// latest clone-attempt summary attached (one batched query, no N+1). Limit is
    /// capped at 100.
    pub async fn list(
        &self,
        scope: &TenantScope,
        workspace_id: Option<WorkspaceId>,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<ProjectWithClone>> {
        let page = ResourceListPage::new(limit, offset);
        let projects = self.repo.list(scope, workspace_id, page.limit(), page.offset()).await?;
        let ids: Vec<uuid::Uuid> = projects.iter().map(|p| p.id.as_uuid()).collect();
        let mut summaries = self.clones.latest_attempt_summaries_for_projects(scope, &ids).await?;
        Ok(projects
            .into_iter()
            .map(|project| {
                let clone = summaries.remove(&project.id.as_uuid()).map(|row| CloneSummary::from_attempt(&row));
                ProjectWithClone::new(project, clone)
            })
            .collect())
    }

    /// Get a single project by ID, with its latest clone-attempt summary attached.
    pub async fn get(&self, scope: &TenantScope, id: ProjectId) -> AppResult<ProjectWithClone> {
        let project = self.repo.find_by_id(scope, id).await?;
        let clone = self.latest_clone_summary(scope, id).await?;
        Ok(ProjectWithClone::new(project, clone))
    }

    /// Create a new project with validated fields, transactionally.
    ///
    /// Permission + workspace-ownership are validated, then the project row,
    /// its default group, and (when a repository URL is present) the first clone
    /// attempt + transactional-outbox row are written in ONE transaction
    /// (`ProjectRepository::create_with_clone`), so there is never a project
    /// without its clone job, nor a clone job without a committed project.
    /// `team_id` defaults to the org's oldest team when absent. The response
    /// carries the freshly-created clone summary (the queued attempt) so the UI
    /// shows clone status immediately.
    pub async fn create(&self, scope: &TenantScope, input: CreateProjectInput) -> AppResult<ProjectWithClone> {
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

        let project = self
            .repo
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
            .await?;
        let summary = self.latest_clone_summary(scope, project.id).await?;
        Ok(ProjectWithClone::new(project, summary))
    }

    /// Update a project, enforcing the §9 repository-URL immutability rule.
    ///
    /// Metadata edits (name) are always allowed and never touch the clone. A
    /// change to `repository_url` is REJECTED once the project has any clone
    /// attempt (the one-shot bind cannot be re-pointed by the server) — only a
    /// pre-clone project (`clone_status='none'`, no attempt) may set/change it.
    /// "No-op" repo-URL writes (the same value, or clearing an already-absent
    /// URL) are allowed so a metadata PATCH that happens to echo the current URL
    /// does not spuriously fail. A repo-URL change never enqueues a new clone:
    /// the only enqueue path is create.
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: ProjectId,
        input: UpdateProjectInput,
    ) -> AppResult<ProjectWithClone> {
        self.permissions.require_project_manager(scope, id).await?;
        let name = input.name.as_deref().map(ResourceName::parse).transpose()?.map(ResourceName::value);

        // Repo-URL immutability gate (§9). Resolve the request's intended URL
        // value (validated, if present) and compare it to the project's current
        // value. Only a GENUINE change is gated; an unchanged value is a no-op.
        let repository_url = match input.repository_url.as_ref().map(|opt| opt.as_deref()) {
            Some(Some(url)) => {
                ProjectRepositoryUrl::parse(url)?;
                Some(Some(url))
            }
            other => other,
        };
        if let Some(desired) = repository_url {
            let current = self.repo.find_by_id(scope, id).await?;
            let changed = current.repository_url.as_deref() != desired;
            if changed && self.clones.latest_attempt_summary(scope, id).await?.is_some() {
                return Err(CloneApiPolicy::repository_url_immutable());
            }
        }

        let project = self.repo.update(scope, id, name, repository_url).await?;
        let summary = self.latest_clone_summary(scope, id).await?;
        Ok(ProjectWithClone::new(project, summary))
    }

    /// Soft-delete a project.
    pub async fn delete(&self, scope: &TenantScope, id: ProjectId) -> AppResult<()> {
        self.permissions.require_project_manager(scope, id).await?;
        self.repo.delete(scope, id).await
    }

    /// Retry a FAILED clone: create a fresh attempt (`attempt+1`, `queued`) + the
    /// transactional-outbox row the publisher relays, returning the new attempt's
    /// summary. Owner/manager only (`require_project_manager`).
    ///
    /// Guards, in order:
    ///   * the project must exist (tenant-scoped `find_by_id`);
    ///   * the project must have a repository URL (`400` otherwise — nothing to
    ///     clone);
    ///   * a prior attempt must exist (`409` otherwise — nothing to retry);
    ///   * the LATEST attempt must be `failed` (`409` otherwise — an in-flight or
    ///     already-`ready` clone must not be re-queued, and a `cancelled` one is
    ///     not a retry candidate).
    ///
    /// The new attempt is scheduled through `ProjectCloneRepository::schedule_retry`,
    /// which atomically inserts the next attempt row + a deduped outbox row and
    /// mirrors `projects.clone_status='queued'` — reusing the SAME unique-key /
    /// dedup invariants the M5 worker's automatic retry relies on, so a retry that
    /// races the reconciler (or a double-click) cannot pile up two attempts. A
    /// dedup no-op surfaces as `409` (a retry is already in flight). The retry
    /// enqueues immediately (no backoff): a deliberate operator action is not a
    /// fast-fail storm, and the dedup guards bound abuse.
    pub async fn retry_clone(&self, scope: &TenantScope, id: ProjectId) -> AppResult<CloneSummary> {
        self.permissions.require_project_manager(scope, id).await?;
        // Tenant-scoped existence check (foreign-org / deleted -> NotFound).
        let project = self.repo.find_by_id(scope, id).await?;
        if project.repository_url.is_none() {
            return Err(CloneApiPolicy::no_repository_to_clone());
        }

        let Some(latest) = self.clones.latest_attempt_summary(scope, id).await? else {
            return Err(CloneApiPolicy::no_attempt_to_retry());
        };
        if latest.status.as_str() != CloneAttemptStatus::Failed.as_str() {
            return Err(CloneApiPolicy::retry_only_from_failed(&latest.status));
        }

        let next_attempt = latest.attempt + 1;
        let scheduled = self
            .clones
            .schedule_retry(
                latest.organization_id.as_uuid(),
                latest.workspace_id.as_uuid(),
                latest.project_id.as_uuid(),
                next_attempt,
                &latest.repository_url,
                latest.provider.as_deref(),
                None, // operator-initiated retry: enqueue immediately, no backoff
            )
            .await?;
        // `schedule_retry` returns `None` when the next attempt already existed
        // (a racing retry/reconciler re-enqueue) — surface that as a conflict
        // rather than silently returning the stale failed summary.
        if scheduled.is_none() {
            return Err(CloneApiPolicy::retry_already_in_flight());
        }

        // Re-read the new latest attempt to return the queued summary.
        let new_latest =
            self.clones.latest_attempt_summary(scope, id).await?.ok_or_else(CloneApiPolicy::no_attempt_to_retry)?;
        Ok(CloneSummary::from_attempt(&new_latest))
    }

    /// The latest clone-attempt summary for a project, or `None` when it has no
    /// attempt yet. Shared by `get`/`create`/`update` to attach the projection.
    async fn latest_clone_summary(&self, scope: &TenantScope, id: ProjectId) -> AppResult<Option<CloneSummary>> {
        Ok(self.clones.latest_attempt_summary(scope, id).await?.map(|row| CloneSummary::from_attempt(&row)))
    }

    /// Project a single clone attempt into the API/UI summary. Re-exported for the
    /// legacy-navigation service so the active settings/sidebar surface attaches
    /// the SAME `CloneSummary` shape (M6/M7 contract).
    pub(crate) fn clone_summary_of(attempt: &ProjectCloneAttempt) -> CloneSummary {
        CloneSummary::from_attempt(attempt)
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
