//! Task detail Context tab read model.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::{ContextCandidate, TaskRun};
use serde_json::Value;
use uuid::Uuid;

use crate::domain::context::{applied_context_source, context_content_preview, redacted_proposal_preview};
use crate::domain::task_context::task_context_provenance;
pub use crate::domain::task_context::{
    AppliedContextFeedback, AppliedContextItem, TaskContextCandidate, TaskContextEvidence, TaskContextProvenance,
    TaskContextResponse, TaskContextRun,
};
use crate::repositories::orchestration::OrchestrationTaskRepository;
use crate::repositories::task_context::{AppliedContextRow, TaskContextRepository};
use crate::repositories::task_run::RunEvidenceRow;

const CONTENT_PREVIEW_CHARS: usize = 280;

pub struct TaskContextService {
    task_repo: OrchestrationTaskRepository,
    context_repo: TaskContextRepository,
}

impl TaskContextService {
    pub fn new(task_repo: OrchestrationTaskRepository, context_repo: TaskContextRepository) -> Self {
        Self { task_repo, context_repo }
    }

    pub async fn for_task(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<TaskContextResponse> {
        self.task_repo.find_by_id(scope, task_id).await?;
        let workspace_id =
            scope.workspace_id().ok_or_else(|| -> agentforge_core::AppError { ErrorKind::Forbidden.into() })?;
        let runs = self.context_repo.runs_for_task(scope, workspace_id, task_id).await?;
        let run_ids: Vec<Uuid> = runs.iter().map(|run| run.id).collect();

        let applied_rows = self.context_repo.applied_for_runs(scope, workspace_id, &run_ids).await?;
        let candidates = self.context_repo.candidates_for_runs(scope, workspace_id, &run_ids).await?;
        let evidence_rows = self.context_repo.evidence_for_runs(scope, workspace_id, &run_ids).await?;

        let runs = runs.into_iter().map(TaskContextRun::from).collect();
        let applied_items: Vec<AppliedContextItem> = applied_rows.into_iter().map(AppliedContextItem::from).collect();
        let provenance = applied_items.iter().map(task_context_provenance).collect();
        let candidates: Vec<TaskContextCandidate> = candidates.into_iter().map(TaskContextCandidate::from).collect();
        let suggested_memory_updates =
            candidates.iter().filter(|candidate| candidate.item_kind == "memory").cloned().collect();
        let skill_candidates = candidates.iter().filter(|candidate| candidate.item_kind == "skill").cloned().collect();
        let evidence = evidence_rows.into_iter().map(TaskContextEvidence::from).collect();

        Ok(TaskContextResponse {
            task_id,
            runs,
            applied_items,
            suggested_memory_updates,
            skill_candidates,
            evidence,
            provenance,
        })
    }
}

impl From<TaskRun> for TaskContextRun {
    fn from(run: TaskRun) -> Self {
        Self {
            id: run.id,
            status: run.status,
            agent_id: run.agent_id.as_uuid(),
            started_at: run.started_at,
            finished_at: run.finished_at,
            capability_profile: run.capability_profile,
        }
    }
}

impl From<AppliedContextRow> for AppliedContextItem {
    fn from(row: AppliedContextRow) -> Self {
        let title =
            string_field(&row.applied_snapshot, "title").unwrap_or_else(|| format!("{} context", row.item_kind));
        let content = string_field(&row.applied_snapshot, "content").unwrap_or_default();
        let (content_preview, content_truncated) = context_content_preview(&content, CONTENT_PREVIEW_CHARS);
        let snapshot_sensitivity = string_field(&row.applied_snapshot, "sensitivity");
        let source = applied_context_source(&row.applied_snapshot);
        let content_ref = string_field(&row.applied_snapshot, "content_ref");
        let revoked = row.revoked_at.is_some() || row.item_state.as_deref() == Some("revoked");

        Self {
            injection_id: row.injection_id,
            run_id: row.run_id,
            item_id: row.item_id,
            item_kind: row.item_kind,
            position: row.position,
            title,
            content_preview,
            content_truncated,
            content_ref,
            scope_kind: row.scope_kind,
            scope_id: row.scope_id,
            sensitivity: row.sensitivity.or(snapshot_sensitivity),
            state: row.item_state,
            revoked,
            source_task_id: row.source_task_id,
            source_run_id: row.source_run_id,
            source,
            last_used_at: row.last_used_at.or(Some(row.applied_at)),
            last_verified_at: row.last_verified_at,
            applied_at: row.applied_at,
            adapter: row.adapter,
            envelope_version: row.envelope_version,
            capability_profile: row.capability_profile,
            degradation_reason: row.degradation_reason,
            feedback: match (row.feedback_label, row.feedback_updated_at) {
                (Some(label), Some(updated_at)) => {
                    Some(AppliedContextFeedback { label, note: row.feedback_note, updated_at })
                }
                _ => None,
            },
        }
    }
}

impl From<ContextCandidate> for TaskContextCandidate {
    fn from(candidate: ContextCandidate) -> Self {
        Self {
            id: candidate.id,
            item_kind: candidate.item_kind,
            state: candidate.state,
            owner_user_id: candidate.owner_user_id,
            source_run_id: candidate.source_run_id,
            target_skill_id: candidate.target_skill_id,
            proposed_preview: redacted_proposal_preview(&candidate.proposed_content),
            created_at: candidate.created_at,
            updated_at: candidate.updated_at,
        }
    }
}

impl From<RunEvidenceRow> for TaskContextEvidence {
    fn from(row: RunEvidenceRow) -> Self {
        Self {
            run_id: row.run_id,
            source_type: row.source_type,
            source_id: row.source_id,
            agent_id: row.agent_id,
            payload: row.payload,
            created_at: row.created_at,
        }
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}
