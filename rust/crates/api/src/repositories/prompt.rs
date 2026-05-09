//! Prompt repository — database queries for the prompts table.

use agentforge_core::{AppError, AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::Prompt;
use sqlx::PgPool;
use uuid::Uuid;

/// Database access layer for prompts.
pub struct PromptRepository {
    pool: PgPool,
}

impl PromptRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List prompts — user's own + shared prompts in the org.
    pub async fn list(
        &self,
        scope: &TenantScope,
        shared_only: Option<bool>,
        tags: Option<&[String]>,
    ) -> AppResult<Vec<Prompt>> {
        // Build query dynamically based on filters
        let prompts = match (shared_only, tags) {
            (Some(true), Some(tags)) if !tags.is_empty() => {
                sqlx::query_as::<_, Prompt>(
                    r#"SELECT * FROM prompts
                       WHERE organization_id = $1 AND is_shared = true AND tags && $2
                       ORDER BY updated_at DESC"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(tags)
                .fetch_all(&self.pool)
                .await?
            }
            (Some(true), _) => {
                sqlx::query_as::<_, Prompt>(
                    r#"SELECT * FROM prompts
                       WHERE organization_id = $1 AND is_shared = true
                       ORDER BY updated_at DESC"#,
                )
                .bind(scope.org_id().as_uuid())
                .fetch_all(&self.pool)
                .await?
            }
            (_, Some(tags)) if !tags.is_empty() => {
                sqlx::query_as::<_, Prompt>(
                    r#"SELECT * FROM prompts
                       WHERE organization_id = $1 AND (user_id = $2 OR is_shared = true) AND tags && $3
                       ORDER BY updated_at DESC"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(scope.user_id().as_uuid())
                .bind(tags)
                .fetch_all(&self.pool)
                .await?
            }
            _ => {
                sqlx::query_as::<_, Prompt>(
                    r#"SELECT * FROM prompts
                       WHERE organization_id = $1 AND (user_id = $2 OR is_shared = true)
                       ORDER BY updated_at DESC"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(scope.user_id().as_uuid())
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(prompts)
    }

    /// Get a single prompt by ID (tenant-scoped).
    pub async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<Prompt> {
        sqlx::query_as::<_, Prompt>(
            r#"SELECT * FROM prompts
               WHERE id = $1 AND organization_id = $2 AND (user_id = $3 OR is_shared = true)"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("prompt {id}")).into())
    }

    /// Create a new prompt.
    pub async fn create(
        &self,
        scope: &TenantScope,
        title: &str,
        content: &str,
        tags: &[String],
        is_shared: bool,
    ) -> AppResult<Prompt> {
        let prompt = sqlx::query_as::<_, Prompt>(
            r#"INSERT INTO prompts (organization_id, user_id, title, content, tags, is_shared)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(title)
        .bind(content)
        .bind(tags)
        .bind(is_shared)
        .fetch_one(&self.pool)
        .await?;
        Ok(prompt)
    }

    /// Update an existing prompt (only owner can update).
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: Uuid,
        title: Option<&str>,
        content: Option<&str>,
        tags: Option<&[String]>,
        is_shared: Option<bool>,
    ) -> AppResult<Prompt> {
        let prompt = sqlx::query_as::<_, Prompt>(
            r#"UPDATE prompts SET
                title = COALESCE($4, title),
                content = COALESCE($5, content),
                tags = COALESCE($6, tags),
                is_shared = COALESCE($7, is_shared)
               WHERE id = $1 AND organization_id = $2 AND user_id = $3
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(title)
        .bind(content)
        .bind(tags)
        .bind(is_shared)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| -> AppError { ErrorKind::NotFound(format!("prompt {id}")).into() })?;
        Ok(prompt)
    }

    /// Delete a prompt (only owner can delete).
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let result = sqlx::query(
            r#"DELETE FROM prompts
               WHERE id = $1 AND organization_id = $2 AND user_id = $3"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ErrorKind::NotFound(format!("prompt {id}")).into());
        }
        Ok(())
    }
}
