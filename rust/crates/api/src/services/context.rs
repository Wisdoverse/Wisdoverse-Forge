//! Context approval queue service.

use std::sync::Arc;

use agentforge_core::{
    AppResult, ErrorKind, ProjectId, ScopedRead, ScopedWrite, ScopedWriteError, TeamId, TenantScope, UserId,
    WorkspaceId,
};
use agentforge_db::entities::{ContextApproval, ContextCandidate, ContextFeedback, MemoryItem, Skill};
use agentforge_infra::NatsClient;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::context::{
    ContextCandidateKind, ContextCandidateManualRejectionAudit, ContextCandidatePolicy, ContextFeedbackLabel,
    ContextFeedbackPolicy, ContextItemKind, context_candidate_subject, ensure_pending_candidate,
    normalize_candidate_kind_filter, normalize_candidate_state_filter, normalize_context_candidate_limit,
    normalize_feedback_note, normalize_reason, normalize_scope_kind_filter, redacted_proposal_preview,
    validate_context_sensitivity, validate_ttl,
};
use crate::domain::context_governance::ContextAuditEvent;
use crate::domain::memory::MemoryScopeKind;
use crate::repositories::context_approval::{ContextApprovalRepository, CreateContextApprovalRecord};
use crate::repositories::context_candidate::{
    ContextCandidateListRow, ContextCandidateRepository, CreateContextCandidateRecord,
};
use crate::repositories::context_feedback::{ContextFeedbackRepository, CreateContextFeedbackRecord};
use crate::repositories::memory::{CreateMemoryRecord, MemoryRepository};
use crate::repositories::skill::SkillRepository;
use crate::repositories::skill_version::SkillVersionRepository;
use crate::services::context_governance::ContextGovernanceService;

#[derive(Debug, Clone)]
pub struct CreateContextCandidateInput {
    pub source_run_id: Option<Uuid>,
    pub target_skill_id: Option<Uuid>,
    pub item_kind: ContextCandidateKind,
    pub proposed_content: Value,
}

