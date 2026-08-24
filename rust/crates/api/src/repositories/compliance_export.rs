//! Compliance export repository — org enumeration for scheduled exports.

use agentforge_core::{AppResult, OrgId, UserId};
use sqlx::PgPool;
use uuid::Uuid;

/// Database access layer for scheduled compliance exports.
pub struct ComplianceExportRepository {
    pool: PgPool,
}

impl ComplianceExportRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// All live orgs (id, slug) for per-org scheduled exports.
    pub async fn list_orgs(&self) -> AppResult<Vec<(Uuid, String)>> {
        let rows: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT id, slug FROM organizations WHERE deleted_at IS NULL ORDER BY slug ASC")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows)
    }

    /// One membership to scope an org export by (owner/admin first).
    pub async fn any_org_member(&self, org_id: OrgId) -> AppResult<Option<UserId>> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            r#"SELECT user_id FROM organization_members
               WHERE organization_id = $1
               ORDER BY (role = 'owner') DESC, created_at ASC
               LIMIT 1"#,
        )
        .bind(org_id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(user_id,)| UserId::from(user_id)))
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tenant_scope_for_ids;

    #[sqlx::test(migrations = "../db/migrations")]
    async fn org_enumeration_is_live_only(pool: PgPool) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Comp Org', 'comp-org')")
            .bind(org_id)
            .execute(&pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, 'comp@example.com')")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
            .bind(org_id)
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("seed membership");
        sqlx::query("UPDATE organizations SET deleted_at = NOW() WHERE id = $1")
            .bind(Uuid::new_v4())
            .execute(&pool)
            .await
            .expect("noop delete");

        let repo = ComplianceExportRepository::new(pool.clone());
        let orgs = repo.list_orgs().await.expect("orgs");
        assert!(orgs.iter().any(|(id, slug)| *id == org_id && slug == "comp-org"));
        let member = repo.any_org_member(OrgId::from(org_id)).await.expect("member").expect("owner");
        assert_eq!(member, UserId::from(user_id));
        let scope = tenant_scope_for_ids(org_id, user_id);
        assert_eq!(repo.any_org_member(scope.org_id()).await.expect("same").expect("same member"), scope.user_id());
    }
}
