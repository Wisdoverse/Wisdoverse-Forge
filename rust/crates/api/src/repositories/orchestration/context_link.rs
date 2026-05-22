//! Context provenance link repository.

use agentforge_core::{AppResult, TenantScope, UserId, WorkspaceId};
use agentforge_db::entities::ContextLink;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::orchestration::OrchestrationRepositoryPolicy;

pub struct CreateContextLinkRecord<'a> {
    pub workspace_id: WorkspaceId,
    pub item_id: Uuid,
    pub item_kind: &'a str,
    pub ref_id: Uuid,
    pub ref_kind: &'a str,
    pub link_type: &'a str,
    pub created_by_user_id: UserId,
}

#[derive(Debug, Clone, FromRow)]
pub struct ContextLinkedRunRow {
    pub link_id: Uuid,
    pub run_id: Uuid,
    pub run_status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub link_type: String,
    pub linked_at: DateTime<Utc>,
}

pub struct ContextLinkRepository {
    pool: PgPool,
}

impl ContextLinkRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        record: CreateContextLinkRecord<'_>,
    ) -> AppResult<ContextLink> {
        require_workspace(scope, record.workspace_id)?;
        validate_actor_in_org(tx, scope, record.created_by_user_id).await?;
        validate_item_exists(tx, scope, record.workspace_id, record.item_kind, record.item_id).await?;
        validate_ref_exists(tx, scope, record.workspace_id, record.ref_kind, record.ref_id).await?;

        sqlx::query_as::<_, ContextLink>(
            r#"INSERT INTO context_links (
                   organization_id, workspace_id, item_id, item_kind, ref_id, ref_kind,
                   link_type, created_by_user_id
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT ON CONSTRAINT context_links_unique_link DO UPDATE
                  SET created_by_user_id = context_links.created_by_user_id
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(record.item_id)
        .bind(record.item_kind)
        .bind(record.ref_id)
        .bind(record.ref_kind)
        .bind(record.link_type)
        .bind(record.created_by_user_id.as_uuid())
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn runs_for_item(
        &self,
        scope: &TenantScope,
        item_id: Uuid,
        item_kind: &str,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<ContextLinkedRunRow>> {
        let workspace_id = OrchestrationRepositoryPolicy::required_workspace(scope)?;
        let rows = sqlx::query_as::<_, ContextLinkedRunRow>(
            r#"SELECT cl.id AS link_id,
                      tr.id AS run_id,
                      tr.status AS run_status,
                      tr.started_at,
                      tr.finished_at,
                      cl.link_type,
                      cl.created_at AS linked_at
                 FROM context_links cl
                 JOIN task_runs tr
                   ON tr.id = cl.ref_id
                  AND tr.organization_id = cl.organization_id
                  AND tr.workspace_id = cl.workspace_id
                WHERE cl.organization_id = $1
                  AND cl.workspace_id = $2
                  AND cl.item_id = $3
                  AND cl.item_kind = $4
                  AND cl.ref_kind = 'run'
                  AND cl.link_type = 'applied'
                ORDER BY tr.started_at DESC, cl.created_at DESC, cl.id DESC
                LIMIT $5 OFFSET $6"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(item_id)
        .bind(item_kind)
        .bind(normalize_limit(limit))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn explain_runs_for_item_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        item_id: Uuid,
        item_kind: &str,
    ) -> AppResult<Vec<String>> {
        let plan = sqlx::query_scalar::<_, String>(
            r#"EXPLAIN
               SELECT id, ref_id, link_type, created_at
                 FROM context_links
                WHERE item_id = $1
                  AND item_kind = $2
                  AND ref_kind = 'run'
                  AND link_type = 'applied'
                ORDER BY created_at DESC
                LIMIT 50"#,
        )
        .bind(item_id)
        .bind(item_kind)
        .fetch_all(&mut **tx)
        .await?;
        Ok(plan)
    }
}

fn require_workspace(scope: &TenantScope, workspace_id: WorkspaceId) -> AppResult<()> {
    OrchestrationRepositoryPolicy::ensure_workspace(scope, workspace_id)
}

async fn validate_actor_in_org(
    tx: &mut Transaction<'_, Postgres>,
    scope: &TenantScope,
    user_id: UserId,
) -> AppResult<()> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS (
               SELECT 1
                 FROM organization_members
                WHERE organization_id = $1
                  AND user_id = $2
           )"#,
    )
    .bind(scope.org_id().as_uuid())
    .bind(user_id.as_uuid())
    .fetch_one(&mut **tx)
    .await?;
    OrchestrationRepositoryPolicy::ensure_exists_or_forbidden(exists)
}

async fn validate_item_exists(
    tx: &mut Transaction<'_, Postgres>,
    scope: &TenantScope,
    workspace_id: WorkspaceId,
    item_kind: &str,
    item_id: Uuid,
) -> AppResult<()> {
    let exists = match item_kind {
        "memory" => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS (
                       SELECT 1
                         FROM memory_items
                        WHERE id = $1
                          AND organization_id = $2
                          AND workspace_id = $3
                   )"#,
            )
            .bind(item_id)
            .bind(scope.org_id().as_uuid())
            .bind(workspace_id.as_uuid())
            .fetch_one(&mut **tx)
            .await?
        }
        "skill" => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS (
                       SELECT 1
                         FROM skills
                        WHERE id = $1
                          AND organization_id = $2
                          AND workspace_id = $3
                   )"#,
            )
            .bind(item_id)
            .bind(scope.org_id().as_uuid())
            .bind(workspace_id.as_uuid())
            .fetch_one(&mut **tx)
            .await?
        }
        _ => return Err(OrchestrationRepositoryPolicy::invalid_context_item_kind(item_kind)),
    };

    OrchestrationRepositoryPolicy::ensure_exists_or_forbidden(exists)
}

