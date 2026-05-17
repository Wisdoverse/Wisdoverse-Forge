//! Preview-first context injection service.

use std::sync::Arc;

use agentforge_core::{AgentId, AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::{ContextPreview, OrchestrationTask};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::context_preview::{CONTEXT_PREVIEW_TTL_MINUTES, ContextPreviewFreshnessPolicy};
use crate::domain::context_resolver::{
    ContextItemKind, ContextSelection, DegradationReason, ResolvedContext, ResolvedItemRef, apply_context_selection,
};
use crate::domain::orchestration::{ParticipantAvailabilityAction, ParticipantAvailabilityPolicy};
use crate::repositories::context_preview::{ContextPreviewRepository, CreateContextPreviewRecord};
use crate::repositories::orchestration::{OrchestrationTaskRepository, ParticipantRepository};
use crate::services::orchestration::OrchestrationService;

#[derive(Debug, Clone)]
pub struct CreateContextPreviewInput {
    pub task_id: Uuid,
    pub agent_id: AgentId,
}

#[derive(Debug, Clone)]
pub struct PublishWithContextInput {
    pub context_preview_id: Uuid,
    pub preview_hash: String,
    pub pinned_item_ids: Vec<Uuid>,
    pub removed_item_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextPreviewResponse {
    pub context_preview_id: Uuid,
    pub preview_hash: String,
    pub task_id: Uuid,
    pub agent_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub capability: Value,
    pub degradation: Vec<String>,
    pub items: Vec<ContextPreviewItem>,
    pub suggested_items: Vec<ContextPreviewItem>,
    pub previously_pinned: Vec<ContextPreviewItem>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextPreviewItem {
    pub id: Uuid,
    pub item_kind: String,
    pub title: String,
    pub selected: bool,
    pub pinned: bool,
    pub scope_kind: Option<String>,
    pub scope_id: Option<Uuid>,
    pub sensitivity: Option<String>,
    pub estimated_tokens: u32,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub why: String,
}

pub struct ContextPreviewService {
    previews: ContextPreviewRepository,
    tasks: OrchestrationTaskRepository,
    participants: ParticipantRepository,
    resolver: Arc<crate::services::context_resolver::ContextResolverService>,
}

impl ContextPreviewService {
    pub fn new(
        previews: ContextPreviewRepository,
        tasks: OrchestrationTaskRepository,
        participants: ParticipantRepository,
        resolver: Arc<crate::services::context_resolver::ContextResolverService>,
    ) -> Self {
        Self { previews, tasks, participants, resolver }
    }

    pub async fn create(
        &self,
        scope: &TenantScope,
        input: CreateContextPreviewInput,
    ) -> AppResult<ContextPreviewResponse> {
        let task = self.tasks.find_by_id(scope, input.task_id).await?;
        let workspace_id = scope.workspace_id().ok_or_else(|| ErrorKind::Forbidden)?;
        let participant = self.participants.find_by_agent_id(scope, input.agent_id).await?;
        ParticipantAvailabilityPolicy::ensure_available(
            &participant.name,
            &participant.status,
            ParticipantAvailabilityAction::PreviewContext,
        )?;

        let resolved = self
            .resolver
            .resolve(
                &scope.scoped_read(),
                crate::services::context_resolver::ResolveContextInput { task_id: task.id, agent_id: input.agent_id },
            )
            .await?;
        let task_draft_hash = task_draft_hash(&task);
        let preview_hash = preview_hash(&task_draft_hash, input.agent_id, &resolved)?;
        let selected_items = selected_items_payload(&resolved)?;
        let expires_at = Utc::now() + Duration::minutes(CONTEXT_PREVIEW_TTL_MINUTES);
        let preview = self
            .previews
            .create(
                scope,
                CreateContextPreviewRecord {
                    workspace_id: workspace_id.as_uuid(),
                    task_id: task.id,
                    agent_id: input.agent_id.as_uuid(),
                    task_draft_hash: &task_draft_hash,
                    preview_hash: &preview_hash,
                    selected_items: &selected_items,
                    expires_at,
                },
            )
            .await?;

        Ok(response_from_preview(&preview, resolved, Vec::new()))
    }

    pub async fn validate_publish(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        input: &PublishWithContextInput,
    ) -> AppResult<ValidatedContextPreview> {
        let preview = self.previews.find_live_for_publish(scope, input.context_preview_id, task_id).await?;
        ContextPreviewFreshnessPolicy::ensure_request_hash_matches(&preview.preview_hash, &input.preview_hash)?;
        let task = self.tasks.find_by_id(scope, task_id).await?;
        ContextPreviewFreshnessPolicy::ensure_workspace_matches(
            scope.workspace_id().map(|workspace_id| workspace_id.as_uuid()),
            preview.workspace_id.as_uuid(),
        )?;
        ContextPreviewFreshnessPolicy::ensure_task_draft_matches(&task_draft_hash(&task), &preview.task_draft_hash)?;

        let agent_id = AgentId::from(preview.agent_id.as_uuid());
        let resolved = self
            .resolver
            .resolve(&scope.scoped_read(), crate::services::context_resolver::ResolveContextInput { task_id, agent_id })
            .await?;
        let current_hash = preview_hash(&preview.task_draft_hash, agent_id, &resolved)?;
        ContextPreviewFreshnessPolicy::ensure_resolved_context_matches(&current_hash, &preview.preview_hash)?;

        let selected = apply_context_selection(
            resolved,
            &ContextSelection {
                pinned_item_ids: input.pinned_item_ids.clone(),
                removed_item_ids: input.removed_item_ids.clone(),
            },
        );
        Ok(ValidatedContextPreview { preview, resolved: selected.resolved, warnings: selected.warnings })
    }

    pub async fn publish_existing_task(
        &self,
        orchestration: &OrchestrationService,
        scope: &TenantScope,
        task_id: Uuid,
        input: PublishWithContextInput,
    ) -> AppResult<OrchestrationTask> {
        let validated = self.validate_publish(scope, task_id, &input).await?;
        orchestration
            .assign_existing_task_to_agent_with_context(
                scope,
                task_id,
                AgentId::from(validated.preview.agent_id.as_uuid()),
                validated.resolved,
            )
            .await
    }
}

#[derive(Debug, Clone)]
pub struct ValidatedContextPreview {
    pub preview: ContextPreview,
    pub resolved: ResolvedContext,
    pub warnings: Vec<String>,
}

fn response_from_preview(
    preview: &ContextPreview,
    resolved: ResolvedContext,
    warnings: Vec<String>,
) -> ContextPreviewResponse {
    let capability = serde_json::to_value(&resolved.capability).unwrap_or_else(|_| json!({}));
    let degradation = resolved.degradation.iter().map(|reason| reason_label(reason).to_string()).collect();
    let items = resolved.applied.iter().map(|item| preview_item(item, true, false)).collect();
    let suggested_items = resolved.suggested.iter().map(|item| preview_item(item, false, false)).collect();
    ContextPreviewResponse {
        context_preview_id: preview.id,
        preview_hash: preview.preview_hash.clone(),
        task_id: preview.task_id,
        agent_id: preview.agent_id.as_uuid(),
        expires_at: preview.expires_at,
        capability,
        degradation,
        items,
        suggested_items,
        previously_pinned: Vec::new(),
        warnings,
    }
}

fn preview_item(item: &ResolvedItemRef, selected: bool, pinned: bool) -> ContextPreviewItem {
    ContextPreviewItem {
        id: item.id,
        item_kind: match item.kind {
            ContextItemKind::Memory => ContextItemKind::Memory.label(),
            ContextItemKind::Skill => ContextItemKind::Skill.label(),
        }
        .to_string(),
        title: item.title.clone(),
        selected,
        pinned,
        scope_kind: item.scope_kind.clone(),
        scope_id: item.scope_id,
        sensitivity: item.sensitivity.clone(),
        estimated_tokens: item.estimated_tokens,
        last_used_at: item.last_used_at,
        last_verified_at: item.last_verified_at,
        why: item.why.clone(),
    }
}

fn selected_items_payload(resolved: &ResolvedContext) -> AppResult<Value> {
    serde_json::to_value(&resolved.applied)
        .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("serialize context preview selected items: {err}")).into())
}

fn task_draft_hash(task: &OrchestrationTask) -> String {
    let material = json!({
        "task_id": task.id,
        "title": task.title,
        "description": task.description,
        "params": task.params,
        "priority": task.priority,
        "group_id": task.group_id,
        "parent_task_id": task.parent_task_id,
    });
    hex::encode(Sha256::digest(material.to_string().as_bytes()))
}

fn preview_hash(task_draft_hash: &str, agent_id: AgentId, resolved: &ResolvedContext) -> AppResult<String> {
    let material = json!({
        "task_draft_hash": task_draft_hash,
        "agent_id": agent_id.as_uuid(),
        "applied": resolved.applied,
        "suggested": resolved.suggested,
        "capability": resolved.capability,
        "degradation": resolved.degradation,
        "envelope_version": resolved.envelope_version,
    });
    serde_json::to_vec(&material)
        .map(|bytes| hex::encode(Sha256::digest(&bytes)))
        .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("serialize context preview hash: {err}")).into())
}

fn reason_label(reason: &DegradationReason) -> &'static str {
    reason.label()
}
