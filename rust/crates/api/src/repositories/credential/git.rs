//! Git credential repository — database queries for the git_credentials table.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::GitCredential;
use sqlx::PgPool;
use uuid::Uuid;

/// Database access layer for git credentials.
pub struct GitCredentialRepository {
    pool: PgPool,
}

impl GitCredentialRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new git credential (tenant-scoped).
    pub async fn create(
        &self,
        scope: &TenantScope,
        name: &str,
        provider: &str,
        credential_type: &str,
        remote_url: Option<&str>,
        token_encrypted: Option<&[u8]>,
        token_nonce: Option<&[u8]>,
    ) -> AppResult<GitCredential> {
        sqlx::query_as::<_, GitCredential>(
            r#"INSERT INTO git_credentials
               (organization_id, user_id, name, provider, credential_type, remote_url, token_encrypted, token_nonce)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(name)
        .bind(provider)
        .bind(credential_type)
        .bind(remote_url)
        .bind(token_encrypted)
        .bind(token_nonce)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Update the most recent credential for a provider, or create it if none exists.
    pub async fn upsert_for_provider(
        &self,
        scope: &TenantScope,
        name: &str,
        provider: &str,
        credential_type: &str,
        remote_url: Option<&str>,
        token_encrypted: Option<&[u8]>,
        token_nonce: Option<&[u8]>,
    ) -> AppResult<GitCredential> {
        if let Some(cred) = sqlx::query_as::<_, GitCredential>(
            r#"WITH target AS (
                   SELECT id
                   FROM git_credentials
                   WHERE organization_id = $1 AND user_id = $2 AND provider = $4
                   ORDER BY updated_at DESC, created_at DESC, id DESC
                   LIMIT 1
               )
               UPDATE git_credentials
               SET name = $3,
                   credential_type = $5,
                   remote_url = $6,
                   token_encrypted = COALESCE($7, token_encrypted),
                   token_nonce = CASE WHEN $7 IS NULL THEN token_nonce ELSE $8 END,
                   updated_at = now()
               WHERE id = (SELECT id FROM target)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(name)
        .bind(provider)
        .bind(credential_type)
        .bind(remote_url)
        .bind(token_encrypted)
        .bind(token_nonce)
        .fetch_optional(&self.pool)
        .await?
        {
            return Ok(cred);
        }

        self.create(scope, name, provider, credential_type, remote_url, token_encrypted, token_nonce).await
    }

    /// List git credentials for the tenant user (paginated).
    pub async fn list(&self, scope: &TenantScope, limit: i64, offset: i64) -> AppResult<Vec<GitCredential>> {
        let creds = sqlx::query_as::<_, GitCredential>(
            r#"SELECT * FROM git_credentials
               WHERE organization_id = $1 AND user_id = $2
               ORDER BY created_at DESC
               LIMIT $3 OFFSET $4"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(creds)
    }

    /// Latest token credential per Git platform CLI provider.
    pub async fn latest_cli_tokens(&self, scope: &TenantScope) -> AppResult<Vec<GitCredential>> {
        let creds = sqlx::query_as::<_, GitCredential>(
            r#"SELECT DISTINCT ON (provider) *
               FROM git_credentials
               WHERE organization_id = $1
                 AND user_id = $2
                 AND provider IN ('github', 'gitlab')
                 AND credential_type = 'token'
                 AND token_encrypted IS NOT NULL
               ORDER BY provider, updated_at DESC, created_at DESC, id DESC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(creds)
    }

    /// Find a git credential by ID (tenant-scoped).
    pub async fn find_by_id(&self, scope: &TenantScope, id: Uuid) -> AppResult<GitCredential> {
        sqlx::query_as::<_, GitCredential>(
            r#"SELECT * FROM git_credentials
               WHERE id = $1 AND organization_id = $2 AND user_id = $3"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("git credential {id}")).into())
    }

    /// Delete a git credential (tenant-scoped).
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        let result = sqlx::query(
            r#"DELETE FROM git_credentials
               WHERE id = $1 AND organization_id = $2 AND user_id = $3"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(ErrorKind::NotFound(format!("git credential {id}")).into());
        }
        Ok(())
    }
}
