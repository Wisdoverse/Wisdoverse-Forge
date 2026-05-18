//! Governed memory item service.

use agentforge_core::{
    AppResult, ErrorKind, MemoryItemId, ProjectId, ScopedRead, ScopedWrite, ScopedWriteError, TeamId, TenantScope,
    WorkspaceId,
};
use agentforge_db::entities::MemoryItem;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::context_governance::ContextAuditEvent;
use crate::domain::memory::{
    MemoryConfidencePolicy, MemoryContentDecision, MemoryContentPolicy, MemoryContentReadAudit, MemoryCreatedAudit,
    MemoryListPage, MemoryMutationAccess, MemoryMutationAccessPolicy, MemoryMutationManagerCheck,
    MemoryReclassificationPlan, MemoryReclassificationPolicy, MemoryReclassificationRequest, MemoryRevokedAudit,
    MemoryScopeKind, MemoryScopeTargetPolicy, MemoryTitle, MemoryTtlExtendedAudit, MemoryTtlPolicy, MemoryUpdatedAudit,
    MemoryVisibility, PreparedMemoryContent,
};
use crate::repositories::memory::{CreateMemoryRecord, MemoryRepository, UpdateMemoryRecord};
use crate::repositories::resource_permission::ResourcePermissionRepository;
use crate::services::context_governance::ContextGovernanceService;

