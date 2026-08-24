//! Task template service — org-scoped CRUD with validation and audit.

use agentforge_core::{AppResult, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

pub use crate::domain::task_template::{CreateTaskTemplateInput, TaskTemplateView};
use crate::domain::task_template::{
    TaskTemplatePolicy, audit_named_payload, template_delete_forbidden, template_not_found,
};
use crate::repositories::task_template::TaskTemplateRepository;
use crate::services::audit::AuditService;

/// Business logic layer for task templates.
pub struct TaskTemplateService {
    repo: TaskTemplateRepository,
    audit: AuditService,
}

impl TaskTemplateService {
    pub fn new(repo: TaskTemplateRepository, audit: AuditService) -> Self {
        Self { repo, audit }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(TaskTemplateRepository::new(pool.clone()), AuditService::from_pool(pool))
    }

    /// List the team space's saved task templates, newest first. Optional
    /// project filter keeps team-wide templates alongside the project's own.
    pub async fn list(&self, scope: &TenantScope, project_id: Option<Uuid>) -> AppResult<Vec<TaskTemplateView>> {
        let rows = self.repo.list(scope, project_id).await?;
        Ok(rows.into_iter().map(to_view).collect())
    }

    /// Validate and save a new template, then audit the change.
    pub async fn create(&self, scope: &TenantScope, input: &CreateTaskTemplateInput) -> AppResult<TaskTemplateView> {
        TaskTemplatePolicy::validate(&input.name, &input.title, &input.description, &input.priority)?;
        let row = self
            .repo
            .create(
                scope,
                &input.name,
                &input.title,
                &input.description,
                &input.priority,
                input.requires_approval,
                input.project_id,
                scope.user_id(),
            )
            .await?;
        let _ = self
            .audit
            .log_action(
                scope.org_id(),
                Some(scope.user_id()),
                "task_template.created",
                "task_template",
                Some(row.id),
                &audit_named_payload(&row.name),
                None,
            )
            .await;
        Ok(to_view(row))
    }

    /// Delete a template when the caller is its creator or an owner/admin.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid, role: &str) -> AppResult<bool> {
        let row = self.repo.find_by_id(scope, id).await?;
        let Some(row) = row else {
            return Err(template_not_found(id).into());
        };
        if !TaskTemplatePolicy::delete_allowed(role, &row.created_by, &scope.user_id()) {
            return Err(template_delete_forbidden().into());
        }
        self.repo.delete(scope, id).await?;
        let _ = self
            .audit
            .log_action(
                scope.org_id(),
                Some(scope.user_id()),
                "task_template.deleted",
                "task_template",
                Some(id),
                &audit_named_payload(&row.name),
                None,
            )
            .await;
        Ok(true)
    }
}

fn to_view(row: agentforge_db::entities::TaskTemplate) -> TaskTemplateView {
    TaskTemplateView::from_row(
        row.id,
        row.name,
        row.title,
        row.description,
        row.priority,
        row.requires_approval,
        row.project_id,
        row.created_by,
        row.created_at,
    )
}
