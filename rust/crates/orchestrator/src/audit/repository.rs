use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use tokio::sync::Mutex;

use super::errors::{AuditError, Result};
use super::model::{AuditFilter, AuditLog};
use super::store::Store;

pub struct MemoryStore {
    seq: AtomicU64,
    logs: Mutex<Vec<AuditLog>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self { seq: AtomicU64::new(1), logs: Mutex::new(Vec::new()) }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Store for MemoryStore {
    async fn create(&self, log: &mut AuditLog) -> Result<()> {
        let mut stored = log.clone();
        stored.id = format!("audit-{}", self.seq.fetch_add(1, Ordering::Relaxed));
        stored.created_at = Utc::now();
        *log = stored.clone();
        self.logs.lock().await.push(stored);
        Ok(())
    }

    async fn list(&self, filter: AuditFilter) -> Result<(Vec<AuditLog>, usize)> {
        let mut logs: Vec<AuditLog> =
            self.logs.lock().await.iter().filter(|log| matches_filter(log, &filter)).cloned().collect();
        logs.sort_by_key(|log| std::cmp::Reverse(log.created_at));
        let total = logs.len();
        let paged = logs.into_iter().skip(filter.offset).take(filter.limit).collect();
        Ok((paged, total))
    }

    async fn export(&self, filter: AuditFilter) -> Result<Vec<AuditLog>> {
        let mut logs: Vec<AuditLog> =
            self.logs.lock().await.iter().filter(|log| matches_filter(log, &filter)).cloned().collect();
        logs.sort_by_key(|log| std::cmp::Reverse(log.created_at));
        logs.truncate(10_000);
        Ok(logs)
    }
}

pub struct PgAuditStore {
    pool: PgPool,
}

impl PgAuditStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Store for PgAuditStore {
    async fn create(&self, log: &mut AuditLog) -> Result<()> {
        let row = sqlx::query(
            "INSERT INTO audit_logs (action, actor_id, actor_type, resource, resource_id, org_id, changes, ip_address, user_agent)                      VALUES ($1, $2, $3, $4, $5, $6, $7, $8::INET, $9)                      RETURNING id::text AS id, created_at"
        )
        .bind(log.action.as_str())
        .bind(&log.actor_id)
        .bind(&log.actor_type)
        .bind(&log.resource)
        .bind(log.resource_id.as_deref())
        .bind(&log.org_id)
        .bind(log.changes.clone())
        .bind(log.ip_address.as_deref())
        .bind(log.user_agent.as_deref())
        .fetch_one(&self.pool)
        .await
        .map_err(|err| AuditError::Internal(format!("insert audit log: {err}")))?;

        log.id = row.try_get("id").map_err(|err| AuditError::Internal(format!("read audit id: {err}")))?;
        log.created_at =
            row.try_get("created_at").map_err(|err| AuditError::Internal(format!("read audit created_at: {err}")))?;
        Ok(())
    }

    async fn list(&self, filter: AuditFilter) -> Result<(Vec<AuditLog>, usize)> {
        let mut count_qb: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) AS count FROM audit_logs WHERE org_id = ");
        count_qb.push_bind(&filter.org_id);
        push_filters(&mut count_qb, &filter);
        let total: i64 = count_qb
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await
            .map_err(|err| AuditError::Internal(format!("count audit logs: {err}")))?;

        let limit = if filter.limit == 0 { 50 } else { filter.limit.min(500) };
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            "SELECT id::text AS id, action, actor_id, actor_type, resource, resource_id, org_id, changes, ip_address::TEXT AS ip_address, user_agent, created_at                      FROM audit_logs WHERE org_id = ",
        );
        qb.push_bind(&filter.org_id);
        push_filters(&mut qb, &filter);
        qb.push(" ORDER BY created_at DESC LIMIT ")
            .push_bind(limit as i64)
            .push(" OFFSET ")
            .push_bind(filter.offset as i64);

