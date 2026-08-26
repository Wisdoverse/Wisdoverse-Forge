//! Skill usage repository — how often a governed skill was actually applied
//! inside task runs (immutable context-injection facts).

use agentforge_core::{AppResult, TenantScope};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// Aggregate usage facts for one skill: injections inside task runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct SkillUsageRow {
    pub injection_count: i64,
    pub run_count: i64,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Database access layer for skill usage counting.
pub struct SkillUsageRepository {
    pool: PgPool,
}

impl SkillUsageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Usage for one skill id: how many times it was injected and across how
    /// many distinct task runs (tenant-scoped). Rows survive skill rename/
    /// hard-delete because injection facts are immutable provenance.
    pub async fn for_skill(&self, scope: &TenantScope, skill_id: Uuid) -> AppResult<SkillUsageRow> {
        let row = sqlx::query_as::<_, SkillUsageRow>(
            r#"SELECT COUNT(*)::bigint AS injection_count,
                      COUNT(DISTINCT run_id)::bigint AS run_count,
                      MAX(applied_at) AS last_used_at
                 FROM run_context_injections
                WHERE organization_id = $1
                  AND item_kind = 'skill'
                  AND item_id = $2"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(skill_id)
        .fetch_one(self.pool())
        .await?;
        Ok(row)
    }
}

#[cfg(test)]
mod skill_usage_tests {
    use super::*;
    use crate::test_support::tenant_scope_for_ids;
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn seed_injections(pool: &PgPool, org_id: Uuid, user_id: Uuid, skill_id: Uuid) {
        // Org + workspace + task + agent + run are all FK prerequisites.
        sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Usage Org', $2)")
            .bind(org_id)
            .bind(format!("usage-org-{org_id}"))
            .execute(pool)
            .await
            .expect("seed org");
        sqlx::query("INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $1, 'Default')")
            .bind(org_id)
            .execute(pool)
            .await
            .expect("seed workspace");
        sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, 'usage@example.com', 'Dev')")
            .bind(user_id)
            .execute(pool)
            .await
            .expect("seed user");
        let agent_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, user_id, name)
                     VALUES ($1, $2, $2, $3, 'Agent')",
        )
        .bind(agent_id)
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed agent");
        let task_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO orchestration_tasks (id, organization_id, title, status, priority, created_by)
             VALUES ($1, $2, 'Usage task', 'working', 'normal', $3)",
        )
        .bind(task_id)
        .bind(org_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("seed task");
        let run_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO task_runs (id, organization_id, workspace_id, orchestration_task_id, agent_id,
                                    idempotency_key, status, capability_profile, started_at)
             VALUES ($1, $2, $2, $3, $4,
                     $5, 'working', '{}'::jsonb, now())",
        )
        .bind(run_id)
        .bind(org_id)
        .bind(task_id)
        .bind(agent_id)
        .bind(format!("idem-{run_id}"))
        .execute(pool)
        .await
        .expect("seed run");
        // Second run for the same task (different idempotency key): a skill can
        // appear once per run, so two runs = injection_count 2 / run_count 2.
        let run2_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO task_runs (id, organization_id, workspace_id, orchestration_task_id, agent_id,
                                    idempotency_key, status, capability_profile, started_at)
             VALUES ($1, $2, $2, $3, $4, $5, 'working', '{}'::jsonb, now())",
        )
        .bind(run2_id)
        .bind(org_id)
        .bind(task_id)
        .bind(agent_id)
        .bind(format!("idem-{run2_id}"))
        .execute(pool)
        .await
        .expect("seed second run");
        sqlx::query(
            "INSERT INTO run_context_injections (id, organization_id, workspace_id, run_id, item_id,
                                                 item_kind, position, adapter, envelope_version,
                                                 capability_profile, applied_snapshot, applied_at)
             VALUES (gen_random_uuid(), $1, $1, $2, $4, 'skill', 0, 'test', 'v1', '{}'::jsonb, '{}'::jsonb, now()),
                    (gen_random_uuid(), $1, $1, $3, $4, 'skill', 0, 'test', 'v1', '{}'::jsonb, '{}'::jsonb, now() - interval '1 day'),
                    (gen_random_uuid(), $1, $1, $2, $5, 'memory', 1, 'test', 'v1', '{}'::jsonb, '{}'::jsonb, now())",
        )
        .bind(org_id)
        .bind(run_id)
        .bind(run2_id)
        .bind(skill_id)
        .bind(Uuid::new_v4())
        .execute(pool)
        .await
        .expect("seed injections");
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn counts_only_skill_kind_for_this_skill(pool: PgPool) {
        let org_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let skill_id = Uuid::new_v4();
        seed_injections(&pool, org_id, user_id, skill_id).await;

        let repo = SkillUsageRepository::new(pool.clone());
        let usage = repo.for_skill(&tenant_scope_for_ids(org_id, user_id), skill_id).await.expect("usage");
        assert_eq!(usage.injection_count, 2, "memory rows must not count");
        assert_eq!(usage.run_count, 2, "one injection per run across two runs");
        assert!(usage.last_used_at.is_some());
    }
}
