//! Context approval queue service.

use std::sync::Arc;

use agentforge_core::{AppResult, ScopedRead, ScopedWrite, TenantScope, WorkspaceId};
use agentforge_db::entities::ContextCandidate;
use agentforge_infra::NatsClient;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

pub(crate) use crate::domain::context::context_data_response;
pub use crate::domain::context::{ContextApprovalOutcome, ContextCandidateSummary, ContextFeedbackOutcome};
use crate::domain::context::{
    ContextApprovalProvenance, ContextCandidateApprovalAudit, ContextCandidateBroadcast,
    ContextCandidateBroadcastEvent, ContextCandidateCreatedAudit, ContextCandidateKind,
    ContextCandidateManualRejectionAudit, ContextCandidatePolicy, ContextCandidateRecord, ContextFeedbackLabel,
    ContextFeedbackPolicy, ContextFeedbackRecordedAudit, ContextItemKind, ContextTenantPolicy,
    context_candidate_audit_event, context_candidate_summary, ensure_pending_candidate,
    normalize_candidate_kind_filter, normalize_candidate_state_filter, normalize_context_candidate_limit,
    normalize_feedback_note, normalize_reason, normalize_scope_kind_filter, validate_context_sensitivity, validate_ttl,
};
use crate::domain::memory::MemoryScopeKind;
use crate::repositories::context_candidate::{
    ContextApprovalRepository, ContextCandidateListRow, ContextCandidateRepository, ContextFeedbackRepository,
    CreateContextApprovalRecord, CreateContextCandidateRecord, CreateContextFeedbackRecord,
};
use crate::repositories::memory::{CreateMemoryRecord, MemoryRepository};
use crate::repositories::resource::permission::ResourcePermissionRepository;
use crate::repositories::skill::{SkillRepository, SkillVersionRepository};
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

pub struct ContextApprovalService {
    candidates: ContextCandidateRepository,
    memory: MemoryRepository,
    permissions: ResourcePermissionRepository,
    nats: Option<Arc<NatsClient>>,
}

impl ContextApprovalService {
    pub fn new(pool: PgPool, nats: Option<Arc<NatsClient>>) -> Self {
        Self {
            candidates: ContextCandidateRepository::new(pool.clone()),
            memory: MemoryRepository::new(pool.clone()),
            permissions: ResourcePermissionRepository::new(pool.clone()),
            nats,
        }
    }

    pub fn from_runtime(pool: PgPool, nats: Arc<NatsClient>) -> Self {
        Self::new(pool, Some(nats))
    }

