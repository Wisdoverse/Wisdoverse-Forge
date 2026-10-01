//! Organization-scoped maintenance sources and atomic source/task creation.

use agentforge_core::{AppResult, TenantScope};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use super::{CreateTaskRow, OrchestrationTaskRepository};
use crate::domain::admin::AdminRolePolicy;
use crate::domain::maintenance::{
    MaintenancePullRequest, MaintenanceSourceIdentity, PreparedMaintenanceRequest, destination_unavailable,
    maintenance_task_params,
};
use crate::domain::self_fix::SelfFixRepositorySetup;
use crate::repositories::user::UserRepository;

#[derive(Debug, FromRow)]
pub(crate) struct MaintenanceRequestRow {
    pub id: Uuid,
    pub task_id: Uuid,
    pub repository: String,
    pub source_kind: String,
    pub source_reference: String,
    pub source_pr_number: Option<i32>,
    pub source_head_sha: Option<String>,
    pub default_branch: String,
    pub starting_sha: String,
    pub created_at: DateTime<Utc>,
}

pub(crate) struct MaintenanceRequestRepository {
    pool: PgPool,
}

impl MaintenanceRequestRepository {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn find_by_source(
        &self,
        scope: &TenantScope,
        repository: &str,
        source: &MaintenanceSourceIdentity,
    ) -> AppResult<Option<MaintenanceRequestRow>> {
        Ok(sqlx::query_as::<_, MaintenanceRequestRow>(
            "SELECT * FROM maintenance_requests WHERE organization_id = $1 AND repository = $2 AND source_kind = $3 AND source_reference = $4",
        )
        .bind(scope.org_id().as_uuid()).bind(repository).bind(source.kind).bind(&source.reference)
        .fetch_optional(&self.pool).await?)
    }

    pub(crate) async fn find_by_task(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
    ) -> AppResult<Option<MaintenanceRequestRow>> {
        Ok(sqlx::query_as::<_, MaintenanceRequestRow>(
            "SELECT * FROM maintenance_requests WHERE organization_id = $1 AND task_id = $2",
        )
        .bind(scope.org_id().as_uuid())
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn create_or_reuse(
        &self,
        scope: &TenantScope,
        request: &PreparedMaintenanceRequest,
        setup: &SelfFixRepositorySetup,
        source_pr: Option<&MaintenancePullRequest>,
    ) -> AppResult<(MaintenanceRequestRow, bool)> {
        let repository = setup.repository.to_ascii_lowercase();
        let mut tx = self.pool.begin().await?;
        // Re-check after external I/O and hold the authority row through commit.
        let admin = UserRepository::find_is_admin_by_id_in_tx(&mut tx, scope.user_id()).await?;
        AdminRolePolicy::require_platform_admin(admin)?;
        // Serialize only this tenant/repository/source. The unique constraint is
        // the authoritative safeguard; hash collisions merely serialize work.
        let lock_key = format!(
            "maintenance:{}:{repository}:{}:{}",
            scope.org_id().as_uuid(),
            request.source.kind,
            request.source.reference
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))").bind(lock_key).execute(&mut *tx).await?;
        let existing = sqlx::query_as::<_, MaintenanceRequestRow>(
            "SELECT * FROM maintenance_requests WHERE organization_id = $1 AND repository = $2 AND source_kind = $3 AND source_reference = $4",
        ).bind(scope.org_id().as_uuid()).bind(&repository).bind(request.source.kind).bind(&request.source.reference)
            .fetch_optional(&mut *tx).await?;
        if let Some(existing) = existing {
            tx.commit().await?;
            return Ok((existing, true));
        }
        let destination = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT g.id FROM groups g
               JOIN projects p ON p.id = g.project_id AND p.organization_id = g.organization_id
               JOIN workspaces w ON w.id = p.workspace_id AND w.organization_id = p.organization_id
               WHERE g.id = $1 AND g.organization_id = $2
                 AND g.deleted_at IS NULL AND p.deleted_at IS NULL AND w.deleted_at IS NULL
               FOR SHARE OF g, p, w"#,
        )
        .bind(request.group_id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(&mut *tx)
        .await?;
        if destination.is_none() {
            return Err(destination_unavailable());
        }
        let params = maintenance_task_params(request, setup, source_pr);
        let task = OrchestrationTaskRepository::create_in_tx(
            &mut tx,
            scope,
            CreateTaskRow {
                group_id: Some(request.group_id),
                title: &request.title,
                description: Some(&request.brief),
                priority: "normal",
                params: Some(&params),
                initial_status: "backlog",
                self_fix: true,
                ..Default::default()
            },
        )
        .await?;
        let row = sqlx::query_as::<_, MaintenanceRequestRow>(
            r#"INSERT INTO maintenance_requests
                (organization_id, task_id, repository, source_kind, source_reference,
                 source_pr_number, source_head_sha, default_branch, starting_sha)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(task.id)
        .bind(&repository)
        .bind(request.source.kind)
        .bind(&request.source.reference)
        .bind(request.source.pr_number)
        .bind(source_pr.map(|pr| &pr.head_sha))
        .bind(&setup.default_branch)
        .bind(&setup.base_sha)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok((row, false))
    }
}