#[derive(Debug, Clone)]
pub struct ApproveContextCandidateInput {
    pub scope_kind: MemoryScopeKind,
    pub scope_id: Option<Uuid>,
    pub ttl_at: Option<DateTime<Utc>>,
    pub sensitivity: Option<String>,
    pub reason: Option<String>,
    pub redacted: bool,
    pub user_attested: bool,
    pub confirm_expansion: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ListContextCandidatesInput {
    pub state: Option<String>,
    pub item_kind: Option<String>,
    pub scope_kind: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct RejectContextCandidateInput {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextCandidateSummary {
    pub id: Uuid,
    pub workspace_id: WorkspaceId,
    pub item_kind: String,
    pub state: String,
    pub owner_user_id: UserId,
    pub source_run_id: Option<Uuid>,
    pub target_skill_id: Option<agentforge_core::SkillId>,
    pub proposed_scope_kind: String,
    pub source_available: bool,
    pub proposed_preview: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextApprovalOutcome {
    pub candidate: ContextCandidate,
    pub approval: Option<ContextApproval>,
    pub memory_item: Option<MemoryItem>,
    pub skill: Option<Skill>,
}

pub struct ContextApprovalService {
    candidates: ContextCandidateRepository,
    memory: MemoryRepository,
    nats: Option<Arc<NatsClient>>,
}

impl ContextApprovalService {
    pub fn new(pool: PgPool, nats: Option<Arc<NatsClient>>) -> Self {
        Self {
            candidates: ContextCandidateRepository::new(pool.clone()),
            memory: MemoryRepository::new(pool.clone()),
            nats,
        }
    }

    pub async fn create_candidate(
        &self,
        scope: &TenantScope,
        input: CreateContextCandidateInput,
    ) -> AppResult<ContextCandidate> {
        let workspace_id = required_workspace(scope)?;
        ContextCandidatePolicy::validate_create(input.item_kind, input.target_skill_id, &input.proposed_content)?;

        let mut tx = self.candidates.pool().begin().await?;
        let candidate = ContextCandidateRepository::create_in_tx(
            &mut tx,
            scope,
            CreateContextCandidateRecord {
                workspace_id,
                source_run_id: input.source_run_id,
                target_skill_id: input.target_skill_id,
                item_kind: input.item_kind.as_label(),
                proposed_content: &input.proposed_content,
                owner_user_id: scope.user_id(),
            },
        )
        .await?;
        self.emit_candidate_audit(
            &mut tx,
            scope,
            "governance.context.candidate.created",
            json!({
                "item_kind": candidate.item_kind,
                "workspace_id": candidate.workspace_id,
                "has_source_run": candidate.source_run_id.is_some(),
                "has_target_skill": candidate.target_skill_id.is_some()
            }),
        )
        .await?;
        tx.commit().await?;
        self.publish_candidate_event(scope, &candidate, "created", None).await;
        Ok(candidate)
    }

    pub async fn list_pending(
        &self,
        scope: &TenantScope,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> AppResult<Vec<ContextCandidateSummary>> {
        self.list(
            scope,
            ListContextCandidatesInput { state: Some("pending".into()), limit, offset, ..Default::default() },
        )
        .await
    }

    pub async fn list(
        &self,
        scope: &TenantScope,
        input: ListContextCandidatesInput,
    ) -> AppResult<Vec<ContextCandidateSummary>> {
        let proof = self.validated_read(scope).await?;
        let state = normalize_candidate_state_filter(input.state.as_deref())?;
        let item_kind = normalize_candidate_kind_filter(input.item_kind.as_deref())?;
        let scope_kind = normalize_scope_kind_filter(input.scope_kind.as_deref())?;
        let candidates = self
            .candidates
            .list_visible(
                &proof,
                state,
                item_kind,
                scope_kind,
                normalize_context_candidate_limit(input.limit),
                input.offset.unwrap_or(0).max(0),
            )
            .await?;
        Ok(candidates.iter().map(candidate_summary).collect())
    }

    pub async fn approve(
        &self,
        scope: &TenantScope,
        id: Uuid,
        input: ApproveContextCandidateInput,
    ) -> AppResult<ContextApprovalOutcome> {
        validate_ttl(input.ttl_at)?;
        let requested_sensitivity = input.sensitivity.as_deref().map(validate_context_sensitivity).transpose()?;
        let approval_reason = normalize_reason(input.reason)?;
        let proof = self.validated_read(scope).await?;
        let workspace_id = required_workspace(scope)?;
        let target = self.validated_write_scope(&proof, workspace_id, input.scope_kind, input.scope_id).await?;

        let mut tx = self.candidates.pool().begin().await?;
        let candidate = ContextCandidateRepository::lock_visible_for_update(&mut tx, &proof, id).await?;
        ensure_pending_candidate(candidate.id, &candidate.state)?;

        let source_run_status = match candidate.source_run_id {
            Some(source_run_id) => {
                ContextCandidateRepository::source_run_status_in_tx(&mut tx, &proof, source_run_id).await?
            }
            None => None,
        };
        if let Err(rejection) =
            ContextCandidatePolicy::ensure_source_run_approvable(candidate.source_run_id, source_run_status.as_deref())
        {
            let rejected = ContextCandidateRepository::update_state_in_tx(&mut tx, candidate.id, "rejected").await?;
            self.emit_candidate_audit(
                &mut tx,
                scope,
                rejection.audit_action(),
                rejection.audit_payload(&candidate.item_kind),
            )
            .await?;
            tx.commit().await?;
            self.publish_candidate_event(scope, &rejected, "rejected", None).await;
            return Err(rejection.into_app_error());
        }

        let self_approval = candidate.owner_user_id == scope.user_id();
        if let Err(rejection) = ContextCandidatePolicy::ensure_self_approval_scope(self_approval, target.kind()) {
            self.emit_candidate_audit(
                &mut tx,
                scope,
                rejection.audit_action(),
                rejection.audit_payload(&candidate.item_kind),
            )
            .await?;
            tx.commit().await?;
            return Err(rejection.into_app_error());
        }

        match ContextCandidateKind::from_label(candidate.item_kind.as_str())? {
            ContextCandidateKind::Memory => {
                if let Err(rejection) =
                    ContextCandidatePolicy::ensure_memory_scope_expansion(target.kind(), input.confirm_expansion)
                {
                    self.emit_candidate_audit(
                        &mut tx,
                        scope,
                        rejection.audit_action(),
                        rejection.audit_payload(&candidate.item_kind),
                    )
                    .await?;
                    tx.commit().await?;
                    return Err(rejection.into_app_error());
                }
                let prepared = match ContextCandidatePolicy::prepare_memory_candidate(
                    &candidate.proposed_content,
                    requested_sensitivity,
                    input.redacted,
                ) {
                    Ok(prepared) => prepared,
                    Err(err) => {
                        self.emit_candidate_audit(
                            &mut tx,
                            scope,
                            err.audit_action(),
                            err.audit_payload(&candidate.item_kind),
                        )
                        .await?;
                        tx.commit().await?;
                        return Err(err.into_app_error());
                    }
                };
                if let Err(rejection) = ContextCandidatePolicy::ensure_wider_secret_memory_attestation(
                    &prepared.sensitivity,
                    target.kind(),
                    input.user_attested,
                ) {
                    self.emit_candidate_audit(
                        &mut tx,
                        scope,
                        rejection.audit_action(),
                        rejection.audit_payload(&candidate.item_kind),
                    )
                    .await?;
                    tx.commit().await?;
                    return Err(rejection.into_app_error());
                }

                let approval = ContextApprovalRepository::create_in_tx(
                    &mut tx,
                    CreateContextApprovalRecord {
                        candidate_id: candidate.id,
                        approver_user_id: scope.user_id(),
                        decision: "approved",
                        scope_kind: Some(target.kind().as_label()),
                        scope_id: Some(target.id()),
                        ttl_at: input.ttl_at,
                        sensitivity: Some(prepared.sensitivity.as_str()),
                        reason: approval_reason.as_deref(),
                        self_approval,
                        user_attest_at: input.user_attested.then(Utc::now),
                    },
                )
                .await?;
                let provenance = approval_provenance(&candidate, scope);
                let item = MemoryRepository::create_in_tx(
                    &mut tx,
                    &proof,
                    CreateMemoryRecord {
                        workspace_id: candidate.workspace_id,
                        write_scope: &target,
                        owner_user_id: candidate.owner_user_id.as_uuid(),
                        source_task_id: prepared.source_task_id,
                        source_run_id: candidate.source_run_id,
                        title: &prepared.title,
                        content: &prepared.content,
                        content_redacted: prepared.content_redacted,
                        visibility: &prepared.visibility,
                        sensitivity: prepared.sensitivity.as_str(),
                        provenance: &provenance,
                        ttl_expires_at: input.ttl_at,
                        confidence: prepared.confidence,
                        state: "active",
                    },
                )
                .await?;
                let updated = ContextCandidateRepository::update_state_in_tx(&mut tx, candidate.id, "approved").await?;
                self.emit_candidate_audit(
                    &mut tx,
                    scope,
                    "governance.context.candidate.approved",
                    json!({
                        "item_kind": candidate.item_kind,
                        "result_kind": "memory_item",
                        "scope_kind": item.scope_kind,
                        "sensitivity": item.sensitivity,
                        "self_approval": approval.self_approval,
                        "classification": prepared.classification_payload
                    }),
                )
                .await?;
                tx.commit().await?;
                self.publish_candidate_event(scope, &updated, "approved", Some((&item.scope_kind, item.scope_id)))
                    .await;
                Ok(ContextApprovalOutcome {
                    candidate: updated,
                    approval: Some(approval),
                    memory_item: Some(item),
                    skill: None,
                })
            }
            ContextCandidateKind::Skill => {
                let skill_id = ContextCandidatePolicy::require_skill_target_id(candidate.target_skill_id)?;
                let current = SkillRepository::lock_org_skill_for_update(&mut tx, scope, skill_id.as_uuid()).await?;
                ContextCandidatePolicy::ensure_skill_candidate_approvable(
                    current.id,
                    &current.state,
                    current.revoked_at.is_some(),
                )?;
                let from_kind =
                    ContextCandidatePolicy::resolve_skill_candidate_scope_kind(current.scope_kind.as_deref())?;
                if let Err(rejection) =
                    ContextCandidatePolicy::ensure_scope_expansion(from_kind, target.kind(), input.confirm_expansion)
                {
                    self.emit_candidate_audit(
                        &mut tx,
                        scope,
                        rejection.audit_action(),
                        rejection.audit_payload(&candidate.item_kind),
                    )
                    .await?;
                    tx.commit().await?;
                    return Err(rejection.into_app_error());
                }
                if let Err(rejection) = ContextCandidatePolicy::ensure_skill_content_approvable(&current.content) {
                    self.emit_candidate_audit(
                        &mut tx,
                        scope,
                        rejection.audit_action(),
                        rejection.audit_payload(&candidate.item_kind),
                    )
                    .await?;
                    tx.commit().await?;
                    return Err(rejection.into_app_error());
                }
                let sensitivity = match requested_sensitivity {
                    Some(value) => value,
                    None => validate_context_sensitivity(&current.sensitivity)?,
                };
                let approval = ContextApprovalRepository::create_in_tx(
                    &mut tx,
                    CreateContextApprovalRecord {
                        candidate_id: candidate.id,
                        approver_user_id: scope.user_id(),
                        decision: "approved",
                        scope_kind: Some(target.kind().as_label()),
                        scope_id: Some(target.id()),
                        ttl_at: input.ttl_at,
                        sensitivity: Some(sensitivity),
                        reason: approval_reason.as_deref(),
                        self_approval,
                        user_attest_at: input.user_attested.then(Utc::now),
                    },
                )
                .await?;
                let prior_version =
                    SkillVersionRepository::insert_snapshot_in_tx(&mut tx, &current, scope.user_id()).await?;
                let skill = SkillRepository::promote_candidate_in_tx(
                    &mut tx,
                    current.id.as_uuid(),
                    target.kind().as_label(),
                    target.id(),
                    input.ttl_at,
                    sensitivity,
                )
                .await?;
                let updated = ContextCandidateRepository::update_state_in_tx(&mut tx, candidate.id, "approved").await?;
                self.emit_candidate_audit(
                    &mut tx,
                    scope,
                    "governance.context.candidate.approved",
                    json!({
                        "item_kind": candidate.item_kind,
                        "result_kind": "skill",
                        "scope_kind": skill.scope_kind,
                        "sensitivity": skill.sensitivity,
                        "self_approval": approval.self_approval,
                        "from_version": current.version,
                        "resulting_version": skill.version,
                        "skill_version_id": prior_version.id
                    }),
                )
                .await?;
                tx.commit().await?;
                if let (Some(scope_kind), Some(scope_id)) = (skill.scope_kind.as_deref(), skill.scope_id) {
                    self.publish_candidate_event(scope, &updated, "approved", Some((scope_kind, scope_id))).await;
                }
                Ok(ContextApprovalOutcome {
                    candidate: updated,
                    approval: Some(approval),
                    memory_item: None,
                    skill: Some(skill),
                })
            }
        }
    }

    pub async fn reject(
        &self,
        scope: &TenantScope,
        id: Uuid,
        input: RejectContextCandidateInput,
    ) -> AppResult<ContextApprovalOutcome> {
        let proof = self.validated_read(scope).await?;
        let mut tx = self.candidates.pool().begin().await?;
        let candidate = ContextCandidateRepository::lock_visible_for_update(&mut tx, &proof, id).await?;
        ensure_pending_candidate(candidate.id, &candidate.state)?;
        let reason = normalize_reason(input.reason)?;
        let approval = ContextApprovalRepository::create_in_tx(
            &mut tx,
            CreateContextApprovalRecord {
                candidate_id: candidate.id,
                approver_user_id: scope.user_id(),
                decision: "rejected",
                scope_kind: None,
                scope_id: None,
                ttl_at: None,
                sensitivity: None,
                reason: reason.as_deref(),
                self_approval: candidate.owner_user_id == scope.user_id(),
                user_attest_at: None,
            },
        )
        .await?;
        let updated = ContextCandidateRepository::update_state_in_tx(&mut tx, candidate.id, "rejected").await?;
        let audit = ContextCandidateManualRejectionAudit::new(reason, approval.self_approval);
        self.emit_candidate_audit(&mut tx, scope, audit.audit_action(), audit.audit_payload(&candidate.item_kind))
            .await?;
        tx.commit().await?;
        self.publish_candidate_event(scope, &updated, "rejected", None).await;
        Ok(ContextApprovalOutcome { candidate: updated, approval: Some(approval), memory_item: None, skill: None })
    }

    async fn validated_read(&self, scope: &TenantScope) -> AppResult<ScopedRead> {
        validated_context_read(self.candidates.pool(), scope).await
    }

    async fn validated_write_scope(
        &self,
        proof: &ScopedRead,
        workspace_id: WorkspaceId,
        scope_kind: MemoryScopeKind,
        scope_id: Option<Uuid>,
    ) -> AppResult<ScopedWrite> {
        let (scope_kind, scope_id) =
            ContextCandidatePolicy::resolve_approval_scope(scope_kind, scope_id, proof.user_id().as_uuid())?;
        let write = ScopedWrite::try_new(scope_kind, scope_id, proof.clone()).map_err(scoped_write_error)?;
        if !self.memory.resource_belongs_to_scope(proof, scope_kind, scope_id, workspace_id).await? {
            return Err(ErrorKind::Forbidden.into());
        }
        Ok(write)
    }

    async fn emit_candidate_audit(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        scope: &TenantScope,
        action: &'static str,
        payload: Value,
    ) -> AppResult<()> {
        ContextGovernanceService::emit_audit(
            tx,
            scope,
            ContextAuditEvent {
                action,
                resource_type: "context_candidate",
                resource_id: None,
                payload,
                ip_address: None,
            },
        )
        .await?;
        Ok(())
    }

    async fn publish_candidate_event(
        &self,
        scope: &TenantScope,
        candidate: &ContextCandidate,
        event: &'static str,
        approved_scope: Option<(&str, Uuid)>,
    ) {
        let Some(nats) = &self.nats else {
            return;
        };
        let (scope_kind, scope_id) = approved_scope.unwrap_or(("user", candidate.owner_user_id.as_uuid()));
        let subject = context_candidate_subject(scope.org_id().as_uuid(), scope_kind, scope_id, event);
        let payload = json!({
            "type": format!("context_candidate.{event}"),
            "candidateId": candidate.id,
            "itemKind": candidate.item_kind,
            "state": candidate.state,
            "scopeKind": scope_kind,
            "scopeId": scope_id,
            "timestamp": Utc::now().to_rfc3339()
        });
        if let Err(err) = nats.publish_json(&subject, payload).await {
            tracing::warn!(error = ?err, subject, "failed to publish context candidate broadcast");
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecordContextFeedbackInput {
    pub run_id: Uuid,
    pub item_id: Uuid,
    pub item_kind: ContextItemKind,
    pub label: ContextFeedbackLabel,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextFeedbackOutcome {
    pub feedback: ContextFeedback,
    pub item_state_changed: bool,
}

pub struct ContextFeedbackService {
    feedback: ContextFeedbackRepository,
}

impl ContextFeedbackService {
    pub fn new(pool: PgPool) -> Self {
        Self { feedback: ContextFeedbackRepository::new(pool) }
    }

    pub async fn record(
        &self,
        scope: &TenantScope,
        input: RecordContextFeedbackInput,
    ) -> AppResult<ContextFeedbackOutcome> {
        let workspace_id = required_workspace(scope)?;
        let proof = validated_context_read(self.feedback.pool(), scope).await?;
        let note = normalize_feedback_note(input.note)?;
        let item_kind = input.item_kind.as_label();
        let label = input.label.as_label();

        let mut tx = self.feedback.pool().begin().await?;
        let run_status =
            ContextFeedbackRepository::run_status_in_scope_in_tx(&mut tx, &proof, workspace_id, input.run_id).await?;
        ContextFeedbackPolicy::ensure_run_terminal(&run_status)?;

        let already_revoked = match input.item_kind {
            ContextItemKind::Memory => {
                let item = ContextFeedbackRepository::lock_memory_for_feedback_in_tx(
                    &mut tx,
                    &proof,
                    workspace_id,
                    input.item_id,
                )
                .await?;
                item.revoked_at.is_some()
            }
            ContextItemKind::Skill => {
                let item = ContextFeedbackRepository::lock_skill_for_feedback_in_tx(
                    &mut tx,
                    &proof,
                    workspace_id,
                    input.item_id,
                )
                .await?;
                item.revoked_at.is_some()
            }
        };

        let feedback = ContextFeedbackRepository::upsert_in_tx(
            &mut tx,
            &proof,
            CreateContextFeedbackRecord {
                workspace_id,
                run_id: input.run_id,
                item_id: input.item_id,
                item_kind,
                label,
                note: note.as_deref(),
            },
        )
        .await?;

        let mut item_state_changed = false;
        if !already_revoked {
            item_state_changed = match input.item_kind {
                ContextItemKind::Memory => {
                    self.apply_memory_feedback(&mut tx, &proof, workspace_id, input.item_id, input.label).await?
                }
                ContextItemKind::Skill => {
                    self.apply_skill_feedback(&mut tx, &proof, workspace_id, input.item_id, input.label).await?
                }
            };
        }

        ContextGovernanceService::emit_audit(
            &mut tx,
            scope,
            ContextAuditEvent {
                action: "governance.context.feedback.recorded",
                resource_type: "context_feedback",
                resource_id: Some(feedback.id),
                payload: json!({
                    "run_id": feedback.run_id,
                    "item_id": feedback.item_id,
                    "item_kind": feedback.item_kind,
                    "label": feedback.label,
                    "item_state_changed": item_state_changed
                }),
                ip_address: None,
            },
        )
        .await?;

        tx.commit().await?;
        Ok(ContextFeedbackOutcome { feedback, item_state_changed })
    }

    async fn apply_memory_feedback(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        proof: &ScopedRead,
        workspace_id: WorkspaceId,
        item_id: Uuid,
        label: ContextFeedbackLabel,
    ) -> AppResult<bool> {
        match label {
            ContextFeedbackLabel::Useful => {
                ContextFeedbackRepository::mark_memory_useful_in_tx(tx, item_id).await?;
                Ok(false)
            }
            ContextFeedbackLabel::Stale => {
                ContextFeedbackRepository::mark_memory_stale_in_tx(tx, item_id).await?;
                let stale_count =
                    ContextFeedbackRepository::count_label_in_tx(tx, proof, workspace_id, item_id, "memory", "stale")
                        .await?;
                if ContextFeedbackPolicy::should_revoke_after_label(label, stale_count) {
                    Ok(ContextFeedbackRepository::revoke_memory_if_active_in_tx(tx, item_id).await?.is_some())
                } else {
                    Ok(false)
                }
            }
            ContextFeedbackLabel::Wrong => {
                let wrong_count =
                    ContextFeedbackRepository::count_label_in_tx(tx, proof, workspace_id, item_id, "memory", "wrong")
                        .await?;
                if ContextFeedbackPolicy::should_revoke_after_label(label, wrong_count) {
                    Ok(ContextFeedbackRepository::revoke_memory_if_active_in_tx(tx, item_id).await?.is_some())
                } else {
                    Ok(false)
                }
            }
            ContextFeedbackLabel::TooSensitive => {
                ContextFeedbackRepository::mark_memory_needs_review_in_tx(tx, item_id).await?;
                Ok(true)
            }
            ContextFeedbackLabel::DoNotUseAgain => Ok(false),
        }
    }

    async fn apply_skill_feedback(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        proof: &ScopedRead,
        workspace_id: WorkspaceId,
        item_id: Uuid,
        label: ContextFeedbackLabel,
    ) -> AppResult<bool> {
        match label {
            ContextFeedbackLabel::Wrong => {
                let wrong_count =
                    ContextFeedbackRepository::count_label_in_tx(tx, proof, workspace_id, item_id, "skill", "wrong")
                        .await?;
                if ContextFeedbackPolicy::should_revoke_after_label(label, wrong_count) {
                    Ok(ContextFeedbackRepository::revoke_skill_if_active_in_tx(tx, item_id).await?.is_some())
                } else {
                    Ok(false)
                }
            }
            ContextFeedbackLabel::Stale => {
                let stale_count =
                    ContextFeedbackRepository::count_label_in_tx(tx, proof, workspace_id, item_id, "skill", "stale")
                        .await?;
                if ContextFeedbackPolicy::should_revoke_after_label(label, stale_count) {
                    Ok(ContextFeedbackRepository::revoke_skill_if_active_in_tx(tx, item_id).await?.is_some())
                } else {
                    Ok(false)
                }
            }
            ContextFeedbackLabel::Useful | ContextFeedbackLabel::TooSensitive | ContextFeedbackLabel::DoNotUseAgain => {
                Ok(false)
            }
        }
    }
}

fn candidate_summary(candidate: &ContextCandidateListRow) -> ContextCandidateSummary {
    ContextCandidateSummary {
        id: candidate.id,
        workspace_id: candidate.workspace_id,
        item_kind: candidate.item_kind.clone(),
        state: candidate.state.clone(),
        owner_user_id: candidate.owner_user_id,
        source_run_id: candidate.source_run_id,
        target_skill_id: candidate.target_skill_id,
        proposed_scope_kind: candidate.proposed_scope_kind.clone(),
        source_available: candidate.source_available,
        proposed_preview: redacted_proposal_preview(&candidate.proposed_content),
        created_at: candidate.created_at,
        updated_at: candidate.updated_at,
    }
}

fn approval_provenance(candidate: &ContextCandidate, scope: &TenantScope) -> Value {
    json!({
        "source": "context_candidate",
        "candidate_id": candidate.id,
        "source_run_id": candidate.source_run_id,
        "approved_by": scope.user_id(),
        "approved_at": Utc::now()
    })
}

fn required_workspace(scope: &TenantScope) -> AppResult<WorkspaceId> {
    scope.workspace_id().ok_or_else(|| agentforge_core::AppError::from(ErrorKind::Forbidden))
}

async fn validated_context_read(pool: &PgPool, scope: &TenantScope) -> AppResult<ScopedRead> {
    let Some(workspace_id) = scope.workspace_id() else {
        return Ok(ScopedRead::from_validated_memberships(
            scope.org_id(),
            scope.user_id(),
            std::iter::empty(),
            std::iter::empty(),
            std::iter::empty(),
        ));
    };

    let workspace_exists = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS (
               SELECT 1 FROM workspaces
                WHERE id = $1 AND organization_id = $2 AND deleted_at IS NULL
           )"#,
    )
    .bind(workspace_id.as_uuid())
    .bind(scope.org_id().as_uuid())
    .fetch_one(pool)
    .await?;
    if !workspace_exists {
        return Err(ErrorKind::NotFound(format!("workspace {workspace_id}")).into());
    }

    let team_ids = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT tm.team_id
             FROM team_members tm
             JOIN teams t ON t.id = tm.team_id
            WHERE t.organization_id = $1
              AND t.deleted_at IS NULL
              AND tm.user_id = $2"#,
    )
    .bind(scope.org_id().as_uuid())
    .bind(scope.user_id().as_uuid())
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(TeamId::from)
    .collect::<Vec<_>>();

    let project_ids = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT DISTINCT p.id
             FROM projects p
            WHERE p.organization_id = $1
              AND p.workspace_id = $2
              AND p.deleted_at IS NULL
              AND (
                  EXISTS (
                      SELECT 1 FROM project_members pm
                       WHERE pm.project_id = p.id AND pm.user_id = $3
                  )
                  OR EXISTS (
                      SELECT 1 FROM team_members tm
                       WHERE tm.team_id = p.team_id AND tm.user_id = $3
                  )
              )"#,
    )
    .bind(scope.org_id().as_uuid())
    .bind(workspace_id.as_uuid())
    .bind(scope.user_id().as_uuid())
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(ProjectId::from)
    .collect::<Vec<_>>();

    Ok(ScopedRead::from_validated_memberships(scope.org_id(), scope.user_id(), [workspace_id], team_ids, project_ids))
}

fn scoped_write_error(_err: ScopedWriteError) -> agentforge_core::AppError {
    ErrorKind::Forbidden.into()
}
