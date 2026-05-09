//! Governed memory item service.

use agentforge_core::{
    AppResult, ErrorKind, MemoryItemId, ProjectId, ScopeKind, ScopedRead, ScopedWrite, ScopedWriteError, TeamId,
    TenantScope, WorkspaceId,
};
use agentforge_db::entities::MemoryItem;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::repositories::memory::{CreateMemoryRecord, MemoryRepository, UpdateMemoryRecord};
use crate::repositories::resource_permission::ResourcePermissionRepository;
use crate::services::context_governance::{
    ContextAuditEvent, ContextGovernanceService, ContextScopeKind, ScopeExpansionRequest, Sensitivity,
};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScopeKind {
    User,
    Team,
    Project,
}

impl MemoryScopeKind {
    pub fn from_label(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "team" => Some(Self::Team),
            "project" => Some(Self::Project),
            _ => None,
        }
    }

    pub fn as_scope_kind(self) -> ScopeKind {
        match self {
            Self::User => ScopeKind::User,
            Self::Team => ScopeKind::Team,
            Self::Project => ScopeKind::Project,
        }
    }
}

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

struct PreparedContent {
    content: String,
    content_redacted: bool,
    sensitivity: &'static str,
    audit_payload: Value,
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
        self.repo.list_visible(&proof, normalize_limit(limit), offset.unwrap_or(0).max(0)).await
    }

    pub async fn get(&self, scope: &TenantScope, id: MemoryItemId) -> AppResult<MemoryItem> {
        let proof = self.validated_read(scope).await?;
        self.repo.get_visible_by_id(&proof, id).await
    }

    pub async fn read_content(&self, scope: &TenantScope, id: MemoryItemId) -> AppResult<MemoryContent> {
        let proof = self.validated_read(scope).await?;
        let mut tx = self.repo.pool().begin().await?;
        let item = MemoryRepository::lock_visible_for_update(&mut tx, &proof, id).await?;
        self.emit_memory_audit(
            &mut tx,
            scope,
            "governance.context.memory.content_read",
            json!({
                "scope_kind": item.scope_kind,
                "sensitivity": item.sensitivity,
                "content_redacted": item.content_redacted
            }),
        )
        .await?;
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
        let title = input.title.trim().to_string();
        validate_title(&title)?;
        let visibility = validate_visibility(input.visibility.as_deref())?;
        validate_ttl(input.ttl_expires_at)?;
        validate_confidence(input.confidence)?;

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
        self.emit_memory_audit(
            &mut tx,
            scope,
            "governance.context.memory.created",
            json!({
                "scope_kind": item.scope_kind,
                "visibility": item.visibility,
                "sensitivity": item.sensitivity,
                "content_redacted": item.content_redacted,
                "classification": prepared.audit_payload
            }),
        )
        .await?;
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
            validate_title(title)?;
        }
        let visibility = match input.visibility.as_deref() {
            Some(value) => Some(validate_visibility(Some(value))?),
            None => None,
        };
        validate_confidence(input.confidence)?;

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
        self.emit_memory_audit(
            &mut tx,
            scope,
            "governance.context.memory.updated",
            json!({
                "scope_kind": item.scope_kind,
                "visibility": item.visibility,
                "sensitivity": item.sensitivity,
                "content_changed": prepared.is_some(),
                "content_redacted": item.content_redacted
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(item)
    }

    pub async fn revoke(&self, scope: &TenantScope, id: MemoryItemId) -> AppResult<MemoryItem> {
        let proof = self.validated_read(scope).await?;
        let mut tx = self.repo.pool().begin().await?;
        let current = MemoryRepository::lock_visible_for_update(&mut tx, &proof, id).await?;
        self.require_owner_or_manager(scope, &current).await?;
        let item = MemoryRepository::revoke_in_tx(&mut tx, id).await?;
        self.emit_memory_audit(
            &mut tx,
            scope,
            "governance.context.memory.revoked",
            json!({
                "scope_kind": item.scope_kind,
                "sensitivity": item.sensitivity
            }),
        )
        .await?;
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
        validate_ttl(ttl_expires_at)?;
        let mut tx = self.repo.pool().begin().await?;
        let current = MemoryRepository::lock_visible_for_update(&mut tx, &proof, id).await?;
        self.require_owner_or_manager(scope, &current).await?;
        let item = MemoryRepository::extend_ttl_in_tx(&mut tx, id, ttl_expires_at).await?;
        self.emit_memory_audit(
            &mut tx,
            scope,
            "governance.context.memory.ttl_extended",
            json!({
                "scope_kind": item.scope_kind,
                "ttl_expires_at": item.ttl_expires_at
            }),
        )
        .await?;
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
        let from_kind = ContextScopeKind::from_label(&current.scope_kind)
            .ok_or_else(|| ErrorKind::Validation(format!("unsupported memory scope kind `{}`", current.scope_kind)))?;
        let to_kind = ContextScopeKind::from_scope_kind(target.kind());
        if let Err(rejection) = ContextGovernanceService::gate_scope_expansion(ScopeExpansionRequest {
            from_kind,
            to_kind,
            confirm_expansion: input.confirm_expansion,
        }) {
            self.emit_memory_audit(
                &mut tx,
                scope,
                "governance.context.memory.scope_expansion_rejected",
                json!({
                    "from_scope_kind": rejection.from_kind.as_label(),
                    "to_scope_kind": rejection.to_kind.as_label(),
                    "reason": rejection.reason.as_label(),
                    "confirm_expansion": input.confirm_expansion
                }),
            )
            .await?;
            tx.commit().await?;
            return Err(rejection.into_app_error());
        }
        if current.sensitivity == "secret_detected" && !current.content_redacted && !input.confirm_sensitive {
            return Err(ErrorKind::Unprocessable(
                "secret-detected memory requires explicit redaction before scope change".into(),
            )
            .into());
        }
        let item = MemoryRepository::reclassify_scope_in_tx(&mut tx, id, &target).await?;
        self.emit_memory_audit(
            &mut tx,
            scope,
            "governance.context.memory.reclassified",
            json!({
                "from_scope_kind": current.scope_kind,
                "to_scope_kind": item.scope_kind,
                "sensitivity": item.sensitivity,
                "content_redacted": item.content_redacted
            }),
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
        let scope_kind = scope_kind.as_scope_kind();
        let scope_id = match scope_kind {
            ScopeKind::User => scope_id.unwrap_or_else(|| proof.user_id().as_uuid()),
            ScopeKind::Team | ScopeKind::Project => scope_id.ok_or_else(|| {
                ErrorKind::Validation(format!("scope_id is required for {} memory", scope_kind.as_label()))
            })?,
        };
        let write = ScopedWrite::try_new(scope_kind, scope_id, proof.clone()).map_err(scoped_write_error)?;
        if !self.repo.resource_belongs_to_scope(proof, scope_kind, scope_id, workspace_id).await? {
            return Err(ErrorKind::Forbidden.into());
        }
        Ok(write)
    }

    async fn require_owner_or_manager(&self, scope: &TenantScope, item: &MemoryItem) -> AppResult<()> {
        if item.owner_user_id == scope.user_id() {
            return Ok(());
        }
        match MemoryScopeKind::from_label(&item.scope_kind) {
            Some(MemoryScopeKind::Team) => {
                if self.permissions.can_manage_team(scope, TeamId::from(item.scope_id)).await? {
                    return Ok(());
                }
            }
            Some(MemoryScopeKind::Project) => {
                if self.permissions.can_manage_project(scope, ProjectId::from(item.scope_id)).await? {
                    return Ok(());
                }
            }
            Some(MemoryScopeKind::User) | None => {}
        }
        Err(ErrorKind::Forbidden.into())
    }

    async fn prepare_content_or_audit_rejection(
        &self,
        scope: &TenantScope,
        operation: &str,
        content: &str,
        redacted: bool,
    ) -> AppResult<PreparedContent> {
        let content = content.trim();
        if content.is_empty() {
            return Err(ErrorKind::Validation("memory content must not be empty".into()).into());
        }
        let classification = ContextGovernanceService::classify_sensitivity(content);
        let matched_patterns = classification.matched_patterns.clone();
        let redacted_preview = classification.redacted_preview.clone();
        if matches!(classification.sensitivity, Sensitivity::SecretDetected) && !redacted {
            let mut tx = self.repo.pool().begin().await?;
            self.emit_memory_audit(
                &mut tx,
                scope,
                "governance.context.memory.rejected",
                json!({
                    "operation": operation,
                    "reason": "secret_detected",
                    "matched_patterns": matched_patterns,
                    "redacted_preview": redacted_preview
                }),
            )
            .await?;
            tx.commit().await?;
            return Err(
                ErrorKind::Unprocessable("secret detected in memory content; submit redacted content".into()).into()
            );
        }

        let content_redacted = matches!(classification.sensitivity, Sensitivity::SecretDetected);
        let stored_content = if content_redacted {
            redacted_preview.clone().unwrap_or_else(|| "[REDACTED]".to_string())
        } else {
            content.to_string()
        };
        let sensitivity = sensitivity_label(classification.sensitivity);
        Ok(PreparedContent {
            content: stored_content,
            content_redacted,
            sensitivity,
            audit_payload: json!({
                "sensitivity": sensitivity,
                "matched_patterns": matched_patterns,
                "redacted": content_redacted
            }),
        })
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

fn normalize_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

fn validate_title(title: &str) -> AppResult<&str> {
    let title = title.trim();
    if title.is_empty() || title.len() > 255 {
        return Err(ErrorKind::Validation("memory title must be 1-255 characters".into()).into());
    }
    Ok(title)
}

fn validate_visibility(visibility: Option<&str>) -> AppResult<&str> {
    match visibility.unwrap_or("shared") {
        "private" => Ok("private"),
        "shared" => Ok("shared"),
        other => Err(ErrorKind::Validation(format!("unsupported memory visibility `{other}`")).into()),
    }
}

fn validate_ttl(ttl_expires_at: Option<DateTime<Utc>>) -> AppResult<()> {
    if let Some(ttl) = ttl_expires_at
        && ttl <= Utc::now()
    {
        return Err(ErrorKind::Validation("ttl_expires_at must be in the future".into()).into());
    }
    Ok(())
}

fn validate_confidence(confidence: Option<f64>) -> AppResult<()> {
    if let Some(value) = confidence
        && !(0.0..=1.0).contains(&value)
    {
        return Err(ErrorKind::Validation("confidence must be between 0 and 1".into()).into());
    }
    Ok(())
}

fn required_workspace(scope: &TenantScope) -> AppResult<WorkspaceId> {
    scope.workspace_id().ok_or_else(|| agentforge_core::AppError::from(ErrorKind::Forbidden))
}

fn scoped_write_error(_err: ScopedWriteError) -> agentforge_core::AppError {
    ErrorKind::Forbidden.into()
}

fn sensitivity_label(sensitivity: Sensitivity) -> &'static str {
    match sensitivity {
        Sensitivity::Public => "public",
        Sensitivity::Internal => "internal",
        Sensitivity::Confidential => "confidential",
        Sensitivity::SecretDetected => "secret_detected",
    }
}
