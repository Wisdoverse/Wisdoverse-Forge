//! Task detail Context tab read model.

use agentforge_core::{AppResult, ErrorKind, TenantScope, UserId};
use agentforge_db::entities::{ContextCandidate, TaskRun};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::context_governance::ContextGovernancePolicy;
use crate::repositories::orchestration::OrchestrationTaskRepository;
use crate::repositories::task_context::{AppliedContextRow, TaskContextRepository};
use crate::repositories::task_run::RunEvidenceRow;

const CONTENT_PREVIEW_CHARS: usize = 280;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContextResponse {
    pub task_id: Uuid,
    pub runs: Vec<TaskContextRun>,
    pub applied_items: Vec<AppliedContextItem>,
    pub suggested_memory_updates: Vec<TaskContextCandidate>,
    pub skill_candidates: Vec<TaskContextCandidate>,
    pub evidence: Vec<TaskContextEvidence>,
    pub provenance: Vec<TaskContextProvenance>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContextRun {
    pub id: Uuid,
    pub status: String,
    pub agent_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub capability_profile: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedContextItem {
    pub injection_id: Uuid,
    pub run_id: Uuid,
    pub item_id: Uuid,
    pub item_kind: String,
    pub position: i32,
    pub title: String,
    pub content_preview: String,
    pub content_truncated: bool,
    pub content_ref: Option<String>,
    pub scope_kind: Option<String>,
    pub scope_id: Option<Uuid>,
    pub sensitivity: Option<String>,
    pub state: Option<String>,
    pub revoked: bool,
    pub source_task_id: Option<Uuid>,
    pub source_run_id: Option<Uuid>,
    pub source: Option<AppliedContextSource>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub applied_at: DateTime<Utc>,
    pub adapter: String,
    pub envelope_version: String,
    pub capability_profile: Value,
    pub degradation_reason: Option<String>,
    pub feedback: Option<AppliedContextFeedback>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedContextSource {
    pub source_type: String,
    pub source_id: Option<Uuid>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedContextFeedback {
    pub label: String,
    pub note: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContextCandidate {
    pub id: Uuid,
    pub item_kind: String,
    pub state: String,
    pub owner_user_id: UserId,
    pub source_run_id: Option<Uuid>,
    pub target_skill_id: Option<agentforge_core::SkillId>,
    pub proposed_preview: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContextEvidence {
    pub run_id: Option<Uuid>,
    pub source_type: String,
    pub source_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContextProvenance {
    pub run_id: Uuid,
    pub item_id: Uuid,
    pub item_kind: String,
    pub title: String,
    pub source: Option<AppliedContextSource>,
    pub adapter: String,
    pub envelope_version: String,
    pub applied_at: DateTime<Utc>,
    pub state: Option<String>,
    pub revoked: bool,
}

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
        let provenance = applied_items.iter().map(TaskContextProvenance::from).collect();
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
        let (content_preview, content_truncated) = preview_text(&content, CONTENT_PREVIEW_CHARS);
        let snapshot_sensitivity = string_field(&row.applied_snapshot, "sensitivity");
        let source = source_field(&row.applied_snapshot);
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

impl From<&AppliedContextItem> for TaskContextProvenance {
    fn from(item: &AppliedContextItem) -> Self {
        Self {
            run_id: item.run_id,
            item_id: item.item_id,
            item_kind: item.item_kind.clone(),
            title: item.title.clone(),
            source: item.source.clone(),
            adapter: item.adapter.clone(),
            envelope_version: item.envelope_version.clone(),
            applied_at: item.applied_at,
            state: item.state.clone(),
            revoked: item.revoked,
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

fn source_field(value: &Value) -> Option<AppliedContextSource> {
    let source = value.get("source")?.as_object()?;
    let source_type = source.get("source_type")?.as_str()?.to_string();
    let source_id = source.get("source_id").and_then(Value::as_str).and_then(|value| Uuid::parse_str(value).ok());
    let title = source.get("title").and_then(Value::as_str).map(str::to_string);
    Some(AppliedContextSource { source_type, source_id, title })
}

fn preview_text(value: &str, limit: usize) -> (String, bool) {
    let mut preview = String::new();
    let mut truncated = false;
    for (idx, ch) in value.chars().enumerate() {
        if idx >= limit {
            truncated = true;
            break;
        }
        preview.push(ch);
    }
    if truncated {
        preview.push_str("...");
    }
    (preview, truncated)
}

fn redacted_proposal_preview(value: &Value) -> Value {
    let Some(map) = value.as_object() else {
        return json!({});
    };
    let mut out = serde_json::Map::new();
    for key in ["title", "name", "description", "scope_kind", "visibility"] {
        if let Some(value) = map.get(key)
            && value.is_string()
        {
            out.insert(key.to_string(), value.clone());
        }
    }
    if let Some(content) = map.get("content").and_then(Value::as_str) {
        let classification = ContextGovernancePolicy::classify_sensitivity(content);
        let preview = classification.redacted_preview.unwrap_or_else(|| content.chars().take(160).collect());
        out.insert("content_preview".to_string(), json!(preview));
    }
    Value::Object(out)
}
