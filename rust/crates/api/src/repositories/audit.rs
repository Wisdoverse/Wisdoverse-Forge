//! Audit log repository — database queries for the audit_log table.

use agentforge_core::{AppResult, OrgId, UserId};
use agentforge_db::entities::AuditLogEntry;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Database access layer for audit log entries.
pub struct AuditRepository {
    pool: PgPool,
}

impl AuditRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List audit log entries for an org (paginated, with optional filters).
    pub async fn list(
        &self,
        org_id: OrgId,
        action: Option<&str>,
        resource_type: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<AuditLogEntry>> {
        // Build dynamic query with optional filters
        let entries = sqlx::query_as::<_, AuditLogEntry>(
            r#"SELECT * FROM audit_log
               WHERE organization_id = $1
                 AND ($2::TEXT IS NULL OR action = $2)
                 AND ($3::TEXT IS NULL OR resource_type = $3)
               ORDER BY created_at DESC
               LIMIT $4 OFFSET $5"#,
        )
        .bind(org_id.as_uuid())
        .bind(action)
        .bind(resource_type)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(entries)
    }

    /// Insert an audit log entry.
    pub async fn create(
        &self,
        org_id: OrgId,
        user_id: Option<UserId>,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        details: &serde_json::Value,
        ip_address: Option<&str>,
    ) -> AppResult<AuditLogEntry> {
        sqlx::query_as::<_, AuditLogEntry>(
            r#"INSERT INTO audit_log (organization_id, user_id, action, resource_type, resource_id, details, ip_address)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING *"#,
        )
        .bind(org_id.as_uuid())
        .bind(user_id.map(|u| u.as_uuid()))
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(details)
        .bind(ip_address)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Insert an audit log entry inside the caller's transaction.
    pub async fn create_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        org_id: OrgId,
        user_id: Option<UserId>,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        details: &serde_json::Value,
        ip_address: Option<&str>,
    ) -> AppResult<AuditLogEntry> {
        sqlx::query_as::<_, AuditLogEntry>(
            r#"INSERT INTO audit_log (organization_id, user_id, action, resource_type, resource_id, details, ip_address)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING *"#,
        )
        .bind(org_id.as_uuid())
        .bind(user_id.map(|u| u.as_uuid()))
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(details)
        .bind(ip_address)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }
}
