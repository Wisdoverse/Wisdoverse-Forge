//! Context resolver read-model repository.

use agentforge_core::{AgentId, AppResult, ScopedRead};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::domain::context_resolver::{ContextTaskSnapshot, MemoryCandidate, SkillSuggestionCandidate};

#[derive(Debug, Clone)]
pub(crate) struct AgentRuntimeRecord {
    pub cli_tool: Option<String>,
    pub provider: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ContextResolverRepository {
    pool: PgPool,
}

#[derive(Debug, FromRow)]
struct TaskSnapshotRow {
    id: Uuid,
    title: String,
    description: Option<String>,
    params: Option<serde_json::Value>,
}

impl From<TaskSnapshotRow> for ContextTaskSnapshot {
    fn from(row: TaskSnapshotRow) -> Self {
        Self { task_id: row.id, title: row.title, description: row.description, params: row.params }
    }
}

#[derive(Debug, FromRow)]
struct AgentRuntimeRow {
    cli_tool: Option<String>,
    provider: Option<String>,
}

impl From<AgentRuntimeRow> for AgentRuntimeRecord {
    fn from(row: AgentRuntimeRow) -> Self {
        Self { cli_tool: row.cli_tool, provider: row.provider }
    }
}

#[derive(Debug, FromRow)]
struct MemoryCandidateRow {
    id: Uuid,
    title: String,
    scope_kind: String,
    scope_id: Uuid,
    sensitivity: String,
    estimated_tokens: i64,
    last_used_at: Option<DateTime<Utc>>,
    last_verified_at: Option<DateTime<Utc>>,
    confidence: Option<f64>,
}

impl From<MemoryCandidateRow> for MemoryCandidate {
    fn from(row: MemoryCandidateRow) -> Self {
        Self {
            id: row.id,
            title: row.title,
            scope_kind: row.scope_kind,
            scope_id: row.scope_id,
            sensitivity: row.sensitivity,
            estimated_tokens: row.estimated_tokens,
            last_used_at: row.last_used_at,
            last_verified_at: row.last_verified_at,
            confidence: row.confidence,
        }
    }
}

#[derive(Debug, FromRow)]
struct SkillCandidateRow {
    id: Uuid,
    name: String,
    scope_kind: Option<String>,
    scope_id: Option<Uuid>,
    sensitivity: String,
}

impl From<SkillCandidateRow> for SkillSuggestionCandidate {
    fn from(row: SkillCandidateRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            scope_kind: row.scope_kind,
            scope_id: row.scope_id,
            sensitivity: row.sensitivity,
        }
    }
}

