//! Builds runtime-neutral context envelopes for agent CLI adapters.

use std::collections::HashMap;
use std::sync::Arc;

use agentforge_core::context_envelope::{CONTEXT_ENVELOPE_VERSION_V1, ContextEnvelope, ContextEnvelopeItem};
use agentforge_core::{AgentId, AppResult, ScopedRead};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::context_envelope::{
    ContextEnvelopeCapabilityPolicy, ContextEnvelopeDegradationPolicy, ContextEnvelopeMemoryItem,
    ContextEnvelopeVersionPolicy,
};
use crate::domain::context_resolver::{ContextItemKind as ResolvedContextItemKind, ResolvedContext};
use crate::repositories::context_envelope::{ContextEnvelopeMemoryRecord, ContextEnvelopeRepository};
use crate::services::context_resolver::{ContextResolverService, ResolveContextInput};

#[derive(Debug, Clone)]
pub struct ContextEnvelopeInput {
    pub task_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: AgentId,
    pub supported_versions: Vec<String>,
}

#[derive(Clone)]
pub struct ContextEnvelopeService {
    repo: ContextEnvelopeRepository,
    resolver: Arc<ContextResolverService>,
}

impl ContextEnvelopeService {
    pub fn new(pool: PgPool, resolver: Arc<ContextResolverService>) -> Self {
        Self { repo: ContextEnvelopeRepository::new(pool), resolver }
    }

    pub fn from_runtime(pool: PgPool, resolver: Arc<ContextResolverService>) -> Self {
        Self::new(pool, resolver)
    }

    pub async fn build(&self, proof: &ScopedRead, input: ContextEnvelopeInput) -> AppResult<ContextEnvelope> {
        ContextEnvelopeVersionPolicy::ensure_v1_supported(&input.supported_versions)?;
        self.verify_run(proof, &input).await?;
        let resolved = self
            .resolver
            .resolve(proof, ResolveContextInput { task_id: input.task_id, agent_id: input.agent_id })
            .await?;
        self.build_from_resolved(proof, input.task_id, input.run_id, input.agent_id, resolved).await
    }

    pub async fn build_from_resolved(
        &self,
        proof: &ScopedRead,
        task_id: Uuid,
        run_id: Uuid,
        agent_id: AgentId,
        resolved: ResolvedContext,
    ) -> AppResult<ContextEnvelope> {
        let memory = self.load_applied_memory_content(proof, &resolved).await?;
        let applied = resolved
            .applied
            .iter()
            .filter_map(|item| match item.kind {
                ResolvedContextItemKind::Memory => memory.get(&item.id).map(envelope_item),
                ResolvedContextItemKind::Skill => None,
            })
            .collect();

        Ok(ContextEnvelope {
            envelope_version: CONTEXT_ENVELOPE_VERSION_V1.to_string(),
            run_id,
            task_id,
            agent_id: agent_id.as_uuid(),
            capability: ContextEnvelopeCapabilityPolicy::snapshot(&resolved.capability),
            applied,
            skills_mount: Vec::new(),
            degradation: ContextEnvelopeDegradationPolicy::labels(&resolved.degradation),
        })
    }

    async fn verify_run(&self, proof: &ScopedRead, input: &ContextEnvelopeInput) -> AppResult<()> {
        self.repo.verify_run(proof, input.run_id, input.task_id, input.agent_id).await
    }

    async fn load_applied_memory_content(
        &self,
        proof: &ScopedRead,
        resolved: &ResolvedContext,
    ) -> AppResult<HashMap<Uuid, ContextEnvelopeMemoryRecord>> {
        let ids: Vec<Uuid> = resolved
            .applied
            .iter()
            .filter(|item| item.kind == ResolvedContextItemKind::Memory)
            .map(|item| item.id)
            .collect();

        self.repo.applied_memory_content(proof, &ids).await
    }
}

fn envelope_item(row: &ContextEnvelopeMemoryRecord) -> ContextEnvelopeItem {
    ContextEnvelopeMemoryItem {
        id: row.id,
        title: &row.title,
        content: &row.content,
        content_redacted: row.content_redacted,
        content_encrypted: row.content_encrypted,
        sensitivity: &row.sensitivity,
    }
    .to_envelope_item()
}
