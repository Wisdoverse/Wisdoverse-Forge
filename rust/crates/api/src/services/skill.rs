//! Skill service — validation and governance management.

use agentforge_core::{AppResult, ErrorKind, ProjectId, ScopedRead, TeamId, TenantScope, WorkspaceId};
use agentforge_db::entities::{Skill, SkillVersion};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::context_governance::ContextAuditEvent;
use crate::domain::skill::{
    PreparedSkillContent, SkillBoundaryMutationPolicy, SkillContentDecision, SkillContentPolicy,
    SkillCreateStatePolicy, SkillJsonObjectPolicy, SkillMutationAccess, SkillMutationAccessPolicy,
    SkillMutationManagerCheck, SkillMutationPolicy, SkillName, SkillRestoreVersionPlan, SkillRestoreVersionPolicy,
    SkillRestoreVersionRequest, SkillScopeKind, SkillScopeTargetPolicy, SkillSensitivity, SkillState,
    SkillStateTransitionPolicy, SkillTtlPolicy,
};
use crate::repositories::resource_permission::ResourcePermissionRepository;
use crate::repositories::skill::{CreateSkillRecord, SkillRepository, UpdateSkillRecord};
use crate::repositories::skill_version::SkillVersionRepository;
use crate::services::context_governance::ContextGovernanceService;

#[derive(Debug, Clone)]
pub struct CreateSkillInput {
    pub name: String,
    pub description: Option<String>,
    pub trigger_pattern: Option<String>,
    pub negative_trigger: Option<String>,
    pub content: String,
    pub scope_kind: SkillScopeKind,
    pub scope_id: Option<Uuid>,
    pub state: Option<SkillState>,
    pub sensitivity: Option<String>,
    pub provenance: Option<Value>,
    pub required_inputs: Option<Value>,
    pub tools: Option<Value>,
    pub examples: Option<Value>,
    pub success_evidence: Option<Value>,
    pub ttl_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateSkillInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub trigger_pattern: Option<String>,
    pub content: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct RestoreSkillVersionInput {
    pub version: i32,
    pub expected_current_version: Option<i32>,
    pub confirm_expansion: bool,
}

/// Business logic layer for skill operations.
pub struct SkillService {
    repo: SkillRepository,
    permissions: ResourcePermissionRepository,
}

impl SkillService {
    pub fn new(repo: SkillRepository) -> Self {
        let permissions = ResourcePermissionRepository::new(repo.pool().clone());
        Self { repo, permissions }
    }

    /// List all active visible skills for the request scope.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<Skill>> {
        let proof = self.validated_read(scope).await?;
        self.repo.list_visible(&proof).await
    }

    /// Get an active visible skill by ID.
    pub async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<Skill> {
        let proof = self.validated_read(scope).await?;
        self.repo.get_visible_by_id(&proof, id).await
    }

