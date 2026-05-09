//! Skill repository — database queries for the governed skills table.

use agentforge_core::{AppResult, ErrorKind, ProjectId, ScopedRead, TeamId, TenantScope, WorkspaceId};
use agentforge_db::entities::Skill;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub struct CreateSkillRecord<'a> {
    pub workspace_id: WorkspaceId,
    pub scope_kind: &'a str,
    pub scope_id: Uuid,
    pub owner_user_id: Uuid,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub trigger_pattern: Option<&'a str>,
    pub negative_trigger: Option<&'a str>,
    pub content: &'a str,
    pub enabled: bool,
    pub state: &'a str,
    pub sensitivity: &'a str,
    pub provenance: &'a Value,
    pub required_inputs: &'a Value,
    pub tools: &'a Value,
    pub examples: &'a Value,
    pub success_evidence: &'a Value,
    pub ttl_expires_at: Option<DateTime<Utc>>,
}

pub struct UpdateSkillRecord<'a> {
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub trigger_pattern: Option<&'a str>,
    pub content: Option<&'a str>,
    pub enabled: Option<bool>,
    pub state: Option<&'a str>,
    pub sensitivity: Option<&'a str>,
}

/// Database access layer for skills.
pub struct SkillRepository {
    pool: PgPool,
}