async fn validate_ref_exists(
    tx: &mut Transaction<'_, Postgres>,
    scope: &TenantScope,
    workspace_id: WorkspaceId,
    ref_kind: &str,
    ref_id: Uuid,
) -> AppResult<()> {
    let exists = match ref_kind {
        "run" => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS (
                       SELECT 1
                         FROM task_runs
                        WHERE id = $1
                          AND organization_id = $2
                          AND workspace_id = $3
                   )"#,
            )
            .bind(ref_id)
            .bind(scope.org_id().as_uuid())
            .bind(workspace_id.as_uuid())
            .fetch_one(&mut **tx)
            .await?
        }
        "task" => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS (
                       SELECT 1
                         FROM orchestration_tasks task
                         LEFT JOIN agents agent
                           ON agent.id = task.assigned_agent_id
                          AND agent.organization_id = task.organization_id
                        WHERE task.id = $1
                          AND task.organization_id = $2
                          AND (agent.workspace_id = $3 OR task.assigned_agent_id IS NULL)
                   )"#,
            )
            .bind(ref_id)
            .bind(scope.org_id().as_uuid())
            .bind(workspace_id.as_uuid())
            .fetch_one(&mut **tx)
            .await?
        }
        "agent" => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS (
                       SELECT 1 FROM agents
                        WHERE id = $1 AND organization_id = $2 AND workspace_id = $3
                   )"#,
            )
            .bind(ref_id)
            .bind(scope.org_id().as_uuid())
            .bind(workspace_id.as_uuid())
            .fetch_one(&mut **tx)
            .await?
        }
        "user" => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS (
                       SELECT 1 FROM organization_members
                        WHERE user_id = $1 AND organization_id = $2
                   )"#,
            )
            .bind(ref_id)
            .bind(scope.org_id().as_uuid())
            .fetch_one(&mut **tx)
            .await?
        }
        "team" => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS (
                       SELECT 1 FROM teams
                        WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL
                   )"#,
            )
            .bind(ref_id)
            .bind(scope.org_id().as_uuid())
            .fetch_one(&mut **tx)
            .await?
        }
        "project" => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS (
                       SELECT 1 FROM projects
                        WHERE id = $1 AND organization_id = $2 AND workspace_id = $3 AND deleted_at IS NULL
                   )"#,
            )
            .bind(ref_id)
            .bind(scope.org_id().as_uuid())
            .bind(workspace_id.as_uuid())
            .fetch_one(&mut **tx)
            .await?
        }
        "source_message" => {
            sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS (
                       SELECT 1
                         FROM agent_messages msg
                         JOIN agents agent
                           ON agent.id = msg.agent_id
                          AND agent.organization_id = msg.organization_id
                        WHERE msg.id = $1
                          AND msg.organization_id = $2
                          AND agent.workspace_id = $3
                   )"#,
            )
            .bind(ref_id)
            .bind(scope.org_id().as_uuid())
            .bind(workspace_id.as_uuid())
            .fetch_one(&mut **tx)
            .await?
        }
        _ => return Err(OrchestrationRepositoryPolicy::invalid_context_ref_kind(ref_kind)),
    };

    OrchestrationRepositoryPolicy::ensure_exists_or_forbidden(exists)
}

fn normalize_limit(limit: i64) -> i64 {
    limit.clamp(1, 200)
}