#[derive(Debug, Clone)]
pub struct CreateMemoryInput {
    pub title: String,
    pub content: String,
    pub redacted: bool,
    pub scope_kind: MemoryScopeKind,
    pub scope_id: Option<Uuid>,
    pub source_task_id: Option<Uuid>,
    pub source_run_id: Option<Uuid>,
    pub provenance: Option<Value>,
    pub visibility: Option<String>,
    pub ttl_expires_at: Option<DateTime<Utc>>,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateMemoryInput {
    pub title: Option<String>,
    pub content: Option<String>,
    pub redacted: bool,
    pub provenance: Option<Value>,
    pub visibility: Option<String>,
    pub confidence: Option<f64>,
    pub last_verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct ReclassifyScopeInput {
    pub scope_kind: MemoryScopeKind,
    pub scope_id: Option<Uuid>,
    pub confirm_sensitive: bool,
    pub confirm_expansion: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryContent {
    pub id: MemoryItemId,
    pub content: String,
    pub content_redacted: bool,
    pub sensitivity: String,
}

pub struct MemoryService {
    repo: MemoryRepository,
    permissions: ResourcePermissionRepository,
}

impl MemoryService {
    pub fn new(pool: PgPool) -> Self {
        Self { repo: MemoryRepository::new(pool.clone()), permissions: ResourcePermissionRepository::new(pool) }
    }

    pub async fn list(
        &self,
        scope: &TenantScope,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> AppResult<Vec<MemoryItem>> {
        let proof = self.validated_read(scope).await?;
        let page = MemoryListPage::new(limit, offset);
        self.repo.list_visible(&proof, page.limit(), page.offset()).await
    }

    pub async fn get(&self, scope: &TenantScope, id: MemoryItemId) -> AppResult<MemoryItem> {
        let proof = self.validated_read(scope).await?;
        self.repo.get_visible_by_id(&proof, id).await
    }

    pub async fn read_content(&self, scope: &TenantScope, id: MemoryItemId) -> AppResult<MemoryContent> {
        let proof = self.validated_read(scope).await?;
        let mut tx = self.repo.pool().begin().await?;
        let item = MemoryRepository::lock_visible_for_update(&mut tx, &proof, id).await?;
        let audit =
            MemoryContentReadAudit::new(item.scope_kind.as_str(), item.sensitivity.as_str(), item.content_redacted);
        self.emit_memory_audit(&mut tx, scope, audit.audit_action(), audit.audit_payload()).await?;
        tx.commit().await?;

        Ok(MemoryContent {
            id: item.id,
            content: item.content,
            content_redacted: item.content_redacted,
            sensitivity: item.sensitivity,
        })
    }

    pub async fn create(&self, scope: &TenantScope, input: CreateMemoryInput) -> AppResult<MemoryItem> {
        let proof = self.validated_read(scope).await?;
        let workspace_id = required_workspace(scope)?;
        let target = self.validated_write_scope(&proof, workspace_id, input.scope_kind, input.scope_id).await?;
        let title = MemoryTitle::parse(&input.title)?.value().to_string();
        let visibility = MemoryVisibility::parse(input.visibility.as_deref())?.as_str();
        MemoryTtlPolicy::validate(input.ttl_expires_at, Utc::now())?;
        MemoryConfidencePolicy::validate(input.confidence)?;

        let prepared = self.prepare_content_or_audit_rejection(scope, "create", &input.content, input.redacted).await?;
        let provenance = input.provenance.unwrap_or_else(|| json!({}));

        let mut tx = self.repo.pool().begin().await?;
        let item = MemoryRepository::create_in_tx(
            &mut tx,
            &proof,
            CreateMemoryRecord {
                workspace_id,
                write_scope: &target,
                owner_user_id: scope.user_id().as_uuid(),
                source_task_id: input.source_task_id,
                source_run_id: input.source_run_id,
                title: &title,
                content: &prepared.content,
                content_redacted: prepared.content_redacted,
                visibility,
                sensitivity: prepared.sensitivity,
                provenance: &provenance,
                ttl_expires_at: input.ttl_expires_at,
                confidence: input.confidence,
                state: "active",
            },
        )
        .await?;
        let audit = MemoryCreatedAudit::new(
            item.scope_kind.as_str(),
            item.visibility.as_str(),
            item.sensitivity.as_str(),
            item.content_redacted,
            prepared.audit_payload,
        );
        self.emit_memory_audit(&mut tx, scope, audit.audit_action(), audit.audit_payload()).await?;
        tx.commit().await?;
        Ok(item)
    }

    pub async fn update(
        &self,
        scope: &TenantScope,
        id: MemoryItemId,
        input: UpdateMemoryInput,
    ) -> AppResult<MemoryItem> {
        let proof = self.validated_read(scope).await?;
        if let Some(title) = input.title.as_deref() {
            MemoryTitle::parse(title)?;
        }
        let visibility = match input.visibility.as_deref() {
            Some(value) => Some(MemoryVisibility::parse(Some(value))?.as_str()),
            None => None,
        };
        MemoryConfidencePolicy::validate(input.confidence)?;

        let prepared = match input.content.as_deref() {
            Some(content) => {
                Some(self.prepare_content_or_audit_rejection(scope, "update", content, input.redacted).await?)
            }
            None => None,
        };

        let mut tx = self.repo.pool().begin().await?;
        let current = MemoryRepository::lock_visible_for_update(&mut tx, &proof, id).await?;
        self.require_owner_or_manager(scope, &current).await?;
        let item = MemoryRepository::update_in_tx(
            &mut tx,
            id,
            UpdateMemoryRecord {
                title: input.title.as_deref(),
                content: prepared.as_ref().map(|value| value.content.as_str()),
                content_redacted: prepared.as_ref().map(|value| value.content_redacted),
                sensitivity: prepared.as_ref().map(|value| value.sensitivity),
                provenance: input.provenance.as_ref(),
                visibility,
                confidence: input.confidence,
                last_verified_at: input.last_verified_at,
            },
        )
        .await?;
        let audit = MemoryUpdatedAudit::new(
            item.scope_kind.as_str(),
            item.visibility.as_str(),
            item.sensitivity.as_str(),
            prepared.is_some(),
            item.content_redacted,
        );
        self.emit_memory_audit(&mut tx, scope, audit.audit_action(), audit.audit_payload()).await?;
        tx.commit().await?;
        Ok(item)
    }

    pub async fn revoke(&self, scope: &TenantScope, id: MemoryItemId) -> AppResult<MemoryItem> {
        let proof = self.validated_read(scope).await?;
        let mut tx = self.repo.pool().begin().await?;
        let current = MemoryRepository::lock_visible_for_update(&mut tx, &proof, id).await?;
        self.require_owner_or_manager(scope, &current).await?;
        let item = MemoryRepository::revoke_in_tx(&mut tx, id).await?;
        let audit = MemoryRevokedAudit::new(item.scope_kind.as_str(), item.sensitivity.as_str());
        self.emit_memory_audit(&mut tx, scope, audit.audit_action(), audit.audit_payload()).await?;
        tx.commit().await?;
        Ok(item)
    }

    pub async fn extend_ttl(
        &self,
        scope: &TenantScope,
        id: MemoryItemId,
        ttl_expires_at: Option<DateTime<Utc>>,
    ) -> AppResult<MemoryItem> {
        let proof = self.validated_read(scope).await?;
        MemoryTtlPolicy::validate(ttl_expires_at, Utc::now())?;
        let mut tx = self.repo.pool().begin().await?;
        let current = MemoryRepository::lock_visible_for_update(&mut tx, &proof, id).await?;
        self.require_owner_or_manager(scope, &current).await?;
        let item = MemoryRepository::extend_ttl_in_tx(&mut tx, id, ttl_expires_at).await?;
        let audit = MemoryTtlExtendedAudit::new(item.scope_kind.as_str(), item.ttl_expires_at);
        self.emit_memory_audit(&mut tx, scope, audit.audit_action(), audit.audit_payload()).await?;
        tx.commit().await?;
        Ok(item)
    }

    pub async fn reclassify_scope(
        &self,
        scope: &TenantScope,
        id: MemoryItemId,
        input: ReclassifyScopeInput,
    ) -> AppResult<MemoryItem> {
        let proof = self.validated_read(scope).await?;
        let workspace_id = required_workspace(scope)?;
        let target = self.validated_write_scope(&proof, workspace_id, input.scope_kind, input.scope_id).await?;
        let mut tx = self.repo.pool().begin().await?;
        let current = MemoryRepository::lock_visible_for_update(&mut tx, &proof, id).await?;
        self.require_owner_or_manager(scope, &current).await?;

        let decision = match MemoryReclassificationPolicy::plan(MemoryReclassificationRequest {
            current_scope_kind: &current.scope_kind,
            target_scope_kind: target.kind(),
            sensitivity: &current.sensitivity,
            content_redacted: current.content_redacted,
            confirm_sensitive: input.confirm_sensitive,
            confirm_expansion: input.confirm_expansion,
        })? {
            MemoryReclassificationPlan::Approved(decision) => decision,
            MemoryReclassificationPlan::Rejected(rejection) => {
                self.emit_memory_audit(&mut tx, scope, rejection.audit_action(), rejection.audit_payload()).await?;
                tx.commit().await?;
                return Err(rejection.into_app_error());
            }
        };

        let item = MemoryRepository::reclassify_scope_in_tx(&mut tx, id, &target).await?;
        self.emit_memory_audit(
            &mut tx,
            scope,
            "governance.context.memory.reclassified",
            decision.audit_payload(&item.sensitivity, item.content_redacted),
        )
        .await?;
        tx.commit().await?;
        Ok(item)
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
        proof: &ScopedRead,
        workspace_id: WorkspaceId,
        scope_kind: MemoryScopeKind,
        scope_id: Option<Uuid>,
    ) -> AppResult<ScopedWrite> {
        let (scope_kind, scope_id) = MemoryScopeTargetPolicy::resolve(scope_kind, scope_id, proof.user_id().as_uuid())?;
        let write = ScopedWrite::try_new(scope_kind, scope_id, proof.clone()).map_err(scoped_write_error)?;
        if !self.repo.resource_belongs_to_scope(proof, scope_kind, scope_id, workspace_id).await? {
            return Err(ErrorKind::Forbidden.into());
        }
        Ok(write)
    }

    async fn require_owner_or_manager(&self, scope: &TenantScope, item: &MemoryItem) -> AppResult<()> {
        match MemoryMutationAccessPolicy::plan(
            item.owner_user_id.as_uuid(),
            scope.user_id().as_uuid(),
            &item.scope_kind,
            item.scope_id,
        ) {
            MemoryMutationAccess::Allowed => Ok(()),
            MemoryMutationAccess::RequiresManager(MemoryMutationManagerCheck::Team(team_id)) => {
                let can_manage = self.permissions.can_manage_team(scope, TeamId::from(team_id)).await?;
                MemoryMutationAccessPolicy::ensure_manager_authorized(can_manage)
            }
            MemoryMutationAccess::RequiresManager(MemoryMutationManagerCheck::Project(project_id)) => {
                let can_manage = self.permissions.can_manage_project(scope, ProjectId::from(project_id)).await?;
                MemoryMutationAccessPolicy::ensure_manager_authorized(can_manage)
            }
            MemoryMutationAccess::Forbidden => Err(ErrorKind::Forbidden.into()),
        }
    }

    async fn prepare_content_or_audit_rejection(
        &self,
        scope: &TenantScope,
        operation: &str,
        content: &str,
        redacted: bool,
    ) -> AppResult<PreparedMemoryContent> {
        match MemoryContentPolicy::prepare(content, redacted)? {
            MemoryContentDecision::Prepared(prepared) => Ok(prepared),
            MemoryContentDecision::Rejected(rejection) => {
                let action = rejection.audit_action();
                let payload = rejection.audit_payload(operation);
                let mut tx = self.repo.pool().begin().await?;
                self.emit_memory_audit(&mut tx, scope, action, payload).await?;
                tx.commit().await?;
                Err(rejection.into_app_error())
            }
        }
    }

    async fn emit_memory_audit(
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
                resource_type: "memory_item",
                // The current audit route is org-wide. Avoid writing raw memory
                // item IDs until scope-aware audit projection lands.
                resource_id: None,
                payload,
                ip_address: None,
            },
        )
        .await?;
        Ok(())
    }
}

fn required_workspace(scope: &TenantScope) -> AppResult<WorkspaceId> {
    scope.workspace_id().ok_or_else(|| agentforge_core::AppError::from(ErrorKind::Forbidden))
}

fn scoped_write_error(_err: ScopedWriteError) -> agentforge_core::AppError {
    ErrorKind::Forbidden.into()
}
