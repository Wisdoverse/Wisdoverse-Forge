//! Tenant-scoped append-only evidence SQL. Services own transaction boundaries.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::TaskRun;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;
mod outcomes;
pub(crate) use outcomes::SummaryRow;

#[derive(Debug, FromRow, Deserialize)]
pub(crate) struct ReportRow {
    pub id: Uuid,
    pub task_id: Uuid,
    pub run_id: Option<Uuid>,
    pub author_id: Uuid,
    pub task_version: i64,
    pub revision: String,
    pub starting_revision: String,
    pub criteria: String,
    pub comparison_key: Option<String>,
    pub input: Value,
    pub snapshot: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Deserialize)]
pub(crate) struct DecisionRow {
    pub id: Uuid,
    pub report_id: Uuid,
    pub reviewer_id: Uuid,
    pub input: Value,
    pub github_observation: Option<Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub(crate) struct HandoffRow {
    pub id: Uuid,
    pub task_id: Uuid,
    pub author_id: Uuid,
    pub reason: String,
    pub next_step: String,
    pub input: Value,
    pub snapshot: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub(crate) struct EvidenceRow {
    pub source_type: String,
    pub source_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub(crate) struct BridgeRow {
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub run_at: DateTime<Utc>,
}

pub(crate) struct CreateDecisionRow<'a> {
    pub task: Uuid,
    pub key: Uuid,
    pub report: Uuid,
    pub verdict: &'a str,
    pub reason: &'a str,
    pub minutes: [Option<i32>; 6],
    pub baseline: Option<i32>,
    pub input: &'a Value,
    pub observation: Option<&'a Value>,
}
pub(crate) struct CreateHandoffRow<'a> {
    pub task: Uuid,
    pub key: Uuid,
    pub reason: &'a str,
    pub next_step: &'a str,
    pub input: &'a Value,
    pub snapshot: &'a Value,
}

pub(crate) struct MaintenanceDeliveryRepository {
    pool: PgPool,
}

impl MaintenanceDeliveryRepository {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn reports(&self, scope: &TenantScope, task: Uuid) -> AppResult<Vec<ReportRow>> {
        Ok(sqlx::query_as("SELECT * FROM maintenance_verification_reports WHERE organization_id=$1 AND task_id=$2 ORDER BY created_at DESC,id DESC LIMIT 21")
            .bind(scope.org_id().as_uuid()).bind(task).fetch_all(&self.pool).await?)
    }

    pub(crate) async fn decisions(&self, scope: &TenantScope, task: Uuid) -> AppResult<Vec<DecisionRow>> {
        Ok(sqlx::query_as("SELECT * FROM maintenance_review_decisions WHERE organization_id=$1 AND task_id=$2 ORDER BY created_at DESC,id DESC LIMIT 101")
            .bind(scope.org_id().as_uuid()).bind(task).fetch_all(&self.pool).await?)
    }

    pub(crate) async fn handoffs(&self, scope: &TenantScope, task: Uuid) -> AppResult<Vec<HandoffRow>> {
        Ok(sqlx::query_as("SELECT * FROM maintenance_handoffs WHERE organization_id=$1 AND task_id=$2 ORDER BY created_at DESC,id DESC LIMIT 21")
            .bind(scope.org_id().as_uuid()).bind(task).fetch_all(&self.pool).await?)
    }

    pub(crate) async fn latest_run(&self, scope: &TenantScope, task: Uuid) -> AppResult<Option<TaskRun>> {
        Ok(sqlx::query_as("SELECT * FROM task_runs WHERE organization_id=$1 AND orchestration_task_id=$2 ORDER BY started_at DESC,created_at DESC,id DESC LIMIT 1")
            .bind(scope.org_id().as_uuid()).bind(task).fetch_optional(&self.pool).await?)
    }

    pub(crate) async fn latest_run_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        task: Uuid,
    ) -> AppResult<Option<TaskRun>> {
        Ok(sqlx::query_as("SELECT * FROM task_runs WHERE organization_id=$1 AND orchestration_task_id=$2 ORDER BY started_at DESC,created_at DESC,id DESC LIMIT 1 FOR SHARE")
            .bind(scope.org_id().as_uuid()).bind(task).fetch_optional(&mut **tx).await?)
    }

    pub(crate) async fn evidence(&self, scope: &TenantScope, run: Uuid) -> AppResult<Vec<EvidenceRow>> {
        Ok(sqlx::query_as("SELECT source_type,source_id,created_at FROM v_run_evidence WHERE organization_id=$1 AND run_id=$2 ORDER BY created_at,source_id LIMIT 101")
            .bind(scope.org_id().as_uuid()).bind(run).fetch_all(&self.pool).await?)
    }

    pub(crate) async fn bridge(&self, scope: &TenantScope, task: Uuid) -> AppResult<Option<BridgeRow>> {
        Ok(sqlx::query_as("SELECT status,attempts,max_attempts,run_at FROM job_queue WHERE queue='self_fix_pr' AND unique_key=$2 AND payload->>'org_id'=$1 AND payload->>'task_id'=$2 ORDER BY created_at DESC LIMIT 1")
            .bind(scope.org_id().as_uuid().to_string()).bind(task.to_string()).fetch_optional(&self.pool).await?)
    }

