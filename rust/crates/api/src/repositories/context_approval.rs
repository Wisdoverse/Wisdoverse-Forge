//! Context approval repository.

use agentforge_core::{AppResult, ErrorKind, UserId};
use agentforge_db::entities::ContextApproval;
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub struct CreateContextApprovalRecord<'a> {
    pub candidate_id: Uuid,
    pub approver_user_id: UserId,
    pub decision: &'a str,
    pub scope_kind: Option<&'a str>,
    pub scope_id: Option<Uuid>,
    pub ttl_at: Option<DateTime<Utc>>,
    pub sensitivity: Option<&'a str>,
    pub reason: Option<&'a str>,
    pub self_approval: bool,
    pub user_attest_at: Option<DateTime<Utc>>,
}

pub struct ContextApprovalRepository;

impl ContextApprovalRepository {
    pub async fn create_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        record: CreateContextApprovalRecord<'_>,
    ) -> AppResult<ContextApproval> {
        sqlx::query_as::<_, ContextApproval>(
            r#"INSERT INTO context_approvals (
                   candidate_id, approver_user_id, decision, scope_kind, scope_id,
                   ttl_at, sensitivity, reason, self_approval, user_attest_at
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
               RETURNING *"#,
        )
        .bind(record.candidate_id)
        .bind(record.approver_user_id.as_uuid())
        .bind(record.decision)
        .bind(record.scope_kind)
        .bind(record.scope_id)
        .bind(record.ttl_at)
        .bind(record.sensitivity)
        .bind(record.reason)
        .bind(record.self_approval)
        .bind(record.user_attest_at)
        .fetch_one(&mut **tx)
        .await
        .map_err(map_insert_error)
    }
}

fn map_insert_error(err: sqlx::Error) -> agentforge_core::AppError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(db_err.constraint(), Some("idx_context_approvals_candidate_once"))
    {
        return ErrorKind::Conflict("context candidate already has an approval decision".into()).into();
    }
    err.into()
}