impl SkillRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// List active visible skills. Candidate, deprecated, revoked, disabled,
    /// and expired rows are hidden from the active-skill surface.
    pub async fn list_visible(&self, proof: &ScopedRead) -> AppResult<Vec<Skill>> {
        let skills = sqlx::query_as::<_, Skill>(VISIBLE_SKILLS_QUERY)
            .bind(proof.org_id().as_uuid())
            .bind(workspace_ids(proof))
            .bind(proof.user_id().as_uuid())
            .bind(team_ids(proof))
            .bind(project_ids(proof))
            .fetch_all(&self.pool)
            .await?;
        Ok(skills)
    }

    /// Get an active visible skill by ID.
    pub async fn get_visible_by_id(&self, proof: &ScopedRead, id: Uuid) -> AppResult<Skill> {
        sqlx::query_as::<_, Skill>(VISIBLE_SKILL_BY_ID_QUERY)
            .bind(id)
            .bind(proof.org_id().as_uuid())
            .bind(workspace_ids(proof))
            .bind(proof.user_id().as_uuid())
            .bind(team_ids(proof))
            .bind(project_ids(proof))
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| ErrorKind::NotFound(format!("skill {id}")).into())
    }

    /// Lock an org-owned skill for mutation. Global skills are read-only from
    /// tenant routes.
    pub async fn lock_org_skill_for_update(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        id: Uuid,
    ) -> AppResult<Skill> {
        sqlx::query_as::<_, Skill>(
            r#"SELECT * FROM skills
               WHERE id = $1
                 AND organization_id = $2
                 AND workspace_id = $3
               FOR UPDATE"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.workspace_id().map(|id| id.as_uuid()))
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("skill {id}")).into())
    }

    pub async fn exists_outside_request_boundary(&self, scope: &TenantScope, id: Uuid) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1 FROM skills
                    WHERE id = $1
                      AND (
                          organization_id IS DISTINCT FROM $2
                          OR (
                              organization_id = $2
                              AND workspace_id IS DISTINCT FROM $3
                          )
                      )
               )"#,
        )
        .bind(id)
        .bind(scope.org_id().as_uuid())
        .bind(scope.workspace_id().map(|id| id.as_uuid()))
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Create a new governed skill in the request organization.
    pub async fn create_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        record: CreateSkillRecord<'_>,
    ) -> AppResult<Skill> {
        sqlx::query_as::<_, Skill>(
            r#"INSERT INTO skills (
                   organization_id, workspace_id, scope_kind, scope_id, owner_user_id,
                   name, description, trigger_pattern, negative_trigger, content,
                   enabled, state, sensitivity, provenance,
                   required_inputs, tools, examples, success_evidence, ttl_expires_at
               )
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
               RETURNING *"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(record.workspace_id.as_uuid())
        .bind(record.scope_kind)
        .bind(record.scope_id)
        .bind(record.owner_user_id)
        .bind(record.name)
        .bind(record.description)
        .bind(record.trigger_pattern)
        .bind(record.negative_trigger)
        .bind(record.content)
        .bind(record.enabled)
        .bind(record.state)
        .bind(record.sensitivity)
        .bind(record.provenance)
        .bind(record.required_inputs)
        .bind(record.tools)
        .bind(record.examples)
        .bind(record.success_evidence)
        .bind(record.ttl_expires_at)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn update_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        record: UpdateSkillRecord<'_>,
    ) -> AppResult<Skill> {
        sqlx::query_as::<_, Skill>(
            r#"UPDATE skills
               SET name = COALESCE($2, name),
                   description = COALESCE($3, description),
                   trigger_pattern = COALESCE($4, trigger_pattern),
                   content = COALESCE($5, content),
                   enabled = COALESCE($6, enabled),
                   state = COALESCE($7, state),
                   sensitivity = COALESCE($8, sensitivity),
                   version = version + 1,
                   updated_at = now()
               WHERE id = $1
               RETURNING *"#,
        )
        .bind(id)
        .bind(record.name)
        .bind(record.description)
        .bind(record.trigger_pattern)
        .bind(record.content)
        .bind(record.enabled)
        .bind(record.state)
        .bind(record.sensitivity)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn revoke_in_tx(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> AppResult<Skill> {
        sqlx::query_as::<_, Skill>(
            r#"UPDATE skills
               SET state = 'revoked',
                   enabled = FALSE,
                   revoked_at = now(),
                   version = version + 1,
                   updated_at = now()
               WHERE id = $1
                 AND state <> 'revoked'
               RETURNING *"#,
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| ErrorKind::Conflict(format!("skill {id} is already revoked")).into())
    }

    pub async fn restore_from_snapshot_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        snapshot: &crate::repositories::skill_version::SkillSnapshot,
        resulting_version: i32,
    ) -> AppResult<Skill> {
        sqlx::query_as::<_, Skill>(
            r#"UPDATE skills
               SET scope_kind = $2,
                   scope_id = $3,
                   owner_user_id = $4,
                   name = $5,
                   description = $6,
                   trigger_pattern = $7,
                   negative_trigger = $8,
                   content = $9,
                   enabled = $10,
                   state = $11,
                   version = $12,
                   ttl_expires_at = $13,
                   sensitivity = $14,
                   provenance = $15,
                   required_inputs = $16,
                   tools = $17,
                   examples = $18,
                   success_evidence = $19,
                   revoked_at = $20,
                   updated_at = now()
               WHERE id = $1
               RETURNING *"#,
        )
        .bind(id)
        .bind(snapshot.scope_kind.as_deref())
        .bind(snapshot.scope_id)
        .bind(snapshot.owner_user_id.map(|id| id.as_uuid()))
        .bind(&snapshot.name)
        .bind(snapshot.description.as_deref())
        .bind(snapshot.trigger_pattern.as_deref())
        .bind(snapshot.negative_trigger.as_deref())
        .bind(&snapshot.content)
        .bind(snapshot.enabled)
        .bind(&snapshot.state)
        .bind(resulting_version)
        .bind(snapshot.ttl_expires_at)
        .bind(&snapshot.sensitivity)
        .bind(&snapshot.provenance)
        .bind(&snapshot.required_inputs)
        .bind(&snapshot.tools)
        .bind(&snapshot.examples)
        .bind(&snapshot.success_evidence)
        .bind(snapshot.revoked_at)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn promote_candidate_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        scope_kind: &str,
        scope_id: Uuid,
        ttl_expires_at: Option<DateTime<Utc>>,
        sensitivity: &str,
    ) -> AppResult<Skill> {
        sqlx::query_as::<_, Skill>(
            r#"UPDATE skills
               SET scope_kind = $2,
                   scope_id = $3,
                   ttl_expires_at = $4,
                   sensitivity = $5,
                   enabled = TRUE,
                   state = 'active',
                   version = version + 1,
                   updated_at = now()
               WHERE id = $1
                 AND state = 'candidate'
                 AND revoked_at IS NULL
               RETURNING *"#,
        )
        .bind(id)
        .bind(scope_kind)
        .bind(scope_id)
        .bind(ttl_expires_at)
        .bind(sensitivity)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| ErrorKind::Conflict(format!("skill {id} is not a pending candidate")).into())
    }

    pub async fn resource_belongs_to_scope(
        &self,
        scope: &TenantScope,
        workspace_id: WorkspaceId,
        kind: &str,
        scope_id: Uuid,
    ) -> AppResult<bool> {
        if !self.workspace_belongs_to_scope(scope, workspace_id).await? {
            return Ok(false);
        }

        let exists = match kind {
            "org" => scope_id == scope.org_id().as_uuid(),
            "user" => scope_id == scope.user_id().as_uuid(),
            "team" => {
                sqlx::query_scalar::<_, bool>(
                    r#"SELECT EXISTS (
                           SELECT 1
                             FROM teams t
                            WHERE t.id = $3
                              AND t.organization_id = $1
                              AND t.deleted_at IS NULL
                              AND EXISTS (
                                  SELECT 1 FROM team_members tm
                                   WHERE tm.team_id = t.id AND tm.user_id = $2
                              )
                       )"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(scope.user_id().as_uuid())
                .bind(scope_id)
                .fetch_one(&self.pool)
                .await?
            }
            "project" => {
                sqlx::query_scalar::<_, bool>(
                    r#"SELECT EXISTS (
                           SELECT 1
                             FROM projects p
                            WHERE p.id = $3
                              AND p.organization_id = $1
                              AND p.deleted_at IS NULL
                              AND p.workspace_id = $4
                              AND (
                                  EXISTS (
                                      SELECT 1 FROM project_members pm
                                       WHERE pm.project_id = p.id AND pm.user_id = $2
                                  )
                                  OR EXISTS (
                                      SELECT 1 FROM team_members tm
                                       WHERE tm.team_id = p.team_id AND tm.user_id = $2
                                  )
                              )
                       )"#,
                )
                .bind(scope.org_id().as_uuid())
                .bind(scope.user_id().as_uuid())
                .bind(scope_id)
                .bind(workspace_id.as_uuid())
                .fetch_one(&self.pool)
                .await?
            }
            _ => false,
        };
        Ok(exists)
    }

    async fn workspace_belongs_to_scope(&self, scope: &TenantScope, workspace_id: WorkspaceId) -> AppResult<bool> {
        sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1 FROM workspaces
                    WHERE id = $1
                      AND organization_id = $2
                      AND deleted_at IS NULL
               )"#,
        )
        .bind(workspace_id.as_uuid())
        .bind(scope.org_id().as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }
}

