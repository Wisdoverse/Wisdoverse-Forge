//! Scoped read-side context resolver for task assignment previews.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agentforge_core::{AgentId, AppResult, CliToolKind, ErrorKind, RuntimeCapability, RuntimeKind, ScopedRead};
use agentforge_infra::RedisClient;
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::services::runtime_capability_registry::RuntimeCapabilityRegistryService;

const ENVELOPE_VERSION: &str = "v1";
const MEMO_TTL: Duration = Duration::from_secs(60);
const CANDIDATE_LIMIT: i64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextItemKind {
    Memory,
    Skill,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradationReason {
    BudgetTruncated,
    RuntimeCapabilityFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedItemRef {
    pub id: Uuid,
    pub kind: ContextItemKind,
    pub title: String,
    pub scope_kind: Option<String>,
    pub scope_id: Option<Uuid>,
    pub sensitivity: Option<String>,
    pub estimated_tokens: u32,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedContext {
    pub applied: Vec<ResolvedItemRef>,
    pub suggested: Vec<ResolvedItemRef>,
    pub capability: RuntimeCapability,
    pub degradation: Vec<DegradationReason>,
    pub envelope_version: String,
}

#[derive(Debug, Clone, Default)]
pub struct ContextSelection {
    pub pinned_item_ids: Vec<Uuid>,
    pub removed_item_ids: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub struct SelectedContext {
    pub resolved: ResolvedContext,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct ResolveContextInput {
    pub task_id: Uuid,
    pub agent_id: AgentId,
}

#[derive(Debug, Clone)]
pub struct ContextTaskSnapshot {
    pub task_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub params: Option<serde_json::Value>,
}

impl ContextTaskSnapshot {
    pub fn from_task(task: &agentforge_db::entities::OrchestrationTask) -> Self {
        Self {
            task_id: task.id,
            title: task.title.clone(),
            description: task.description.clone(),
            params: task.params.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ContextResolverService {
    pool: PgPool,
    runtime_registry: RuntimeCapabilityRegistryService,
    redis: Option<Arc<RwLock<RedisClient>>>,
    memo: Arc<RwLock<HashMap<String, MemoEntry>>>,
}

#[derive(Debug, Clone)]
struct MemoEntry {
    expires_at: Instant,
    resolved: ResolvedContext,
}

#[derive(Debug, FromRow)]
struct TaskSnapshotRow {
    id: Uuid,
    title: String,
    description: Option<String>,
    params: Option<serde_json::Value>,
}

#[derive(Debug, FromRow)]
struct AgentRuntimeRow {
    cli_tool: Option<String>,
    provider: Option<String>,
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

#[derive(Debug, FromRow)]
struct SkillCandidateRow {
    id: Uuid,
    name: String,
    scope_kind: Option<String>,
    scope_id: Option<Uuid>,
    sensitivity: String,
}

impl ContextResolverService {
    pub fn new(pool: PgPool, runtime_registry: RuntimeCapabilityRegistryService) -> Self {
        Self { pool, runtime_registry, redis: None, memo: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub fn with_redis(mut self, redis: Arc<RwLock<RedisClient>>) -> Self {
        self.redis = Some(redis);
        self
    }

    pub async fn resolve(&self, proof: &ScopedRead, input: ResolveContextInput) -> AppResult<ResolvedContext> {
        let snapshot = self.load_task_snapshot(proof, input.task_id).await?;
        self.resolve_for_task_snapshot(proof, snapshot, input.agent_id).await
    }

    pub async fn resolve_for_task_snapshot(
        &self,
        proof: &ScopedRead,
        snapshot: ContextTaskSnapshot,
        agent_id: AgentId,
    ) -> AppResult<ResolvedContext> {
        let cache_key = memo_key(snapshot.task_id, agent_id, proof);
        if let Some(cached) = self.memo_get(&cache_key).await {
            return Ok(cached);
        }
        if let Some(cached) = self.redis_get(&cache_key).await {
            self.memo_set(cache_key.clone(), cached.clone()).await;
            return Ok(cached);
        }

        let search_text = task_search_text(&snapshot);
        let (capability, mut degradation) = self.capability_for_agent(proof, agent_id).await?;
        let memory_rows = self.visible_memory_candidates(proof, &search_text).await?;
        let (applied, mut suggested, was_truncated) = apply_budget(memory_rows, capability.max_context_tokens);
        if was_truncated {
            push_degradation(&mut degradation, DegradationReason::BudgetTruncated);
        }

        suggested.extend(self.visible_skill_suggestions(proof, &search_text).await?);
        let resolved = ResolvedContext {
            applied,
            suggested,
            capability,
            degradation,
            envelope_version: ENVELOPE_VERSION.to_string(),
        };

        self.memo_set(cache_key.clone(), resolved.clone()).await;
        self.redis_set(&cache_key, &resolved).await;
        Ok(resolved)
    }

    async fn load_task_snapshot(&self, proof: &ScopedRead, task_id: Uuid) -> AppResult<ContextTaskSnapshot> {
        let row = sqlx::query_as::<_, TaskSnapshotRow>(
            r#"SELECT id, title, description, params
                 FROM orchestration_tasks
                WHERE id = $1
                  AND organization_id = $2"#,
        )
        .bind(task_id)
        .bind(proof.org_id().as_uuid())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("orchestration task {task_id}")))?;

        Ok(ContextTaskSnapshot { task_id: row.id, title: row.title, description: row.description, params: row.params })
    }

    async fn capability_for_agent(
        &self,
        proof: &ScopedRead,
        agent_id: AgentId,
    ) -> AppResult<(RuntimeCapability, Vec<DegradationReason>)> {
        if proof.workspace_ids().is_empty() {
            return Err(ErrorKind::NotFound(format!("agent {}", agent_id.as_uuid())).into());
        }

        let workspace_ids: Vec<Uuid> = proof.workspace_ids().iter().map(|id| id.as_uuid()).collect();
        let row = sqlx::query_as::<_, AgentRuntimeRow>(
            r#"SELECT cli_tool, provider
                 FROM agents
                WHERE id = $1
                  AND organization_id = $2
                  AND workspace_id = ANY($3)"#,
        )
        .bind(agent_id.as_uuid())
        .bind(proof.org_id().as_uuid())
        .bind(workspace_ids)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ErrorKind::NotFound(format!("agent {}", agent_id.as_uuid())))?;

        let Some(cli_tool) = row.cli_tool else {
            return Ok((
                RuntimeCapability::api_provider_or_default(
                    row.provider.unwrap_or_else(|| "provider".to_string()),
                    4_096,
                ),
                Vec::new(),
            ));
        };

        let cli_tool = CliToolKind::parse_legacy(&cli_tool).map_err(|err| -> agentforge_core::AppError {
            ErrorKind::Internal(anyhow::anyhow!("agent {} has unsupported cli_tool: {err}", agent_id.as_uuid())).into()
        })?;
        let capability = self.runtime_registry.for_cli_tool(cli_tool, RuntimeKind::Container).await;
        let mut degradation = Vec::new();
        if capability == RuntimeCapability::fallback_for_cli_tool(cli_tool, RuntimeKind::Container) {
            degradation.push(DegradationReason::RuntimeCapabilityFallback);
        }
        Ok((capability, degradation))
    }

    async fn visible_memory_candidates(
        &self,
        proof: &ScopedRead,
        search_text: &str,
    ) -> AppResult<Vec<MemoryCandidateRow>> {
        if proof.workspace_ids().is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as::<_, MemoryCandidateRow>(
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
        .bind(CANDIDATE_LIMIT)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    async fn visible_skill_suggestions(
        &self,
        proof: &ScopedRead,
        search_text: &str,
    ) -> AppResult<Vec<ResolvedItemRef>> {
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
        .bind(CANDIDATE_LIMIT)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| ResolvedItemRef {
                id: row.id,
                kind: ContextItemKind::Skill,
                title: row.name,
                scope_kind: row.scope_kind,
                scope_id: row.scope_id,
                sensitivity: Some(row.sensitivity),
                estimated_tokens: 0,
                last_used_at: None,
                last_verified_at: None,
                why: "Skill trigger matched the task text.".to_string(),
            })
            .collect())
    }

    async fn memo_get(&self, key: &str) -> Option<ResolvedContext> {
        let mut memo = self.memo.write().await;
        let entry = memo.get(key)?;
        if Instant::now() >= entry.expires_at {
            memo.remove(key);
            return None;
        }
        Some(entry.resolved.clone())
    }

    async fn memo_set(&self, key: String, resolved: ResolvedContext) {
        self.memo.write().await.insert(key, MemoEntry { expires_at: Instant::now() + MEMO_TTL, resolved });
    }

    async fn redis_get(&self, key: &str) -> Option<ResolvedContext> {
        let Some(redis) = &self.redis else {
            return None;
        };
        let mut redis = redis.write().await;
        let conn = redis.connection_mut()?;
        let raw: Option<String> = match conn.get(key).await {
            Ok(raw) => raw,
            Err(err) => {
                tracing::warn!(error = %err, "context resolver Redis GET failed; using in-process memo fallback");
                metrics::counter!("context_resolver_memo_redis_error_total", "op" => "get").increment(1);
                return None;
            }
        };
        raw.and_then(|raw| serde_json::from_str(&raw).ok())
    }

    async fn redis_set(&self, key: &str, resolved: &ResolvedContext) {
        let Some(redis) = &self.redis else {
            return;
        };
        let Ok(payload) = serde_json::to_string(resolved) else {
            return;
        };
        let mut redis = redis.write().await;
        let Some(conn) = redis.connection_mut() else {
            return;
        };
        let result: redis::RedisResult<()> = conn.set_ex(key, payload, MEMO_TTL.as_secs()).await;
        if let Err(err) = result {
            tracing::warn!(error = %err, "context resolver Redis SETEX failed; using in-process memo fallback");
            metrics::counter!("context_resolver_memo_redis_error_total", "op" => "set").increment(1);
        }
    }
}

pub fn apply_context_selection(resolved: ResolvedContext, selection: &ContextSelection) -> SelectedContext {
    let ResolvedContext {
        applied: default_applied,
        suggested: default_suggested,
        capability,
        degradation,
        envelope_version,
    } = resolved;
    let mut warnings = Vec::new();
    let removed: std::collections::HashSet<Uuid> = selection.removed_item_ids.iter().copied().collect();
    let pinned: std::collections::HashSet<Uuid> = selection.pinned_item_ids.iter().copied().collect();
    let mut all_items: Vec<ResolvedItemRef> = default_applied.iter().chain(default_suggested.iter()).cloned().collect();
    all_items.sort_by_key(|item| (item.kind as u8, item.id));

    let mut by_id = std::collections::HashMap::new();
    for item in all_items {
        by_id.insert(item.id, item);
    }

    let mut applied = Vec::new();
    for id in &selection.pinned_item_ids {
        if removed.contains(id) {
            continue;
        }
        if let Some(item) = by_id.get(id) {
            if item.kind != ContextItemKind::Memory {
                warnings.push(format!("pinned item {id} is not injectable by the current envelope"));
                continue;
            }
            if default_suggested.iter().any(|candidate| candidate.id == *id) {
                warnings.push(format!("pinned item {id} was outside the default selection"));
            }
            applied.push(item.clone());
        }
    }

    for item in &default_applied {
        if removed.contains(&item.id) || pinned.contains(&item.id) {
            continue;
        }
        applied.push(item.clone());
    }

    let applied_ids: std::collections::HashSet<Uuid> = applied.iter().map(|item| item.id).collect();
    let suggested = default_suggested
        .into_iter()
        .chain(default_applied)
        .filter(|item| !applied_ids.contains(&item.id) && !removed.contains(&item.id))
        .collect();

    SelectedContext {
        resolved: ResolvedContext { applied, suggested, capability, degradation, envelope_version },
        warnings,
    }
}

fn apply_budget(
    rows: Vec<MemoryCandidateRow>,
    max_context_tokens: u32,
) -> (Vec<ResolvedItemRef>, Vec<ResolvedItemRef>, bool) {
    let mut used = 0_u32;
    let mut truncated = false;
    let mut applied = Vec::new();
    let mut suggested = Vec::new();

    for row in rows {
        let estimated_tokens = u32::try_from(row.estimated_tokens.max(1)).unwrap_or(u32::MAX);
        let item = ResolvedItemRef {
            id: row.id,
            kind: ContextItemKind::Memory,
            title: row.title,
            scope_kind: Some(row.scope_kind),
            scope_id: Some(row.scope_id),
            sensitivity: Some(row.sensitivity),
            estimated_tokens,
            last_used_at: row.last_used_at,
            last_verified_at: row.last_verified_at,
            why: memory_why(row.confidence, row.last_verified_at),
        };
        if used.saturating_add(estimated_tokens) <= max_context_tokens {
            used = used.saturating_add(estimated_tokens);
            applied.push(item);
        } else {
            truncated = true;
            suggested.push(item);
        }
    }

    (applied, suggested, truncated)
}

fn memory_why(confidence: Option<f64>, last_verified_at: Option<DateTime<Utc>>) -> String {
    match (confidence, last_verified_at) {
        (Some(confidence), Some(last_verified_at)) => {
            format!("Matched task text; confidence {:.2}; verified {}.", confidence, last_verified_at.to_rfc3339())
        }
        (Some(confidence), None) => format!("Matched task text; confidence {:.2}.", confidence),
        (None, Some(last_verified_at)) => format!("Matched task text; verified {}.", last_verified_at.to_rfc3339()),
        (None, None) => "Matched task text.".to_string(),
    }
}

fn memo_key(task_id: Uuid, agent_id: AgentId, proof: &ScopedRead) -> String {
    format!("context_resolver:{task_id}:{}:{}", agent_id.as_uuid(), scope_hash(proof))
}

fn scope_hash(proof: &ScopedRead) -> String {
    let mut workspaces: Vec<String> = proof.workspace_ids().iter().map(|id| id.as_uuid().to_string()).collect();
    let mut teams: Vec<String> = proof.team_ids().iter().map(|id| id.as_uuid().to_string()).collect();
    let mut projects: Vec<String> = proof.project_ids().iter().map(|id| id.as_uuid().to_string()).collect();
    workspaces.sort();
    teams.sort();
    projects.sort();
    let material = serde_json::json!({
        "org_id": proof.org_id().as_uuid(),
        "user_id": proof.user_id().as_uuid(),
        "workspace_ids": workspaces,
        "team_ids": teams,
        "project_ids": projects,
    });
    hex::encode(Sha256::digest(material.to_string().as_bytes()))
}

fn task_search_text(snapshot: &ContextTaskSnapshot) -> String {
    let mut parts = vec![snapshot.title.clone()];
    if let Some(description) = &snapshot.description {
        parts.push(description.clone());
    }
    if let Some(params) = &snapshot.params {
        for key in ["task", "message"] {
            if let Some(value) = params.get(key).and_then(|value| value.as_str()) {
                parts.push(value.to_string());
            }
        }
    }
    parts.join(" ")
}

fn push_degradation(items: &mut Vec<DegradationReason>, reason: DegradationReason) {
    if !items.contains(&reason) {
        items.push(reason);
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
