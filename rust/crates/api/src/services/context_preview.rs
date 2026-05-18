//! Preview-first context injection service.

use std::sync::Arc;

use agentforge_core::{AgentId, AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::{ContextPreview, OrchestrationTask};
use chrono::{Duration, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::domain::context_preview::{
    CONTEXT_PREVIEW_TTL_MINUTES, ContextPreviewFreshnessPolicy, ContextPreviewResponse, ContextPreviewTaskDraft,
    context_preview_hash, context_preview_response,
};
use crate::domain::context_resolver::{ContextSelection, ResolvedContext, apply_context_selection};
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
        let task_draft_hash = task_draft(&task).hash();
        let preview_hash = context_preview_hash(&task_draft_hash, input.agent_id, &resolved)?;
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

        Ok(context_preview_response(&preview, resolved, Vec::new()))
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
        ContextPreviewFreshnessPolicy::ensure_task_draft_matches(&task_draft(&task).hash(), &preview.task_draft_hash)?;

        let agent_id = AgentId::from(preview.agent_id.as_uuid());
        let resolved = self
            .resolver
            .resolve(&scope.scoped_read(), crate::services::context_resolver::ResolveContextInput { task_id, agent_id })
            .await?;
        let current_hash = context_preview_hash(&preview.task_draft_hash, agent_id, &resolved)?;
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

fn selected_items_payload(resolved: &ResolvedContext) -> AppResult<Value> {
    serde_json::to_value(&resolved.applied)
        .map_err(|err| ErrorKind::Internal(anyhow::anyhow!("serialize context preview selected items: {err}")).into())
}

fn task_draft(task: &OrchestrationTask) -> ContextPreviewTaskDraft<'_> {
    ContextPreviewTaskDraft {
        task_id: task.id,
        title: &task.title,
        description: task.description.as_deref(),
        params: task.params.as_ref(),
        priority: &task.priority,
        group_id: task.group_id,
        parent_task_id: task.parent_task_id,
    }
}
