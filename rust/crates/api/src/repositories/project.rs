//! Project repository — tenant-scoped database queries for projects.

use agentforge_core::{AppError, AppResult, ProjectId, TenantScope, WorkspaceId};
use agentforge_db::entities::Project;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::project_clone::{
    CLONE_OUTBOX_AGGREGATE_TYPE, CLONE_OUTBOX_EVENT_TYPE, CloneAttemptStatus, CloneOutboxPayload, CloneProvider,
    CloneStatus, WorkspaceDirName,
};
use crate::domain::resource::{ProjectRepositoryUrl, ResourceRepositoryPolicy};

/// A validated, host-resolved git repository to clone into the new project.
///
/// Built in the service layer from a raw URL via [`ProjectRepositoryUrl::parse`]
/// so the repository never re-parses an untrusted string. `provider` is the
/// host-derived [`CloneProvider`] (NULL in the DB when the host matches no known
/// provider — the attempt still proceeds; M6 decides credential availability).
#[derive(Debug, Clone)]
pub struct CloneRequest {
    /// The exact URL string to persist on the project + attempt snapshot.
    pub url: String,
    /// The provider resolved from the URL host, if it is a known SaaS host.
    pub provider: Option<CloneProvider>,
}

impl CloneRequest {
    /// Parse + host-resolve a raw repository URL into a clone request. Returns a
    /// `Validation` error if the URL fails [`ProjectRepositoryUrl::parse`].
    pub fn parse(url: &str) -> AppResult<Self> {
        let parsed = ProjectRepositoryUrl::parse(url)?;
        let provider = CloneProvider::from_host(parsed.host());
        Ok(Self { url: url.to_string(), provider })
    }
}

/// The fully-resolved input for one transactional project create.
///
/// The service resolves `team_id` (and validates permissions) before building
/// this; the repository owns only the single-transaction persistence: workspace
/// ownership, dir-name allocation, project + default-group + clone-attempt +
/// outbox inserts. Every field is already validated.
#[derive(Debug, Clone)]
pub struct ProjectCreateTx {
    pub workspace_id: WorkspaceId,
    pub team_id: Uuid,
    pub name: String,
    /// Optional presentation color (legacy-navigation surface). `None` keeps the
    /// column default.
    pub color: Option<String>,
    /// Optional description (legacy-navigation surface). `None` keeps the column
    /// default.
    pub description: Option<String>,
    /// Present only when the project is created with a git repository to clone.
    pub clone: Option<CloneRequest>,
}

/// Maximum number of `workspace_dir_name` suffix attempts before giving up.
///
/// Each pass appends a fresh short suffix; a collision past this bound means the
/// workspace genuinely has an extraordinary number of same-named projects and
/// the create is refused rather than looping forever.
const MAX_DIR_NAME_ATTEMPTS: usize = 16;

/// Database access layer for projects.
pub struct ProjectRepository {
    pool: PgPool,
}

