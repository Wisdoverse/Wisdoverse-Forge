//! Read-only projection for governed context audit events.
//!
//! The current governance mutation services persist their audit trail in
//! `audit_log` using `governance.context.*` actions. This repository projects
//! those rows into a scope-aware shape without widening the write surface.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use agentforge_core::{AppResult, TenantScope};

pub const GOVERNANCE_CONTEXT_AUDIT_PREFIX: &str = "governance.context.";
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

#[derive(Debug, Clone)]
pub struct GovernanceAuditFilter<'a> {
    pub event_type: Option<&'a str>,
    pub event_prefix: Option<&'a str>,
    pub item_kind: Option<&'a str>,
    pub scope_kind: Option<&'a str>,
    pub scope_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct GovernanceAuditRow {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub event_type: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub details: Value,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub item_kind: Option<String>,
    pub subject_item_id: Option<Uuid>,
    pub subject_scope_kind: Option<String>,
    pub subject_scope_id: Option<Uuid>,
    pub visible_by_scope: bool,
}

pub struct GovernanceAuditRepository {
    pool: PgPool,
}

impl GovernanceAuditRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(
        &self,
        scope: &TenantScope,
        filter: GovernanceAuditFilter<'_>,
        include_org_wide: bool,
    ) -> AppResult<Vec<GovernanceAuditRow>> {
        let read = scope.scoped_read();
        let workspace_ids: Vec<Uuid> = read.workspace_ids().iter().map(|id| id.as_uuid()).collect();
        let team_ids: Vec<Uuid> = read.team_ids().iter().map(|id| id.as_uuid()).collect();
        let project_ids: Vec<Uuid> = read.project_ids().iter().map(|id| id.as_uuid()).collect();
        let event_prefix = filter.event_prefix.unwrap_or(GOVERNANCE_CONTEXT_AUDIT_PREFIX);
        let limit = normalize_limit(filter.limit);
        let offset = filter.offset.unwrap_or(0).max(0);

        let rows = sqlx::query_as::<_, GovernanceAuditRow>(
            r#"
            WITH projected AS (
                SELECT
                    al.id,
                    al.user_id AS actor_user_id,
                    al.action AS event_type,
                    al.resource_type,
                    al.resource_id,
                    al.details,
                    al.ip_address,
                    al.created_at,
                    COALESCE(
                        NULLIF(al.details ->> 'item_kind', ''),
                        CASE
                            WHEN al.resource_type = 'memory_item' THEN 'memory'
                            WHEN al.resource_type = 'skill' THEN 'skill'
                            ELSE NULL
                        END
                    ) AS item_kind,
                    COALESCE(
                        CASE
                            WHEN al.details ->> 'item_id' ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                                THEN (al.details ->> 'item_id')::UUID
                            ELSE NULL
                        END,
                        CASE
                            WHEN al.details ->> 'memory_id' ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                                THEN (al.details ->> 'memory_id')::UUID
                            ELSE NULL
                        END,
                        CASE
                            WHEN al.details ->> 'skill_id' ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                                THEN (al.details ->> 'skill_id')::UUID
                            ELSE NULL
                        END,
                        CASE
                            WHEN al.resource_type IN ('memory_item', 'skill') THEN al.resource_id
                            ELSE NULL
                        END
                    ) AS subject_item_id
                FROM audit_log al
                WHERE al.organization_id = $1
                  AND al.action LIKE ($2 || '%')
                  AND ($3::TEXT IS NULL OR al.action = $3)
                  AND ($8::UUID IS NULL OR al.user_id = $8)
                  AND ($9::TIMESTAMPTZ IS NULL OR al.created_at >= $9)
                  AND ($10::TIMESTAMPTZ IS NULL OR al.created_at < $10)
            ),
            scoped AS (
                SELECT
                    p.*,
                    COALESCE(NULLIF(p.details ->> 'scope_kind', ''), mi.scope_kind, sk.scope_kind) AS subject_scope_kind,
                    COALESCE(
                        CASE
                            WHEN p.details ->> 'scope_id' ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                                THEN (p.details ->> 'scope_id')::UUID
                            ELSE NULL
                        END,
                        mi.scope_id,
                        sk.scope_id
                    ) AS subject_scope_id
                FROM projected p
                LEFT JOIN memory_items mi
                  ON mi.organization_id = $1
                 AND p.subject_item_id = mi.id
                LEFT JOIN skills sk
                  ON sk.organization_id = $1
                 AND p.subject_item_id = sk.id
            )
            SELECT
                id,
                actor_user_id,
                event_type,
                resource_type,
                resource_id,
                details,
                ip_address,
                created_at,
                item_kind,
                subject_item_id,
                subject_scope_kind,
                subject_scope_id,
                CASE
                    WHEN $11::BOOL THEN TRUE
                    WHEN subject_scope_kind = 'org' THEN TRUE
                    WHEN subject_scope_kind = 'user' AND subject_scope_id = $12 THEN TRUE
                    WHEN subject_scope_kind = 'workspace' AND subject_scope_id = ANY($13) THEN TRUE
                    WHEN subject_scope_kind = 'team' AND subject_scope_id = ANY($14) THEN TRUE
                    WHEN subject_scope_kind = 'project' AND subject_scope_id = ANY($15) THEN TRUE
                    ELSE FALSE
                END AS visible_by_scope
            FROM scoped
            WHERE ($4::TEXT IS NULL OR item_kind = $4)
              AND ($5::TEXT IS NULL OR subject_scope_kind = $5)
              AND ($6::UUID IS NULL OR subject_scope_id = $6)
              AND (
                    $11::BOOL
                    OR actor_user_id = $7
                    OR subject_scope_kind = 'org'
                    OR (subject_scope_kind = 'user' AND subject_scope_id = $12)
                    OR (subject_scope_kind = 'workspace' AND subject_scope_id = ANY($13))
                    OR (subject_scope_kind = 'team' AND subject_scope_id = ANY($14))
                    OR (subject_scope_kind = 'project' AND subject_scope_id = ANY($15))
                  )
            ORDER BY created_at DESC, id DESC
            LIMIT $16 OFFSET $17
            "#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(event_prefix)
        .bind(filter.event_type)
        .bind(filter.item_kind)
        .bind(filter.scope_kind)
        .bind(filter.scope_id)
        .bind(scope.user_id().as_uuid())
        .bind(filter.user_id)
        .bind(filter.from)
        .bind(filter.to)
        .bind(include_org_wide)
        .bind(scope.user_id().as_uuid())
        .bind(&workspace_ids)
        .bind(&team_ids)
        .bind(&project_ids)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}

pub fn normalize_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}
