//! Immutable context injection repository.

use std::collections::HashMap;

use agentforge_core::context_envelope::{ContextEnvelope, ContextEnvelopeItemKind};
use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::{RunContextInjection, TaskRun};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::orchestration::OrchestrationRepositoryPolicy;

#[derive(Debug, Clone, FromRow)]
pub struct ContextAppliedRunRow {
    pub injection_id: Uuid,
    pub run_id: Uuid,
    pub run_status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub adapter: String,
    pub envelope_version: String,
    pub degradation_reason: Option<String>,
    pub applied_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContextInjectionCounts {
    pub applied_memories: i64,
    pub applied_skills: i64,
}

#[derive(Debug, FromRow)]
struct ContextInjectionCountRow {
    task_id: Uuid,
    item_kind: String,
    count: i64,
}

pub struct RunContextInjectionRepository {
    pool: PgPool,
}

impl RunContextInjectionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn record_envelope_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        scope: &TenantScope,
        run: &TaskRun,
        envelope: &ContextEnvelope,
    ) -> AppResult<Vec<RunContextInjection>> {
        require_run_scope(scope, run)?;
        let adapter = envelope.capability.cli_tool.as_str();
        let capability_profile = serde_json::to_value(&envelope.capability)
            .map_err(OrchestrationRepositoryPolicy::context_injection_capability_profile_serialize)?;
        let degradation_reason = applied_degradation_reason(envelope);
        let mut rows = Vec::with_capacity(envelope.applied.len());

        for (position, item) in envelope.applied.iter().enumerate() {
            let position =
                i32::try_from(position).map_err(OrchestrationRepositoryPolicy::context_injection_position_overflow)?;
            let item_kind = item_kind_label(item.kind);
            let applied_snapshot = serde_json::to_value(item)
                .map_err(OrchestrationRepositoryPolicy::context_injection_applied_snapshot_serialize)?;
            let row = sqlx::query_as::<_, RunContextInjection>(
                r#"WITH inserted AS (
                       INSERT INTO run_context_injections (
                           organization_id, workspace_id, run_id, item_id, item_kind,
                           position, adapter, envelope_version, capability_profile, applied_snapshot,
                           degradation_reason
                       )
                       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                       ON CONFLICT ON CONSTRAINT run_context_injections_unique_item DO NOTHING
                       RETURNING *
                   )
                   SELECT * FROM inserted
                   UNION ALL
                   SELECT *
                     FROM run_context_injections
                    WHERE run_id = $3
                      AND item_id = $4
                      AND item_kind = $5
                    LIMIT 1"#,
            )
            .bind(run.organization_id.as_uuid())
            .bind(run.workspace_id.as_uuid())
            .bind(run.id)
            .bind(item.id)
            .bind(item_kind)
            .bind(position)
            .bind(adapter)
            .bind(&envelope.envelope_version)
            .bind(&capability_profile)
            .bind(&applied_snapshot)
            .bind(&degradation_reason)
            .fetch_one(&mut **tx)
            .await?;
            rows.push(row);
        }

        Ok(rows)
    }

    pub async fn list_by_run(&self, scope: &TenantScope, run_id: Uuid) -> AppResult<Vec<RunContextInjection>> {
        let workspace_id = require_workspace(scope)?;
        let rows = sqlx::query_as::<_, RunContextInjection>(
            r#"SELECT *
                 FROM run_context_injections
                WHERE organization_id = $1
                  AND workspace_id = $2
                  AND run_id = $3
                ORDER BY position ASC, applied_at ASC, id ASC"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn count_by_tasks(
        &self,
        scope: &TenantScope,
        task_ids: &[Uuid],
    ) -> AppResult<HashMap<Uuid, ContextInjectionCounts>> {
        if task_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let Some(workspace_id) = scope.workspace_id() else {
            return Ok(HashMap::new());
        };

        let rows = sqlx::query_as::<_, ContextInjectionCountRow>(
            r#"SELECT tr.orchestration_task_id AS task_id,
                      rci.item_kind,
                      COUNT(*)::bigint AS count
                 FROM task_runs tr
                 JOIN run_context_injections rci
                   ON rci.run_id = tr.id
                  AND rci.organization_id = tr.organization_id
                  AND rci.workspace_id = tr.workspace_id
                WHERE tr.organization_id = $1
                  AND tr.workspace_id = $2
                  AND tr.orchestration_task_id = ANY($3)
                GROUP BY tr.orchestration_task_id, rci.item_kind"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(task_ids)
        .fetch_all(&self.pool)
        .await?;

        let mut counts = HashMap::new();
        for row in rows {
            let entry = counts.entry(row.task_id).or_insert_with(ContextInjectionCounts::default);
            match row.item_kind.as_str() {
                "memory" => entry.applied_memories = row.count,
                "skill" => entry.applied_skills = row.count,
                _ => {}
            }
        }
        Ok(counts)
    }

    pub async fn runs_for_item(
        &self,
        scope: &TenantScope,
        item_id: Uuid,
        item_kind: &str,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<ContextAppliedRunRow>> {
        let workspace_id = require_workspace(scope)?;
        let rows = sqlx::query_as::<_, ContextAppliedRunRow>(
            r#"SELECT rci.id AS injection_id,
                      tr.id AS run_id,
                      tr.status AS run_status,
                      tr.started_at,
                      tr.finished_at,
                      rci.adapter,
                      rci.envelope_version,
                      rci.degradation_reason,
                      rci.applied_at
                 FROM run_context_injections rci
                 JOIN task_runs tr
                   ON tr.id = rci.run_id
                  AND tr.organization_id = rci.organization_id
                  AND tr.workspace_id = rci.workspace_id
                WHERE rci.organization_id = $1
                  AND rci.workspace_id = $2
                  AND rci.item_id = $3
                  AND rci.item_kind = $4
                ORDER BY rci.applied_at DESC, rci.id DESC
                LIMIT $5 OFFSET $6"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(item_id)
        .bind(item_kind)
        .bind(normalize_limit(limit))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn explain_runs_for_item_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        item_id: Uuid,
        item_kind: &str,
    ) -> AppResult<Vec<String>> {
        let plan = sqlx::query_scalar::<_, String>(
            r#"EXPLAIN
               SELECT id, run_id, adapter, envelope_version, degradation_reason, applied_at
                 FROM run_context_injections
                WHERE item_id = $1
                  AND item_kind = $2
                ORDER BY applied_at DESC, id DESC
                LIMIT 50"#,
        )
        .bind(item_id)
        .bind(item_kind)
        .fetch_all(&mut **tx)
        .await?;
        Ok(plan)
    }
}

fn require_run_scope(scope: &TenantScope, run: &TaskRun) -> AppResult<()> {
    OrchestrationRepositoryPolicy::ensure_run_scope(scope, run.organization_id, run.workspace_id)
}

fn require_workspace(scope: &TenantScope) -> AppResult<agentforge_core::WorkspaceId> {
    OrchestrationRepositoryPolicy::required_workspace(scope)
}

fn item_kind_label(kind: ContextEnvelopeItemKind) -> &'static str {
    match kind {
        ContextEnvelopeItemKind::Memory => "memory",
        ContextEnvelopeItemKind::Skill => "skill",
    }
}

fn applied_degradation_reason(envelope: &ContextEnvelope) -> Option<String> {
    envelope
        .degradation
        .iter()
        .find(|reason| reason.as_str() != "budget_truncated")
        .or_else(|| envelope.degradation.first())
        .cloned()
}

fn normalize_limit(limit: i64) -> i64 {
    limit.clamp(1, 200)
}