    /// Create a new governed skill.
    pub async fn create(&self, scope: &TenantScope, input: CreateSkillInput) -> AppResult<Skill> {
        let workspace_id = required_workspace(scope)?;
        let name = SkillName::parse(&input.name)?.value();
        let content = self.prepare_content_or_audit_rejection(scope, "create", None, &input.content).await?;
        let target_scope_id = self.validated_write_scope(scope, workspace_id, input.scope_kind, input.scope_id).await?;
        let state = self.validated_create_state(scope, input.scope_kind, target_scope_id, input.state).await?;
        SkillTtlPolicy::validate(input.ttl_expires_at, Utc::now())?;
        let requested_sensitivity =
            input.sensitivity.as_deref().map(SkillSensitivity::parse).transpose()?.map(SkillSensitivity::as_str);
        let sensitivity = requested_sensitivity.unwrap_or(content.sensitivity);
        let provenance = input.provenance.unwrap_or_else(|| json!({}));
        SkillJsonObjectPolicy::validate("provenance", &provenance)?;
        let required_inputs = input.required_inputs.unwrap_or_else(|| json!([]));
        let tools = input.tools.unwrap_or_else(|| json!([]));
        let examples = input.examples.unwrap_or_else(|| json!([]));
        let success_evidence = input.success_evidence.unwrap_or_else(|| json!([]));
        let state_label = state.as_label();

        let mut tx = self.repo.pool().begin().await?;
        let skill = SkillRepository::create_in_tx(
            &mut tx,
            scope,
            CreateSkillRecord {
                workspace_id,
                scope_kind: input.scope_kind.as_label(),
                scope_id: target_scope_id,
                owner_user_id: scope.user_id().as_uuid(),
                name,
                description: input.description.as_deref(),
                trigger_pattern: input.trigger_pattern.as_deref(),
                negative_trigger: input.negative_trigger.as_deref(),
                content: &content.content,
                enabled: state == SkillState::Active,
                state: state_label,
                sensitivity,
                provenance: &provenance,
                required_inputs: &required_inputs,
                tools: &tools,
                examples: &examples,
                success_evidence: &success_evidence,
                ttl_expires_at: input.ttl_expires_at,
            },
        )
        .await?;
        self.emit_skill_audit(
            &mut tx,
            scope,
            "governance.context.skill.created",
            Some(skill.id.as_uuid()),
            skill_event_payload(
                &skill,
                json!({
                    "sensitivity": skill.sensitivity,
                    "classification": content.audit_payload
                }),
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(skill)
    }

    /// Update a skill and append the prior state to `skill_versions`.
    pub async fn update(&self, scope: &TenantScope, id: Uuid, input: UpdateSkillInput) -> AppResult<Skill> {
        if let Some(name) = input.name.as_deref() {
            SkillName::parse(name)?;
        }
        let prepared = match input.content.as_deref() {
            Some(content) => Some(self.prepare_content_or_audit_rejection(scope, "update", Some(id), content).await?),
            None => None,
        };

        self.reject_outside_boundary_mutation(scope, id, "update").await?;
        let mut tx = self.repo.pool().begin().await?;
        let current = self.lock_mutable_skill(&mut tx, scope, id, "update").await?;
        SkillMutationPolicy::ensure_updateable(id, &current.state)?;
        self.require_owner_or_manager(scope, &current).await?;
        let state_change = SkillStateTransitionPolicy::next(&current.state, input.enabled)?;
        let prior_version = SkillVersionRepository::insert_snapshot_in_tx(&mut tx, &current, scope.user_id()).await?;
        let skill = SkillRepository::update_in_tx(
            &mut tx,
            id,
            UpdateSkillRecord {
                name: input.name.as_deref(),
                description: input.description.as_deref(),
                trigger_pattern: input.trigger_pattern.as_deref(),
                content: prepared.as_ref().map(|value| value.content.as_str()),
                enabled: state_change.enabled(),
                state: state_change.state(),
                sensitivity: prepared.as_ref().map(|value| value.sensitivity),
            },
        )
        .await?;
        self.emit_skill_audit(
            &mut tx,
            scope,
            "governance.context.skill.updated",
            Some(skill.id.as_uuid()),
            skill_event_payload(
                &skill,
                json!({
                    "content_changed": prepared.is_some(),
                    "from_version": current.version,
                    "resulting_version": skill.version,
                    "skill_version_id": prior_version.id,
                    "classification": prepared.as_ref().map(|value| value.audit_payload.clone())
                }),
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(skill)
    }

    /// Soft-delete a skill by revoking it. Governance evidence stays in the DB.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.reject_outside_boundary_mutation(scope, id, "revoke").await?;
        let mut tx = self.repo.pool().begin().await?;
        let current = self.lock_mutable_skill(&mut tx, scope, id, "revoke").await?;
        SkillMutationPolicy::ensure_revokeable(id, &current.state)?;
        self.require_owner_or_manager(scope, &current).await?;
        let prior_version = SkillVersionRepository::insert_snapshot_in_tx(&mut tx, &current, scope.user_id()).await?;
        let skill = SkillRepository::revoke_in_tx(&mut tx, id).await?;
        self.emit_skill_audit(
            &mut tx,
            scope,
            "governance.context.skill.revoked",
            Some(skill.id.as_uuid()),
            skill_event_payload(
                &skill,
                json!({
                    "from_version": current.version,
                    "resulting_version": skill.version,
                    "skill_version_id": prior_version.id
                }),
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// List version snapshots for a skill inside the caller's workspace boundary.
    pub async fn list_versions(&self, scope: &TenantScope, id: Uuid) -> AppResult<Vec<SkillVersion>> {
        self.reject_outside_boundary_version_access(scope, id).await?;
        let mut tx = self.repo.pool().begin().await?;
        let current = self.lock_mutable_skill(&mut tx, scope, id, "list_versions").await?;
        self.require_owner_or_manager(scope, &current).await?;
        let versions = SkillVersionRepository::list_by_skill_in_tx(&mut tx, current.id).await?;
        tx.commit().await?;
        Ok(versions)
    }

    /// Restore a historical skill snapshot in place, creating a new current version.
    pub async fn restore_version(
        &self,
        scope: &TenantScope,
        id: Uuid,
        input: RestoreSkillVersionInput,
    ) -> AppResult<Skill> {
        SkillRestoreVersionPolicy::validate(input.version, input.expected_current_version)?;

        self.reject_outside_boundary_mutation(scope, id, "restore_version").await?;
        let mut tx = self.repo.pool().begin().await?;
        let current = self.lock_mutable_skill(&mut tx, scope, id, "restore_version").await?;
        SkillRestoreVersionPolicy::ensure_current_restorable(id, &current.state)?;
        SkillRestoreVersionPolicy::ensure_expected_current_version(
            id,
            current.version,
            input.expected_current_version,
        )?;
        self.require_owner_or_manager(scope, &current).await?;

        let (_target_row, snapshot) =
            SkillVersionRepository::snapshot_for_version_in_tx(&mut tx, current.id, input.version).await?;
        SkillRestoreVersionPolicy::ensure_snapshot_boundary(
            id,
            input.version,
            current.organization_id,
            current.workspace_id,
            snapshot.organization_id,
            snapshot.workspace_id,
        )?;
        SkillRestoreVersionPolicy::ensure_snapshot_restorable(id, input.version, &snapshot.state)?;
        match SkillRestoreVersionPolicy::plan_restore(SkillRestoreVersionRequest {
            skill_id: id,
            target_version: input.version,
            current_scope_kind: current.scope_kind.as_deref(),
            snapshot_scope_kind: snapshot.scope_kind.as_deref(),
            snapshot_sensitivity: &snapshot.sensitivity,
            snapshot_content: &snapshot.content,
            confirm_expansion: input.confirm_expansion,
        })? {
            SkillRestoreVersionPlan::Approved => {}
            SkillRestoreVersionPlan::Rejected(rejection) => {
                self.emit_skill_audit(&mut tx, scope, rejection.audit_action(), Some(id), rejection.audit_payload())
                    .await?;
                tx.commit().await?;
                return Err(rejection.into_app_error());
            }
        }

        let pre_restore_version =
            SkillVersionRepository::insert_snapshot_in_tx(&mut tx, &current, scope.user_id()).await?;
        let resulting_version = current.version + 1;
        let skill = SkillRepository::restore_from_snapshot_in_tx(&mut tx, id, &snapshot, resulting_version).await?;
        self.emit_skill_audit(
            &mut tx,
            scope,
            "governance.context.skill.restored",
            Some(skill.id.as_uuid()),
            skill_event_payload(
                &skill,
                json!({
                    "target_version": input.version,
                    "from_version": current.version,
                    "resulting_version": skill.version,
                    "skill_version_id": pre_restore_version.id
                }),
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(skill)
    }

    async fn validated_read(&self, scope: &TenantScope) -> AppResult<ScopedRead> {
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
        .fetch_one(self.repo.pool())
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
        .fetch_all(self.repo.pool())
        .await?
        .into_iter()
        .map(TeamId::from)
        .collect::<Vec<_>>();

        let project_ids = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT DISTINCT p.id
                 FROM projects p
                WHERE p.organization_id = $1
                  AND p.deleted_at IS NULL
                  AND p.workspace_id = $3
                  AND (
                      EXISTS (
                          SELECT 1 FROM project_members pm
                           WHERE pm.project_id = p.id AND pm.user_id = $2
                      )
                      OR EXISTS (
                          SELECT 1 FROM team_members tm
                           WHERE tm.team_id = p.team_id AND tm.user_id = $2
                      )
                  )"#,
        )
        .bind(scope.org_id().as_uuid())
        .bind(scope.user_id().as_uuid())
        .bind(workspace_id.as_uuid())
        .fetch_all(self.repo.pool())
        .await?
        .into_iter()
        .map(ProjectId::from)
        .collect::<Vec<_>>();

        Ok(ScopedRead::from_validated_memberships(
            scope.org_id(),
            scope.user_id(),
            [workspace_id],
            team_ids,
            project_ids,
        ))
    }

    async fn validated_write_scope(
        &self,
        scope: &TenantScope,
        workspace_id: WorkspaceId,
        scope_kind: SkillScopeKind,
        scope_id: Option<Uuid>,
    ) -> AppResult<Uuid> {
        let target_scope_id =
            SkillScopeTargetPolicy::resolve(scope_kind, scope_id, scope.org_id().as_uuid(), scope.user_id().as_uuid())?;
        if !self.repo.resource_belongs_to_scope(scope, workspace_id, scope_kind.as_label(), target_scope_id).await? {
            return Err(ErrorKind::Forbidden.into());
        }
        Ok(target_scope_id)
    }

    async fn validated_create_state(
        &self,
        scope: &TenantScope,
        scope_kind: SkillScopeKind,
        scope_id: Uuid,
        state: Option<SkillState>,
    ) -> AppResult<SkillState> {
        let wants_active = state.unwrap_or(SkillState::Active) == SkillState::Active;
        let can_publish_active = !wants_active || self.can_publish_active(scope, scope_kind, scope_id).await?;
        SkillCreateStatePolicy::resolve(state, can_publish_active)
    }

    async fn can_publish_active(
        &self,
        scope: &TenantScope,
        scope_kind: SkillScopeKind,
        scope_id: Uuid,
    ) -> AppResult<bool> {
        match scope_kind {
            SkillScopeKind::User => Ok(scope_id == scope.user_id().as_uuid()),
            SkillScopeKind::Org => self.permissions.can_manage_org(scope).await,
            SkillScopeKind::Team => self.permissions.can_manage_team(scope, TeamId::from(scope_id)).await,
            SkillScopeKind::Project => self.permissions.can_manage_project(scope, ProjectId::from(scope_id)).await,
        }
    }

    async fn require_owner_or_manager(&self, scope: &TenantScope, skill: &Skill) -> AppResult<()> {
        match SkillMutationAccessPolicy::plan(
            skill.owner_user_id.map(|owner| owner.as_uuid()),
            scope.user_id().as_uuid(),
            skill.scope_kind.as_deref(),
            skill.scope_id,
        ) {
            SkillMutationAccess::Allowed => Ok(()),
            SkillMutationAccess::RequiresManager(SkillMutationManagerCheck::Org) => {
                let can_manage = self.permissions.can_manage_org(scope).await?;
                SkillMutationAccessPolicy::ensure_manager_authorized(can_manage)
            }
            SkillMutationAccess::RequiresManager(SkillMutationManagerCheck::Team(team_id)) => {
                let can_manage = self.permissions.can_manage_team(scope, TeamId::from(team_id)).await?;
                SkillMutationAccessPolicy::ensure_manager_authorized(can_manage)
            }
            SkillMutationAccess::RequiresManager(SkillMutationManagerCheck::Project(project_id)) => {
                let can_manage = self.permissions.can_manage_project(scope, ProjectId::from(project_id)).await?;
                SkillMutationAccessPolicy::ensure_manager_authorized(can_manage)
            }
            SkillMutationAccess::Forbidden => Err(ErrorKind::Forbidden.into()),
        }
    }

    async fn lock_mutable_skill(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        scope: &TenantScope,
        id: Uuid,
        _operation: &'static str,
    ) -> AppResult<Skill> {
        SkillRepository::lock_org_skill_for_update(tx, scope, id).await
    }

    async fn reject_outside_boundary_mutation(
        &self,
        scope: &TenantScope,
        id: Uuid,
        operation: &'static str,
    ) -> AppResult<()> {
        let exists_outside_request_boundary = self.repo.exists_outside_request_boundary(scope, id).await?;
        if let Some(rejection) =
            SkillBoundaryMutationPolicy::plan(exists_outside_request_boundary, operation, id, scope.workspace_id())
        {
            let mut tx = self.repo.pool().begin().await?;
            self.emit_skill_audit(&mut tx, scope, rejection.audit_action(), Some(id), rejection.audit_payload())
                .await?;
            tx.commit().await?;
            return Err(rejection.into_app_error());
        }
        Ok(())
    }

    async fn reject_outside_boundary_version_access(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        if self.repo.exists_outside_request_boundary(scope, id).await? {
            return Err(ErrorKind::Forbidden.into());
        }
        Ok(())
    }

    async fn prepare_content_or_audit_rejection(
        &self,
        scope: &TenantScope,
        operation: &'static str,
        resource_id: Option<Uuid>,
        content: &str,
    ) -> AppResult<PreparedSkillContent> {
        match SkillContentPolicy::prepare(content)? {
            SkillContentDecision::Prepared(prepared) => Ok(prepared),
            SkillContentDecision::Rejected(rejection) => {
                let action = rejection.audit_action();
                let payload = rejection.audit_payload(operation, resource_id);
                let mut tx = self.repo.pool().begin().await?;
                self.emit_skill_audit(&mut tx, scope, action, resource_id, payload).await?;
                tx.commit().await?;
                Err(rejection.into_app_error())
            }
        }
    }

    async fn emit_skill_audit(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        scope: &TenantScope,
        action: &'static str,
        resource_id: Option<Uuid>,
        payload: Value,
    ) -> AppResult<()> {
        ContextGovernanceService::emit_audit(
            tx,
            scope,
            ContextAuditEvent { action, resource_type: "skill", resource_id, payload, ip_address: None },
        )
        .await?;
        Ok(())
    }
}

fn required_workspace(scope: &TenantScope) -> AppResult<WorkspaceId> {
    scope.workspace_id().ok_or_else(|| agentforge_core::AppError::from(ErrorKind::Forbidden))
}

fn skill_event_payload(skill: &Skill, extra: Value) -> Value {
    let mut payload = json!({
        "skill_id": skill.id,
        "workspace_id": skill.workspace_id,
        "scope_kind": skill.scope_kind,
        "scope_id": skill.scope_id,
        "state": skill.state,
        "version": skill.version
    });

    if let (Some(base), Some(extra)) = (payload.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }

    payload
}
