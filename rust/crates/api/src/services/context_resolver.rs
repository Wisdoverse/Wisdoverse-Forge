//! Scoped read-side context resolver for task assignment previews.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agentforge_core::{AgentId, AppResult, CliToolKind, RuntimeCapability, RuntimeKind, ScopedRead};
use agentforge_infra::RedisClient;
use redis::AsyncCommands;
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::Uuid;

pub use crate::domain::context_resolver::{
    ContextItemKind, ContextSelection, ContextTaskSnapshot, DegradationReason, ResolvedContext, ResolvedItemRef,
    SelectedContext, apply_context_selection,
};
use crate::domain::context_resolver::{
    ContextResolverPolicy, MemoryCandidate, apply_budget, context_resolver_cache_key, push_degradation,
    skill_suggestion_item, task_search_text,
};
use crate::repositories::context_resolver::ContextResolverRepository;
use crate::services::runtime_capability_registry::RuntimeCapabilityRegistryService;

const ENVELOPE_VERSION: &str = "v1";
const MEMO_TTL: Duration = Duration::from_secs(60);
const CANDIDATE_LIMIT: i64 = 50;

#[derive(Debug, Clone, Copy)]
pub struct ResolveContextInput {
    pub task_id: Uuid,
    pub agent_id: AgentId,
}

#[derive(Clone)]
pub struct ContextResolverService {
    repo: ContextResolverRepository,
    runtime_registry: RuntimeCapabilityRegistryService,
    redis: Option<Arc<RwLock<RedisClient>>>,
    memo: Arc<RwLock<HashMap<String, MemoEntry>>>,
}

#[derive(Debug, Clone)]
struct MemoEntry {
    expires_at: Instant,
    resolved: ResolvedContext,
}

impl ContextResolverService {
    pub fn new(pool: PgPool, runtime_registry: RuntimeCapabilityRegistryService) -> Self {
        Self {
            repo: ContextResolverRepository::new(pool),
            runtime_registry,
            redis: None,
            memo: Arc::new(RwLock::new(HashMap::new())),
        }
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
        let cache_key = context_resolver_cache_key(snapshot.task_id, agent_id, proof);
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
        self.repo
            .task_snapshot(proof, task_id)
            .await?
            .ok_or_else(|| ContextResolverPolicy::task_not_found(task_id).into())
    }

    async fn capability_for_agent(
        &self,
        proof: &ScopedRead,
        agent_id: AgentId,
    ) -> AppResult<(RuntimeCapability, Vec<DegradationReason>)> {
        let row =
            self.repo.agent_runtime(proof, agent_id).await?.ok_or_else(|| -> agentforge_core::AppError {
                ContextResolverPolicy::agent_not_found(agent_id).into()
            })?;

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
            ContextResolverPolicy::unsupported_cli_tool(agent_id, err).into()
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
    ) -> AppResult<Vec<MemoryCandidate>> {
        self.repo.visible_memory_candidates(proof, search_text, CANDIDATE_LIMIT).await
    }

    async fn visible_skill_suggestions(
        &self,
        proof: &ScopedRead,
        search_text: &str,
    ) -> AppResult<Vec<ResolvedItemRef>> {
        let rows = self.repo.visible_skill_suggestions(proof, search_text, CANDIDATE_LIMIT).await?;
        Ok(rows.into_iter().map(skill_suggestion_item).collect())
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