        let rows = qb
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| AuditError::Internal(format!("list audit logs: {err}")))?;
        let logs = rows.iter().map(row_to_log).collect::<Result<Vec<_>>>()?;
        Ok((logs, total.max(0) as usize))
    }

    async fn export(&self, filter: AuditFilter) -> Result<Vec<AuditLog>> {
        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            "SELECT id::text AS id, action, actor_id, actor_type, resource, resource_id, org_id, changes, ip_address::TEXT AS ip_address, user_agent, created_at                      FROM audit_logs WHERE org_id = ",
        );
        qb.push_bind(&filter.org_id);
        push_filters(&mut qb, &filter);
        qb.push(" ORDER BY created_at DESC LIMIT ").push_bind(10_000_i64);

        let rows = qb
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|err| AuditError::Internal(format!("export audit logs: {err}")))?;
        rows.iter().map(row_to_log).collect()
    }
}

fn matches_filter(log: &AuditLog, filter: &AuditFilter) -> bool {
    if log.org_id != filter.org_id {
        return false;
    }
    if filter.actor_id.as_deref().is_some_and(|actor_id| log.actor_id != actor_id) {
        return false;
    }
    if filter.resource.as_deref().is_some_and(|resource| log.resource != resource) {
        return false;
    }
    if filter.resource_id.as_deref().is_some_and(|resource_id| log.resource_id.as_deref() != Some(resource_id)) {
        return false;
    }
    if filter.action.is_some_and(|action| log.action != action) {
        return false;
    }
    if filter.from.is_some_and(|from| log.created_at < from) {
        return false;
    }
    if filter.to.is_some_and(|to| log.created_at > to) {
        return false;
    }
    true
}

fn push_filters<'a>(qb: &mut QueryBuilder<'a, Postgres>, filter: &'a AuditFilter) {
    if let Some(actor_id) = filter.actor_id.as_deref() {
        qb.push(" AND actor_id = ").push_bind(actor_id);
    }
    if let Some(resource) = filter.resource.as_deref() {
        qb.push(" AND resource = ").push_bind(resource);
    }
    if let Some(resource_id) = filter.resource_id.as_deref() {
        qb.push(" AND resource_id = ").push_bind(resource_id);
    }
    if let Some(action) = filter.action {
        qb.push(" AND action = ").push_bind(action.as_str());
    }
    if let Some(from) = filter.from {
        qb.push(" AND created_at >= ").push_bind(from);
    }
    if let Some(to) = filter.to {
        qb.push(" AND created_at <= ").push_bind(to);
    }
}

fn row_to_log(row: &PgRow) -> Result<AuditLog> {
    let action =
        row.try_get::<String, _>("action").map_err(|err| AuditError::Internal(format!("read audit action: {err}")))?;
    Ok(AuditLog {
        id: row.try_get("id").map_err(|err| AuditError::Internal(format!("read audit id: {err}")))?,
        action: action.parse().map_err(AuditError::Internal)?,
        actor_id: row.try_get("actor_id").map_err(|err| AuditError::Internal(format!("read actor_id: {err}")))?,
        actor_type: row.try_get("actor_type").map_err(|err| AuditError::Internal(format!("read actor_type: {err}")))?,
        resource: row.try_get("resource").map_err(|err| AuditError::Internal(format!("read resource: {err}")))?,
        resource_id: row
            .try_get("resource_id")
            .map_err(|err| AuditError::Internal(format!("read resource_id: {err}")))?,
        org_id: row.try_get("org_id").map_err(|err| AuditError::Internal(format!("read org_id: {err}")))?,
        changes: row.try_get("changes").map_err(|err| AuditError::Internal(format!("read changes: {err}")))?,
        ip_address: row.try_get("ip_address").map_err(|err| AuditError::Internal(format!("read ip_address: {err}")))?,
        user_agent: row.try_get("user_agent").map_err(|err| AuditError::Internal(format!("read user_agent: {err}")))?,
        created_at: row.try_get("created_at").map_err(|err| AuditError::Internal(format!("read created_at: {err}")))?,
    })
}