    pub(crate) async fn report(&self, scope: &TenantScope, id: Uuid) -> AppResult<Option<ReportRow>> {
        Ok(sqlx::query_as("SELECT * FROM maintenance_verification_reports WHERE organization_id=$1 AND id=$2")
            .bind(scope.org_id().as_uuid())
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub(crate) async fn latest_decision(&self, scope: &TenantScope, report: Uuid) -> AppResult<Option<DecisionRow>> {
        Ok(sqlx::query_as("SELECT * FROM maintenance_review_decisions WHERE organization_id=$1 AND report_id=$2 ORDER BY created_at DESC,id DESC LIMIT 1")
            .bind(scope.org_id().as_uuid()).bind(report).fetch_optional(&self.pool).await?)
    }

    pub(crate) async fn report_by_key_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        task: Uuid,
        key: Uuid,
    ) -> AppResult<Option<ReportRow>> {
        Ok(sqlx::query_as(
            "SELECT * FROM maintenance_verification_reports WHERE organization_id=$1 AND task_id=$2 AND request_key=$3",
        )
        .bind(scope.org_id().as_uuid())
        .bind(task)
        .bind(key)
        .fetch_optional(&mut **tx)
        .await?)
    }

    pub(crate) async fn latest_report_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        task: Uuid,
    ) -> AppResult<Option<ReportRow>> {
        Ok(sqlx::query_as("SELECT * FROM maintenance_verification_reports WHERE organization_id=$1 AND task_id=$2 ORDER BY created_at DESC,id DESC LIMIT 1")
            .bind(scope.org_id().as_uuid()).bind(task).fetch_optional(&mut **tx).await?)
    }

    pub(crate) async fn decision_by_key_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        task: Uuid,
        key: Uuid,
    ) -> AppResult<Option<DecisionRow>> {
        Ok(sqlx::query_as(
            "SELECT * FROM maintenance_review_decisions WHERE organization_id=$1 AND task_id=$2 AND request_key=$3",
        )
        .bind(scope.org_id().as_uuid())
        .bind(task)
        .bind(key)
        .fetch_optional(&mut **tx)
        .await?)
    }

    pub(crate) async fn handoff_by_key_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        task: Uuid,
        key: Uuid,
    ) -> AppResult<Option<HandoffRow>> {
        Ok(sqlx::query_as(
            "SELECT * FROM maintenance_handoffs WHERE organization_id=$1 AND task_id=$2 AND request_key=$3",
        )
        .bind(scope.org_id().as_uuid())
        .bind(task)
        .bind(key)
        .fetch_optional(&mut **tx)
        .await?)
    }

    pub(crate) async fn insert_report_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        row: &ReportRow,
        key: Uuid,
    ) -> AppResult<ReportRow> {
        Ok(sqlx::query_as("INSERT INTO maintenance_verification_reports (id,organization_id,task_id,run_id,live_run_id,request_key,author_id,task_version,revision,starting_revision,criteria,comparison_key,input,snapshot) VALUES ($1,$2,$3,$4,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING *")
            .bind(row.id).bind(scope.org_id().as_uuid()).bind(row.task_id).bind(row.run_id).bind(key).bind(scope.user_id().as_uuid())
            .bind(row.task_version).bind(&row.revision).bind(&row.starting_revision).bind(&row.criteria).bind(&row.comparison_key)
            .bind(&row.input).bind(&row.snapshot).fetch_one(&mut **tx).await?)
    }

    pub(crate) async fn insert_decision_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        row: CreateDecisionRow<'_>,
    ) -> AppResult<DecisionRow> {
        Ok(sqlx::query_as("INSERT INTO maintenance_review_decisions (id,organization_id,task_id,report_id,request_key,reviewer_id,verdict,reason,setup_minutes,handling_minutes,review_minutes,recovery_minutes,rework_minutes,operation_minutes,baseline_minutes,input,github_observation) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) RETURNING *")
            .bind(Uuid::now_v7()).bind(scope.org_id().as_uuid()).bind(row.task).bind(row.report).bind(row.key).bind(scope.user_id().as_uuid()).bind(row.verdict).bind(row.reason)
            .bind(row.minutes[0]).bind(row.minutes[1]).bind(row.minutes[2]).bind(row.minutes[3]).bind(row.minutes[4]).bind(row.minutes[5]).bind(row.baseline).bind(row.input).bind(row.observation)
            .fetch_one(&mut **tx).await?)
    }

    pub(crate) async fn insert_handoff_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        row: CreateHandoffRow<'_>,
    ) -> AppResult<HandoffRow> {
        Ok(sqlx::query_as("INSERT INTO maintenance_handoffs (id,organization_id,task_id,request_key,author_id,reason,next_step,input,snapshot) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *")
            .bind(Uuid::now_v7()).bind(scope.org_id().as_uuid()).bind(row.task).bind(row.key).bind(scope.user_id().as_uuid()).bind(row.reason).bind(row.next_step).bind(row.input).bind(row.snapshot)
            .fetch_one(&mut **tx).await?)
    }
}