const VISIBLE_SKILL_BY_ID_QUERY: &str = r#"SELECT * FROM skills
WHERE id = $1
  AND enabled = TRUE
  AND state = 'active'
  AND revoked_at IS NULL
  AND (ttl_expires_at IS NULL OR ttl_expires_at > now())
  AND (
      organization_id IS NULL
      OR (
          organization_id = $2
          AND workspace_id = ANY($3)
          AND (
              (scope_kind = 'org' AND scope_id = $2)
              OR (scope_kind = 'user' AND scope_id = $4)
              OR (scope_kind = 'team' AND scope_id = ANY($5))
              OR (scope_kind = 'project' AND scope_id = ANY($6))
          )
      )
  )"#;

const VISIBLE_SKILLS_QUERY: &str = r#"SELECT * FROM skills
WHERE enabled = TRUE
  AND state = 'active'
  AND revoked_at IS NULL
  AND (ttl_expires_at IS NULL OR ttl_expires_at > now())
  AND (
      organization_id IS NULL
      OR (
          organization_id = $1
          AND workspace_id = ANY($2)
          AND (
              (scope_kind = 'org' AND scope_id = $1)
              OR (scope_kind = 'user' AND scope_id = $3)
              OR (scope_kind = 'team' AND scope_id = ANY($4))
              OR (scope_kind = 'project' AND scope_id = ANY($5))
          )
      )
  )
ORDER BY name ASC, id ASC"#;

fn workspace_ids(proof: &ScopedRead) -> Vec<Uuid> {
    proof.workspace_ids().iter().map(|id: &WorkspaceId| id.as_uuid()).collect()
}

fn team_ids(proof: &ScopedRead) -> Vec<Uuid> {
    proof.team_ids().iter().map(|id: &TeamId| id.as_uuid()).collect()
}

fn project_ids(proof: &ScopedRead) -> Vec<Uuid> {
    proof.project_ids().iter().map(|id: &ProjectId| id.as_uuid()).collect()
}
