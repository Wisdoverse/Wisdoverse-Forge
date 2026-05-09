//! Organization repository — tenant-scoped database queries for organizations.

use agentforge_core::{AppResult, ErrorKind, OrgId, TenantScope, UserId};
use agentforge_db::entities::Organization;
use sqlx::PgPool;

/// Database access layer for organizations.
pub struct OrganizationRepository {
    pool: PgPool,
}

impl OrganizationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List organizations the user belongs to (via organization_members).
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<Organization>> {
        let orgs = sqlx::query_as::<_, Organization>(
            r#"SELECT o.* FROM organizations o
               JOIN organization_members om ON o.id = om.organization_id
               WHERE om.user_id = $1 AND o.deleted_at IS NULL
               ORDER BY o.created_at DESC"#,
        )
        .bind(scope.user_id().as_uuid())
        .fetch_all(&self.pool)
        .await?;
        Ok(orgs)
    }

    /// Get a single organization by ID (tenant-scoped).
    pub async fn find_by_id(&self, scope: &TenantScope, id: OrgId) -> AppResult<Organization> {
        sqlx::query_as::<_, Organization>(
            r#"SELECT o.* FROM organizations o
               JOIN organization_members om ON o.id = om.organization_id
               WHERE o.id = $1 AND om.user_id = $2 AND o.deleted_at IS NULL"#,
        )
        .bind(id.as_uuid())
        .bind(scope.user_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("organization {id}")).into())
    }

    /// Create a new organization and add the creator as owner.
    pub async fn create(&self, user_id: UserId, name: &str, slug: &str) -> AppResult<Organization> {
        let mut tx = self.pool.begin().await?;

        let org = sqlx::query_as::<_, Organization>(
            r#"INSERT INTO organizations (name, slug)
               VALUES ($1, $2)
               RETURNING *"#,
        )
        .bind(name)
        .bind(slug)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"INSERT INTO organization_members (organization_id, user_id, role)
               VALUES ($1, $2, 'owner')"#,
        )
        .bind(org.id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(org)
    }

    /// Update an organization's name (tenant-scoped via membership check).
    pub async fn update(&self, scope: &TenantScope, id: OrgId, name: &str) -> AppResult<Organization> {
        // Verify membership first
        let _ = self.find_by_id(scope, id).await?;

        sqlx::query_as::<_, Organization>(
            r#"UPDATE organizations SET name = $2, updated_at = NOW()
               WHERE id = $1 AND deleted_at IS NULL
               RETURNING *"#,
        )
        .bind(id.as_uuid())
        .bind(name)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("organization {id}")).into())
    }
}
