//! Skill agent link repository — explicit skill-to-agent attachments,
//! tenant-scoped.

use agentforge_core::{AppResult, TenantScope};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// One skill followed by an agent (agent-side attach-back view).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct FollowedSkillRow {
    pub skill_id: Uuid,
    pub name: String,
    pub state: String,
    pub enabled: bool,
}

/// One attached agent projected for the UI (id + name at attach time).
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LinkedAgentRow {
    pub agent_id: Uuid,
    pub agent_name: Option<String>,
    pub attached_at: DateTime<Utc>,
}

/// Database access layer for skill agent links.
pub struct SkillAgentLinkRepository {
    pool: PgPool,
}

impl SkillAgentLinkRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Agents currently attached to a skill, oldest first.
    pub async fn list_agents(&self, scope: &TenantScope, skill_id: Uuid) -> AppResult<Vec<LinkedAgentRow>> {
        let rows = sqlx::query_as::<_, LinkedAgentRow>(
            r#"SELECT l.agent_id, a.name AS agent_name, l.created_at AS attached_at
                 FROM skill_agent_links l
                 LEFT JOIN agents a ON a.id = l.agent_id
                WHERE l.skill_id = $1 AND l.organization_id = $2
                ORDER BY l.created_at ASC, l.agent_id ASC"#,
        )
        .bind(skill_id)
        .bind(scope.org_id().as_uuid())
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Attach idempotently. The INSERT only lands when BOTH the skill and the
    /// agent belong to this organization; returns `None` otherwise. The
    /// conflict path is an upsert so a re-attach still returns the link.
    pub async fn attach(
        &self,
        scope: &TenantScope,
        skill_id: Uuid,
        agent_id: Uuid,
        attached_by: Option<Uuid>,
    ) -> AppResult<Option<LinkedAgentRow>> {
        let row = sqlx::query_as::<_, LinkedAgentRow>(
            r#"INSERT INTO skill_agent_links (id, organization_id, skill_id, agent_id, attached_by)
               SELECT $1, $2, $3, $4, $5
                 WHERE EXISTS (SELECT 1 FROM skills s WHERE s.id = $3 AND s.organization_id = $2)
                   AND EXISTS (SELECT 1 FROM agents a WHERE a.id = $4 AND a.organization_id = $2)
               ON CONFLICT (skill_id, agent_id) DO UPDATE
                     SET attached_by = COALESCE($5, skill_agent_links.attached_by)
               RETURNING $4 AS agent_id,
                         (SELECT name FROM agents WHERE id = $4) AS agent_name,
                         created_at AS attached_at"#,
        )
        .bind(Uuid::now_v7())
        .bind(scope.org_id().as_uuid())
        .bind(skill_id)
        .bind(agent_id)
        .bind(attached_by)
        .fetch_optional(self.pool())
        .await?;
        Ok(row)
    }

    /// Detach a skill from an agent. Returns `false` when the link did not
    /// exist (or belongs to another organization).
    pub async fn detach(&self, scope: &TenantScope, skill_id: Uuid, agent_id: Uuid) -> AppResult<bool> {
        let result = sqlx::query_as::<_, (Uuid,)>(
            r#"DELETE FROM skill_agent_links
               WHERE skill_id = $1 AND agent_id = $2 AND organization_id = $3
               RETURNING id"#,
        )
        .bind(skill_id)
        .bind(agent_id)
        .bind(scope.org_id().as_uuid())
        .fetch_optional(self.pool())
        .await?;
        Ok(result.is_some())
    }

    /// Skill ids attached to an agent (used to preference context for that agent).
    pub async fn list_skill_ids_for_agent(&self, scope: &TenantScope, agent_id: Uuid) -> AppResult<Vec<Uuid>> {
        let rows = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT skill_id
                 FROM skill_agent_links
                WHERE agent_id = $1 AND organization_id = $2
                ORDER BY created_at ASC"#,
        )
        .bind(agent_id)
        .bind(scope.org_id().as_uuid())
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Skills an agent follows, with their governance state. Links join the
    /// skill rows so renaming a skill is reflected here immediately.
    pub async fn list_skills_for_agent(&self, scope: &TenantScope, agent_id: Uuid) -> AppResult<Vec<FollowedSkillRow>> {
        let rows = sqlx::query_as::<_, FollowedSkillRow>(
            r#"SELECT s.id AS skill_id, s.name, s.state, s.enabled
                 FROM skill_agent_links l
                 JOIN skills s ON s.id = l.skill_id
                WHERE l.agent_id = $1 AND l.organization_id = $2
                ORDER BY l.created_at ASC, l.agent_id ASC"#,
        )
        .bind(agent_id)
        .bind(scope.org_id().as_uuid())
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod skill_agent_link_tests {
    use super::*;
    use crate::test_support::tenant_scope_for_ids;
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn seed(pool: &PgPool, org_id: Uuid, user_id: Uuid, skill_id: Uuid, agent_id: Uuid) {
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Link Org', $2)")
            .bind(org_id)
            .bind(format!("link-org-{org_id}"))
            .execute(pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, 'linker@example.com')")
            .bind(user_id)
            .execute(pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_id)
            .execute(pool)
            .await
            .expect("seed workspace");
        sqlx::query(
            "INSERT INTO skills (id, organization_id, workspace_id, scope_kind, scope_id, name, content, state, sensitivity)
             VALUES ($1, $2, $2, 'org', $2, 'Review', 'Do it', 'active', 'internal')",
        )
        .bind(skill_id)
        .bind(org_id)
        .execute(pool)
        .await
        .expect("seed skill");
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, user_id, name)
             VALUES ($1, $2, $2, $3, 'Build Agent')",
        )
        .bind(agent_id)
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed agent");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn attach_list_detach_are_tenant_scoped(pool: PgPool) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let skill_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let other_org_id = Uuid::new_v4();
        seed(&pool, org_id, user_id, skill_id, agent_id).await;

        let scope = tenant_scope_for_ids(org_id, user_id);
        let other_scope = tenant_scope_for_ids(other_org_id, user_id);
        let repo = SkillAgentLinkRepository::new(pool.clone());

        let linked = repo
            .attach(&scope, skill_id, agent_id, Some(user_id))
            .await
            .expect("attach")
            .expect("skill and agent in org");
        assert_eq!(linked.agent_id, agent_id);
        assert_eq!(linked.agent_name.as_deref(), Some("Build Agent"));

        // Idempotent re-attach.
        assert!(repo.attach(&scope, skill_id, agent_id, None).await.expect("reattach").is_some());

        let agents = repo.list_agents(&scope, skill_id).await.expect("list");
        assert_eq!(agents.len(), 1);

        // Other org cannot attach or see anything.
        assert!(
            repo.attach(&other_scope, skill_id, agent_id, None).await.expect("cross attach").is_none(),
            "cross-tenant attach must be a no-op"
        );
        assert!(repo.list_agents(&other_scope, skill_id).await.expect("cross list").is_empty());

        assert!(repo.detach(&scope, skill_id, agent_id).await.expect("detach"));
        assert!(!repo.detach(&scope, skill_id, agent_id).await.expect("detach again"));
        assert!(repo.list_agents(&scope, skill_id).await.expect("list after").is_empty());
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn list_skills_for_agent_returns_linked_skills_with_state(pool: PgPool) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let active_skill = Uuid::new_v4();
        let revoked_skill = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        seed(&pool, org_id, user_id, active_skill, agent_id).await;
        sqlx::query(
            "INSERT INTO skills (id, organization_id, workspace_id, scope_kind, scope_id, name, content, state, sensitivity, revoked_at, enabled)
             VALUES ($1, $2, $2, 'org', $2, 'Old guidance', 'x', 'revoked', 'internal', now(), FALSE)",
        )
        .bind(revoked_skill)
        .bind(org_id)
        .execute(&pool)
        .await
        .expect("seed revoked skill");

        let scope = tenant_scope_for_ids(org_id, user_id);
        let repo = SkillAgentLinkRepository::new(pool.clone());
        repo.attach(&scope, active_skill, agent_id, None).await.expect("attach active");
        repo.attach(&scope, revoked_skill, agent_id, None).await.expect("attach revoked");

        let followed = repo.list_skills_for_agent(&scope, agent_id).await.expect("list");
        assert_eq!(followed.len(), 2, "revoked skills still show who followed them");
        let active = followed.iter().find(|row| row.skill_id == active_skill).expect("active row");
        assert_eq!(active.state, "active");
        assert!(active.enabled);
        let revoked = followed.iter().find(|row| row.skill_id == revoked_skill).expect("revoked row");
        assert_eq!(revoked.state, "revoked");
        assert!(!revoked.enabled);

        let cross =
            repo.list_skills_for_agent(&tenant_scope_for_ids(Uuid::new_v4(), user_id), agent_id).await.expect("cross");
        assert!(cross.is_empty(), "cross-tenant views are empty");
    }
}