impl ProjectRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List projects for the current tenant, with optional workspace filter.
    pub async fn list(
        &self,
        scope: &TenantScope,
        workspace_id: Option<WorkspaceId>,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<Project>> {
        let projects = match workspace_id {
            Some(ws_id) => {
                sqlx::query_as::<_, Project>(
                    r#"SELECT * FROM projects
                       WHERE organization_id = $1 AND workspace_id = $2 AND deleted_at IS NULL
                       ORDER BY created_at DESC
                       LIMIT $3 OFFSET $4"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(ws_id.as_uuid())
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, Project>(
                    r#"SELECT * FROM projects
                       WHERE organization_id = $1 AND deleted_at IS NULL
                       ORDER BY created_at DESC
                       LIMIT $2 OFFSET $3"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(projects)
    }

    /// Get a single project by ID (tenant-scoped).
    pub async fn find_by_id(&self, scope: &TenantScope, id: ProjectId) -> AppResult<Project> {
        sqlx::query_as::<_, Project>(
            "SELECT * FROM projects WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL",
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::project_not_found(id))
    }

    /// Create a project (and, when a repo URL is present, its first clone
    /// attempt + outbox row) in a SINGLE transaction.
    ///
    /// This is the one transactional create path shared by both create surfaces
    /// (the flat `ProjectService` and the legacy-navigation settings/sidebar
    /// path). Within one `sqlx` transaction it:
    ///
    ///   1. validates the target workspace belongs to `scope.org_id`
    ///      (foreign-org workspace -> `NotFound`);
    ///   2. allocates a filesystem-safe, per-workspace-unique `workspace_dir_name`
    ///      from the project name, retrying with a numeric suffix on the
    ///      `(workspace_id, workspace_dir_name)` unique index so two concurrent
    ///      same-name creates cannot both win;
    ///   3. inserts the project row with `clone_status` = `queued` (repo present)
    ///      or `none`;
    ///   4. creates the project's default group;
    ///   5. when a repo is present, inserts the attempt-1 `project_clone_attempts`
    ///      row (`status='queued'`, URL + provider snapshot, tenant cols) AND a
    ///      `project_clone` transactional-outbox row (`{project_id, attempt:1}`).
    ///
    /// Any failure rolls the whole tuple back — there is never a project without
    /// its attempt/outbox, nor an attempt/outbox without a committed project.
    pub async fn create_with_clone(&self, scope: &TenantScope, input: ProjectCreateTx) -> AppResult<Project> {
        let mut tx = self.pool.begin().await?;
        let project = Self::create_with_clone_in_tx(&mut tx, scope, input).await?;
        tx.commit().await?;
        Ok(project)
    }

    /// The transactional body of [`create_with_clone`](Self::create_with_clone),
    /// exposed for callers that already own a transaction (and for tests that
    /// assert rollback). Performs no commit — the caller owns the transaction
    /// boundary.
    pub async fn create_with_clone_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        input: ProjectCreateTx,
    ) -> AppResult<Project> {
        let org_id = scope.org_id().as_uuid();
        let workspace_id = input.workspace_id.as_uuid();

        // (1) Workspace must belong to the caller's organization. A foreign-org
        // (or non-existent / soft-deleted) workspace is indistinguishable from
        // "not found" to this tenant — never confirm another org's resource.
        let workspace_ok = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1 FROM public.workspaces
                    WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL
               )"#,
        )
        .bind(workspace_id)
        .bind(org_id)
        .fetch_one(&mut **tx)
        .await?;
        if !workspace_ok {
            return Err(ResourceRepositoryPolicy::workspace_not_found(input.workspace_id));
        }

        // (2) + (3): allocate a unique dir name and insert the project, retrying
        // on the per-workspace unique index. A SAVEPOINT isolates each insert so
        // a unique-violation does not poison the outer transaction; on collision
        // we roll back to the savepoint and retry with the next suffix.
        let base_dir = WorkspaceDirName::derive(&input.name);
        let clone_status =
            if input.clone.is_some() { CloneStatus::Queued.as_str() } else { CloneStatus::None.as_str() };

        let mut project: Option<Project> = None;
        for attempt in 0..MAX_DIR_NAME_ATTEMPTS {
            // attempt 0 = the bare derived name; 1.. append `-2`, `-3`, …
            let dir = if attempt == 0 { base_dir.clone() } else { base_dir.with_suffix((attempt as u32) + 1) };

            sqlx::query("SAVEPOINT project_dir_alloc").execute(&mut **tx).await?;
            let inserted = sqlx::query_as::<_, Project>(
                r#"INSERT INTO projects
                       (organization_id, workspace_id, team_id, name, slug, workspace_dir_name,
                        color, description, repository_url, clone_status)
                   VALUES ($1, $2, $3, $4, $5, $6,
                           COALESCE($7::text, '#007AFF'), COALESCE($8::text, ''), $9, $10)
                   RETURNING *"#,
            )
            .bind(org_id)
            .bind(workspace_id)
            .bind(input.team_id)
            .bind(&input.name)
            // `slug` keeps the legacy `(team_id, slug)` contract; the on-disk
            // identity is `workspace_dir_name`. Both derive from the name.
            .bind(dir.as_str())
            .bind(dir.as_str())
            .bind(input.color.as_deref())
            .bind(input.description.as_deref())
            .bind(input.clone.as_ref().map(|c| c.url.as_str()))
            .bind(clone_status)
            .fetch_one(&mut **tx)
            .await;

            match inserted {
                Ok(row) => {
                    sqlx::query("RELEASE SAVEPOINT project_dir_alloc").execute(&mut **tx).await?;
                    project = Some(row);
                    break;
                }
                Err(err) if is_unique_violation(&err) => {
                    sqlx::query("ROLLBACK TO SAVEPOINT project_dir_alloc").execute(&mut **tx).await?;
                    continue;
                }
                Err(err) => return Err(err.into()),
            }
        }
        let project = project.ok_or_else(ResourceRepositoryPolicy::workspace_dir_allocation_exhausted)?;
        let project_id = project.id.as_uuid();

        // (4) Default project group, in the SAME transaction so it rolls back
        // with the project. Mirrors `GroupRepository::create` for a project group.
        sqlx::query(
            r#"INSERT INTO groups (organization_id, project_id, name, description, created_by)
               VALUES ($1, $2, 'Tasks', 'Default task group for this project.', $3)"#,
        )
        .bind(org_id)
        .bind(project_id)
        .bind(scope.user_id().as_uuid())
        .execute(&mut **tx)
        .await?;

        // (5) Clone attempt + transactional outbox row, only when a repo is set.
        if let Some(clone) = input.clone.as_ref() {
            sqlx::query(
                r#"INSERT INTO project_clone_attempts
                       (organization_id, workspace_id, project_id, attempt, repository_url, provider, status)
                   VALUES ($1, $2, $3, 1, $4, $5, $6)"#,
            )
            .bind(org_id)
            .bind(workspace_id)
            .bind(project_id)
            .bind(&clone.url)
            .bind(clone.provider.map(|p| p.as_str()))
            .bind(CloneAttemptStatus::Queued.as_str())
            .execute(&mut **tx)
            .await?;

            let payload = CloneOutboxPayload { project_id, attempt: 1 };
            let payload_json = serde_json::to_value(&payload).map_err(|e| AppError::from(anyhow::Error::from(e)))?;
            sqlx::query(
                r#"INSERT INTO orchestration_outbox
                       (id, organization_id, aggregate_type, aggregate_id, event_type, payload)
                   VALUES (gen_random_uuid(), $1, $2, $3, $4, $5)"#,
            )
            .bind(org_id)
            .bind(CLONE_OUTBOX_AGGREGATE_TYPE)
            .bind(project_id)
            .bind(CLONE_OUTBOX_EVENT_TYPE)
            .bind(payload_json)
            .execute(&mut **tx)
            .await?;
        }

        Ok(project)
    }

    /// Returns the org's oldest surviving team id, or a `Validation`
    /// error if the org has no teams. Used as the default parent when
    /// `create` is called without an explicit `team_id`.
    pub async fn default_team_for_org(&self, scope: &TenantScope) -> AppResult<uuid::Uuid> {
        let row: Option<(uuid::Uuid,)> = sqlx::query_as(
            r#"SELECT id FROM public.teams
               WHERE organization_id = $1 AND deleted_at IS NULL
               ORDER BY created_at ASC
               LIMIT 1"#,
        )
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| r.0).ok_or_else(ResourceRepositoryPolicy::default_project_team_required)
    }

    /// Returns true when the user can read `project_id` in the given
    /// `org_id`/`workspace_id` via direct project membership or membership of
    /// the project's owning team. Used by session-context authorization
    /// before a context switch has been minted (no tenant scope yet).
    pub async fn user_can_read(
        &self,
        project_id: Uuid,
        org_id: Uuid,
        workspace_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<bool> {
        let can_read = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1
                     FROM projects p
                    WHERE p.id = $1
                      AND p.organization_id = $2
                      AND p.workspace_id = $3
                      AND p.deleted_at IS NULL
                      AND (
                          EXISTS (
                              SELECT 1 FROM project_members pm
                               WHERE pm.project_id = p.id AND pm.user_id = $4
                          )
                          OR EXISTS (
                              SELECT 1 FROM team_members tm
                               WHERE tm.team_id = p.team_id AND tm.user_id = $4
                          )
                      )
               )"#,
        )
        .bind(project_id)
        .bind(org_id)
        .bind(workspace_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(can_read)
    }

    /// Update a project (tenant-scoped).
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: ProjectId,
        name: Option<&str>,
        repository_url: Option<Option<&str>>,
    ) -> AppResult<Project> {
        // Build update dynamically based on provided fields.
        // For simplicity, we fetch then update only changed fields.
        let existing = self.find_by_id(scope, id).await?;

        let new_name = name.unwrap_or(&existing.name);
        let new_url = match repository_url {
            Some(url) => url,
            None => existing.repository_url.as_deref(),
        };

        sqlx::query_as::<_, Project>(
            r#"UPDATE projects SET name = $3, repository_url = $4, updated_at = NOW()
               WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .bind(new_name)
        .bind(new_url)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ResourceRepositoryPolicy::project_not_found(id))
    }

    /// Soft-delete a project (set deleted_at).
    pub async fn delete(&self, scope: &TenantScope, id: ProjectId) -> AppResult<()> {
        let result = sqlx::query(
            r#"UPDATE projects SET deleted_at = NOW()
               WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL"#,
        )
        .bind(id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ResourceRepositoryPolicy::project_not_found(id));
        }
        Ok(())
    }
}

/// True when a `sqlx` error is a Postgres `unique_violation` (SQLSTATE 23505).
///
/// Used by the dir-name allocation loop to distinguish a recoverable
/// `(workspace_id, workspace_dir_name)` collision (retry with the next suffix)
/// from a genuine error (propagate).
fn is_unique_violation(err: &sqlx::Error) -> bool {
    err.as_database_error().and_then(|db| db.code()).as_deref() == Some("23505")
}
