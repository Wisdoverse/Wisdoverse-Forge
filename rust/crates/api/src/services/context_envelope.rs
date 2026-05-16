//! Builds runtime-neutral context envelopes for agent CLI adapters.

use std::collections::HashMap;
use std::sync::Arc;

use agentforge_core::context_envelope::{
    CONTEXT_ENVELOPE_VERSION_V1, ContextEnvelope, ContextEnvelopeItem, ContextEnvelopeItemKind, ContextEnvelopeSource,
};
use agentforge_core::{AgentId, AppResult, ErrorKind, ScopedRead};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::domain::context_envelope::{
    ContextEnvelopeCapabilityPolicy, ContextEnvelopeMemoryContentPolicy, ContextEnvelopeVersionPolicy,
};
use crate::domain::context_resolver::{ContextItemKind as ResolvedContextItemKind, DegradationReason, ResolvedContext};
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
    pool: PgPool,
    resolver: Arc<ContextResolverService>,
}

#[derive(Debug, FromRow)]
struct MemoryContentRow {
    id: Uuid,
    title: String,
    content: String,
    content_redacted: bool,
    content_encrypted: bool,
    sensitivity: String,
}

impl ContextEnvelopeService {
    pub fn new(pool: PgPool, resolver: Arc<ContextResolverService>) -> Self {
        Self { pool, resolver }
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
                ResolvedContextItemKind::Memory => memory.get(&item.id).map(|row| {
                    let content = ContextEnvelopeMemoryContentPolicy::visible_content(
                        &row.content,
                        row.content_redacted,
                        row.content_encrypted,
                        &row.sensitivity,
                    );
                    ContextEnvelopeItem {
                        id: row.id,
                        kind: ContextEnvelopeItemKind::Memory,
                        title: row.title.clone(),
                        content,
                        content_ref: format!("memory_items/{}", row.id),
                        sensitivity: row.sensitivity.clone(),
                        source: ContextEnvelopeSource {
                            source_type: "memory_item".to_string(),
                            source_id: Some(row.id),
                            title: Some(row.title.clone()),
                        },
                    }
                }),
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
            degradation: resolved.degradation.iter().map(degradation_label).map(str::to_string).collect(),
        })
    }

    async fn verify_run(&self, proof: &ScopedRead, input: &ContextEnvelopeInput) -> AppResult<()> {
        if proof.workspace_ids().is_empty() {
            return Err(ErrorKind::NotFound(format!("task run {}", input.run_id)).into());
        }
        let workspace_ids: Vec<Uuid> = proof.workspace_ids().iter().map(|id| id.as_uuid()).collect();
        let found = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id
                 FROM task_runs
                WHERE id = $1
                  AND organization_id = $2
                  AND workspace_id = ANY($3)
                  AND orchestration_task_id = $4
                  AND agent_id = $5"#,
        )
        .bind(input.run_id)
        .bind(proof.org_id().as_uuid())
        .bind(workspace_ids)
        .bind(input.task_id)
        .bind(input.agent_id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;

        found.map(|_| ()).ok_or_else(|| ErrorKind::NotFound(format!("task run {}", input.run_id)).into())
    }

    async fn load_applied_memory_content(
        &self,
        proof: &ScopedRead,
        resolved: &ResolvedContext,
    ) -> AppResult<HashMap<Uuid, MemoryContentRow>> {
        let ids: Vec<Uuid> = resolved
            .applied
            .iter()
            .filter(|item| item.kind == ResolvedContextItemKind::Memory)
            .map(|item| item.id)
            .collect();
        if ids.is_empty() || proof.workspace_ids().is_empty() {
            return Ok(HashMap::new());
        }

        let workspace_ids: Vec<Uuid> = proof.workspace_ids().iter().map(|id| id.as_uuid()).collect();
        let team_ids: Vec<Uuid> = proof.team_ids().iter().map(|id| id.as_uuid()).collect();
        let project_ids: Vec<Uuid> = proof.project_ids().iter().map(|id| id.as_uuid()).collect();
        let rows = sqlx::query_as::<_, MemoryContentRow>(
            r#"SELECT id, title, content, content_redacted, content_encrypted, sensitivity
                 FROM memory_items
                WHERE id = ANY($1)
                  AND organization_id = $2
                  AND workspace_id = ANY($3)
                  AND revoked_at IS NULL
                  AND state = 'active'
                  AND (ttl_expires_at IS NULL OR ttl_expires_at > now())
                  AND (
                      (scope_kind = 'user' AND scope_id = $4)
                      OR (scope_kind = 'team' AND scope_id = ANY($5))
                      OR (scope_kind = 'project' AND scope_id = ANY($6))
                  )"#,
        )
        .bind(ids)
        .bind(proof.org_id().as_uuid())
        .bind(workspace_ids)
        .bind(proof.user_id().as_uuid())
        .bind(team_ids)
        .bind(project_ids)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.id, row)).collect())
    }
}

fn degradation_label(reason: &DegradationReason) -> &'static str {
    reason.label()
}
