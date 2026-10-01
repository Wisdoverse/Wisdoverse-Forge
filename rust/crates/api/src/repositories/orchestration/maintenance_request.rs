//! Organization-scoped maintenance-source SQL primitives. Services own transactions.

use agentforge_core::{AppResult, TenantScope};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::maintenance::MaintenanceSourceIdentity;

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

pub(crate) struct CreateMaintenanceRequestRow<'a> {
    pub task_id: Uuid,
    pub repository: &'a str,
    pub source_kind: &'a str,
    pub source_reference: &'a str,
    pub source_pr_number: Option<i32>,
    pub source_head_sha: Option<&'a str>,
    pub default_branch: &'a str,
    pub starting_sha: &'a str,
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

    /// Lock one tenant/repository/source in a caller-owned transaction.
    pub(crate) async fn lock_source_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        repository: &str,
        source: &MaintenanceSourceIdentity,
    ) -> AppResult<()> {
        let lock_key =
            format!("maintenance:{}:{repository}:{}:{}", scope.org_id().as_uuid(), source.kind, source.reference);
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))").bind(lock_key).execute(&mut **tx).await?;
        Ok(())
    }

    pub(crate) async fn find_by_source_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        repository: &str,
        source: &MaintenanceSourceIdentity,
    ) -> AppResult<Option<MaintenanceRequestRow>> {
        Ok(sqlx::query_as::<_, MaintenanceRequestRow>(
            "SELECT * FROM maintenance_requests WHERE organization_id = $1 AND repository = $2 AND source_kind = $3 AND source_reference = $4",
        ).bind(scope.org_id().as_uuid()).bind(repository).bind(source.kind).bind(&source.reference)
            .fetch_optional(&mut **tx).await?)
    }

    pub(crate) async fn insert_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        input: CreateMaintenanceRequestRow<'_>,
    ) -> AppResult<MaintenanceRequestRow> {
        let row = sqlx::query_as::<_, MaintenanceRequestRow>(
            r#"INSERT INTO maintenance_requests
                (organization_id, task_id, repository, source_kind, source_reference,
                 source_pr_number, source_head_sha, default_branch, starting_sha)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(input.task_id)
        .bind(input.repository)
        .bind(input.source_kind)
        .bind(input.source_reference)
        .bind(input.source_pr_number)
        .bind(input.source_head_sha)
        .bind(input.default_branch)
        .bind(input.starting_sha)
        .fetch_one(&mut **tx)
        .await?;
        Ok(row)
    }
}