impl ContextResolverRepository {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn task_snapshot(
        &self,
        proof: &ScopedRead,
        task_id: Uuid,
    ) -> AppResult<Option<ContextTaskSnapshot>> {
        let row = sqlx::query_as::<_, TaskSnapshotRow>(
            r#"SELECT id, title, description, params
                 FROM orchestration_tasks
                WHERE id = $1
                  AND organization_id = $2"#,
        )
        .bind(task_id)
        .bind(proof.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub(crate) async fn agent_runtime(
        &self,
        proof: &ScopedRead,
        agent_id: AgentId,
    ) -> AppResult<Option<AgentRuntimeRecord>> {
        if proof.workspace_ids().is_empty() {
            return Ok(None);
        }

        let row = sqlx::query_as::<_, AgentRuntimeRow>(
            r#"SELECT cli_tool, provider
                 FROM agents
                WHERE id = $1
                  AND organization_id = $2
                  AND workspace_id = ANY($3)"#,
        )
        .bind(agent_id.as_uuid())
        .bind(proof.org_id().as_uuid())
        .bind(workspace_ids(proof))
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub(crate) async fn visible_memory_candidates(
        &self,
        proof: &ScopedRead,
        search_text: &str,
        limit: i64,
    ) -> AppResult<Vec<MemoryCandidate>> {
        if proof.workspace_ids().is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, MemoryCandidateRow>(
            r#"WITH q AS (
                   SELECT plainto_tsquery('simple', $6) AS query
               ), ranked AS (
                   SELECT id,
                          title,
                          scope_kind,
                          scope_id,
                          sensitivity,
                          GREATEST(1, CEIL((char_length(title) + char_length(content))::numeric / 4.0)::bigint) AS estimated_tokens,
                          CASE
                              WHEN btrim($6) = '' THEN 0.0
                              ELSE ts_rank(to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(content, '')), (SELECT query FROM q))
                          END AS trigger_match_score,
                          last_used_at,
                          last_verified_at,
                          confidence
                     FROM memory_items
                    WHERE organization_id = $1
                      AND workspace_id = ANY($2)
                      AND revoked_at IS NULL
                      AND state = 'active'
                      AND (ttl_expires_at IS NULL OR ttl_expires_at > now())
                      AND (
                          (scope_kind = 'user' AND scope_id = $3)
                          OR (scope_kind = 'team' AND scope_id = ANY($4))
                          OR (scope_kind = 'project' AND scope_id = ANY($5))
                      )
                      AND (
                          btrim($6) = ''
                          OR to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(content, '')) @@ (SELECT query FROM q)
                          OR tsvector_to_array(to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(content, '')))
                             && tsvector_to_array(to_tsvector('simple', $6))
                      )
                    ORDER BY last_verified_at DESC NULLS LAST,
                             confidence DESC NULLS LAST,
                             trigger_match_score DESC,
                             id ASC
                    LIMIT $7
               )
               SELECT id, title, scope_kind, scope_id, sensitivity, estimated_tokens, last_used_at, last_verified_at, confidence
                 FROM ranked
                ORDER BY last_verified_at DESC NULLS LAST,
                         confidence DESC NULLS LAST,
                         trigger_match_score DESC,
                         id ASC"#,
        )
        .bind(proof.org_id().as_uuid())
        .bind(workspace_ids(proof))
        .bind(proof.user_id().as_uuid())
        .bind(team_ids(proof))
        .bind(project_ids(proof))
        .bind(search_text)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub(crate) async fn visible_skill_suggestions(
        &self,
        proof: &ScopedRead,
        search_text: &str,
        limit: i64,
    ) -> AppResult<Vec<SkillSuggestionCandidate>> {
        if proof.workspace_ids().is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, SkillCandidateRow>(
            r#"WITH q AS (
                   SELECT plainto_tsquery('simple', $6) AS query
               )
               SELECT id, name, scope_kind, scope_id, sensitivity
                 FROM skills
                WHERE enabled = TRUE
                  AND state = 'active'
                  AND revoked_at IS NULL
                  AND trigger_pattern IS NOT NULL
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
                  AND (
                      position(lower(trigger_pattern) in lower($6)) > 0
                      OR to_tsvector(
                          'simple',
                          coalesce(name, '') || ' ' || coalesce(description, '') || ' ' || coalesce(trigger_pattern, '')
                      ) @@ (SELECT query FROM q)
                  )
                ORDER BY name ASC, id ASC
                LIMIT $7"#,
        )
        .bind(proof.org_id().as_uuid())
        .bind(workspace_ids(proof))
        .bind(proof.user_id().as_uuid())
        .bind(team_ids(proof))
        .bind(project_ids(proof))
        .bind(search_text)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }
}

fn workspace_ids(proof: &ScopedRead) -> Vec<Uuid> {
    proof.workspace_ids().iter().map(|id| id.as_uuid()).collect()
}

fn team_ids(proof: &ScopedRead) -> Vec<Uuid> {
    proof.team_ids().iter().map(|id| id.as_uuid()).collect()
}

fn project_ids(proof: &ScopedRead) -> Vec<Uuid> {
    proof.project_ids().iter().map(|id| id.as_uuid()).collect()
}
