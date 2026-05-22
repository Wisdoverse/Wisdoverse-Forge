//! Context preview repository for publish-time stale guards.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::ContextPreview;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::context_preview::ContextPreviewAccessPolicy;

pub struct CreateContextPreviewRecord<'a> {
    pub workspace_id: Uuid,
    pub task_id: Uuid,
    pub agent_id: Uuid,
    pub task_draft_hash: &'a str,
    pub preview_hash: &'a str,
    pub selected_items: &'a Value,
    pub expires_at: DateTime<Utc>,
}

pub struct ContextPreviewRepository {
    pool: PgPool,
}

impl ContextPreviewRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        scope: &TenantScope,
        record: CreateContextPreviewRecord<'_>,
    ) -> AppResult<ContextPreview> {
        let row = sqlx::query_as::<_, ContextPreview>(
            r#"INSERT INTO context_previews (
                   organization_id, workspace_id, task_id, agent_id, created_by_user_id,
                   task_draft_hash, preview_hash, selected_items, expires_at
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(record.workspace_id)
        .bind(record.task_id)
        .bind(record.agent_id)
        .bind(scope.user_id().as_uuid())
        .bind(record.task_draft_hash)
        .bind(record.preview_hash)
        .bind(record.selected_items)
        .bind(record.expires_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn find_live_for_publish(
        &self,
        scope: &TenantScope,
        id: Uuid,
        task_id: Uuid,
    ) -> AppResult<ContextPreview> {
        let workspace_id = ContextPreviewAccessPolicy::required_workspace(scope)?;
        let row = sqlx::query_as::<_, ContextPreview>(
            r#"SELECT *
                 FROM context_previews
                WHERE id = $1
                  AND organization_id = $2
                  AND workspace_id = $3
                  AND task_id = $4
                  AND created_by_user_id = $5
                  AND expires_at > now()"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(task_id)
        .bind(scope.user_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ContextPreviewAccessPolicy::not_found(id))?;
        Ok(row)
    }
}
