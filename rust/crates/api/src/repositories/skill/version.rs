//! Skill version repository — append-only skill snapshots for rollback.

use agentforge_core::{AppResult, OrgId, SkillId, UserId, WorkspaceId};
use agentforge_db::entities::{Skill, SkillVersion};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::domain::skill::SkillRepositoryPolicy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSnapshot {
    pub id: SkillId,
    pub organization_id: Option<OrgId>,
    pub workspace_id: Option<WorkspaceId>,
    pub scope_kind: Option<String>,
    pub scope_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub trigger_pattern: Option<String>,
    pub content: String,
    pub enabled: bool,
    pub state: String,
    pub version: i32,
    pub owner_user_id: Option<UserId>,
    pub ttl_expires_at: Option<DateTime<Utc>>,
    pub sensitivity: String,
    pub provenance: Value,
    pub negative_trigger: Option<String>,
    pub required_inputs: Value,
    pub tools: Value,
    pub examples: Value,
    pub success_evidence: Value,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Skill> for SkillSnapshot {
    fn from(skill: &Skill) -> Self {
        Self {
            id: skill.id,
            organization_id: skill.organization_id,
            workspace_id: skill.workspace_id,
            scope_kind: skill.scope_kind.clone(),
            scope_id: skill.scope_id,
            name: skill.name.clone(),
            description: skill.description.clone(),
            trigger_pattern: skill.trigger_pattern.clone(),
            content: skill.content.clone(),
            enabled: skill.enabled,
            state: skill.state.clone(),
            version: skill.version,
            owner_user_id: skill.owner_user_id,
            ttl_expires_at: skill.ttl_expires_at,
            sensitivity: skill.sensitivity.clone(),
            provenance: skill.provenance.clone(),
            negative_trigger: skill.negative_trigger.clone(),
            required_inputs: skill.required_inputs.clone(),
            tools: skill.tools.clone(),
            examples: skill.examples.clone(),
            success_evidence: skill.success_evidence.clone(),
            revoked_at: skill.revoked_at,
            created_at: skill.created_at,
            updated_at: skill.updated_at,
        }
    }
}

impl SkillSnapshot {
    pub fn from_value(value: Value) -> AppResult<Self> {
        serde_json::from_value(value).map_err(SkillRepositoryPolicy::snapshot_invalid)
    }

    fn to_value(&self) -> AppResult<Value> {
        serde_json::to_value(self).map_err(SkillRepositoryPolicy::snapshot_serialize_failed)
    }
}

pub struct SkillVersionRepository;

impl SkillVersionRepository {
    pub async fn insert_snapshot_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        skill: &Skill,
        author_user_id: UserId,
    ) -> AppResult<SkillVersion> {
        let snapshot = SkillSnapshot::from(skill).to_value()?;
        sqlx::query_as::<_, SkillVersion>(
            r#"INSERT INTO skill_versions (skill_id, version, snapshot, author_user_id)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (skill_id, version) DO NOTHING
               RETURNING *"#,
        )
        .bind(skill.id.as_uuid())
        .bind(skill.version)
        .bind(&snapshot)
        .bind(author_user_id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| SkillRepositoryPolicy::version_already_exists(skill.id, skill.version))
    }

    pub async fn list_by_skill_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        skill_id: SkillId,
    ) -> AppResult<Vec<SkillVersion>> {
        sqlx::query_as::<_, SkillVersion>(
            r#"SELECT *
                 FROM skill_versions
                WHERE skill_id = $1
                ORDER BY version DESC"#,
        )
        .bind(skill_id.as_uuid())
        .fetch_all(&mut **tx)
        .await
        .map_err(Into::into)
    }

    pub async fn snapshot_for_version_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        skill_id: SkillId,
        version: i32,
    ) -> AppResult<(SkillVersion, SkillSnapshot)> {
        let row = sqlx::query_as::<_, SkillVersion>(
            r#"SELECT *
                 FROM skill_versions
                WHERE skill_id = $1
                  AND version = $2"#,
        )
        .bind(skill_id.as_uuid())
        .bind(version)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| SkillRepositoryPolicy::version_not_found(skill_id, version))?;
        let snapshot = SkillSnapshot::from_value(row.snapshot.clone())?;
        Ok((row, snapshot))
    }
}