    pub async fn create_candidate(
        &self,
        scope: &TenantScope,
        input: CreateContextCandidateInput,
    ) -> AppResult<ContextCandidate> {
        let workspace_id = ContextTenantPolicy::required_workspace(scope)?;
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
        let audit = ContextCandidateCreatedAudit::new(
            candidate.workspace_id,
            candidate.source_run_id.is_some(),
            candidate.target_skill_id.is_some(),
        );
        self.emit_candidate_audit(&mut tx, scope, audit.audit_action(), audit.audit_payload(&candidate.item_kind))
            .await?;
        tx.commit().await?;
        self.publish_candidate_event(scope, &candidate, ContextCandidateBroadcastEvent::Created, None).await;
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
        Ok(candidates.into_iter().map(ContextCandidateRecord::from).map(context_candidate_summary).collect())
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
        let workspace_id = ContextTenantPolicy::required_workspace(scope)?;
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
            self.publish_candidate_event(scope, &rejected, ContextCandidateBroadcastEvent::Rejected, None).await;
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
                let provenance =
                    ContextApprovalProvenance::for_approval(candidate.id, candidate.source_run_id, scope.user_id())
                        .into_json();
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
                let audit = ContextCandidateApprovalAudit::memory(
                    item.scope_kind.as_str(),
                    item.sensitivity.as_str(),
                    approval.self_approval,
                    prepared.classification_payload,
                );
                self.emit_candidate_audit(
                    &mut tx,
                    scope,
                    audit.audit_action(),
                    audit.audit_payload(&candidate.item_kind),
                )
                .await?;
                tx.commit().await?;
                self.publish_candidate_event(
                    scope,
                    &updated,
                    ContextCandidateBroadcastEvent::Approved,
                    Some((&item.scope_kind, item.scope_id)),
                )
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
                let audit = ContextCandidateApprovalAudit::skill(
                    skill.scope_kind.clone(),
                    skill.sensitivity.as_str(),
                    approval.self_approval,
                    current.version,
                    skill.version,
                    prior_version.id,
                );
                self.emit_candidate_audit(
                    &mut tx,
                    scope,
                    audit.audit_action(),
                    audit.audit_payload(&candidate.item_kind),
                )
                .await?;
                tx.commit().await?;
                if let (Some(scope_kind), Some(scope_id)) = (skill.scope_kind.as_deref(), skill.scope_id) {
                    self.publish_candidate_event(
                        scope,
                        &updated,
                        ContextCandidateBroadcastEvent::Approved,
                        Some((scope_kind, scope_id)),
                    )
                    .await;
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
        self.publish_candidate_event(scope, &updated, ContextCandidateBroadcastEvent::Rejected, None).await;
        Ok(ContextApprovalOutcome { candidate: updated, approval: Some(approval), memory_item: None, skill: None })
    }

    async fn validated_read(&self, scope: &TenantScope) -> AppResult<ScopedRead> {
        self.permissions.validated_read_scope(scope).await
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
        let write = ScopedWrite::try_new(scope_kind, scope_id, proof.clone())
            .map_err(ContextTenantPolicy::scoped_write_error)?;
        ContextTenantPolicy::ensure_resource_belongs_to_scope(
            self.memory.resource_belongs_to_scope(proof, scope_kind, scope_id, workspace_id).await?,
        )?;
        Ok(write)
    }

    async fn emit_candidate_audit(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        scope: &TenantScope,
        action: &'static str,
        payload: Value,
    ) -> AppResult<()> {
        ContextGovernanceService::emit_audit(tx, scope, context_candidate_audit_event(action, payload)).await?;
        Ok(())
    }

    async fn publish_candidate_event(
        &self,
        scope: &TenantScope,
        candidate: &ContextCandidate,
        event: ContextCandidateBroadcastEvent,
        approved_scope: Option<(&str, Uuid)>,
    ) {
        let Some(nats) = &self.nats else {
            return;
        };
        let (scope_kind, scope_id) = approved_scope.unwrap_or(("user", candidate.owner_user_id.as_uuid()));
        let broadcast = ContextCandidateBroadcast::new(
            event,
            candidate.id,
            candidate.item_kind.as_str(),
            candidate.state.as_str(),
            scope_kind,
            scope_id,
        );
        let subject = broadcast.subject(scope.org_id().as_uuid());
        let payload = broadcast.payload();
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

pub struct ContextFeedbackService {
    feedback: ContextFeedbackRepository,
    permissions: ResourcePermissionRepository,
}

impl ContextFeedbackService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            feedback: ContextFeedbackRepository::new(pool.clone()),
            permissions: ResourcePermissionRepository::new(pool),
        }
    }

    pub async fn record(
        &self,
        scope: &TenantScope,
        input: RecordContextFeedbackInput,
    ) -> AppResult<ContextFeedbackOutcome> {
        let workspace_id = ContextTenantPolicy::required_workspace(scope)?;
        let proof = self.permissions.validated_read_scope(scope).await?;
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

        let audit = ContextFeedbackRecordedAudit::new(
            feedback.run_id,
            feedback.item_id,
            feedback.item_kind.as_str(),
            feedback.label.as_str(),
            item_state_changed,
        );
        ContextGovernanceService::emit_audit(&mut tx, scope, audit.audit_event(feedback.id)).await?;

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

impl From<ContextCandidateListRow> for ContextCandidateRecord {
    fn from(row: ContextCandidateListRow) -> Self {
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            item_kind: row.item_kind,
            state: row.state,
            owner_user_id: row.owner_user_id,
            source_run_id: row.source_run_id,
            target_skill_id: row.target_skill_id,
            proposed_scope_kind: row.proposed_scope_kind,
            source_available: row.source_available,
            proposed_content: row.proposed_content,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
