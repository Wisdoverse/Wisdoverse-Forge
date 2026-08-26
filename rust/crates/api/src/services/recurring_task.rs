//! Recurring task service — validated schedule CRUD and the due-run worker.

use agentforge_core::{AppResult, TenantScope};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::orchestration::TaskCreationPolicy;
pub use crate::domain::recurring_task::{CreateRecurringTaskInput, RecurringTaskView, UpdateRecurringTaskInput};
use crate::domain::recurring_task::{
    RecurringTaskPolicy, audit_created_payload, audit_deleted_payload, audit_enabled_payload, recurring_task_not_found,
};
use crate::repositories::orchestration::{CreateTaskRow, OrchestrationTaskRepository};
use crate::repositories::recurring_task::RecurringTaskRepository;
use crate::services::audit::AuditService;

/// Business logic layer for recurring tasks.
pub struct RecurringTaskService {
    repo: RecurringTaskRepository,
    task_repo: OrchestrationTaskRepository,
    audit: AuditService,
}

impl RecurringTaskService {
    pub fn new(repo: RecurringTaskRepository, task_repo: OrchestrationTaskRepository, audit: AuditService) -> Self {
        Self { repo, task_repo, audit }
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self::new(
            RecurringTaskRepository::new(pool.clone()),
            OrchestrationTaskRepository::new(pool.clone()),
            AuditService::from_pool(pool),
        )
    }

    /// List the team space schedules, newest first.
    pub async fn list(&self, scope: &TenantScope) -> AppResult<Vec<RecurringTaskView>> {
        Ok(self.repo.list(scope).await?.into_iter().map(to_view).collect())
    }

    /// Validate and save a schedule, then audit the change.
    pub async fn create(&self, scope: &TenantScope, input: &CreateRecurringTaskInput) -> AppResult<RecurringTaskView> {
        RecurringTaskPolicy::validate(
            &input.name,
            &input.title,
            &input.description,
            &input.priority,
            input.cadence_minutes,
        )?;
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
                input.group_id,
                input.cadence_minutes,
            )
            .await?;
        let _ = self
            .audit
            .log_action(
                scope.org_id(),
                Some(scope.user_id()),
                "recurring_task.created",
                "recurring_task",
                Some(row.id),
                &audit_created_payload(&row.name, row.cadence_minutes),
                None,
            )
            .await;
        Ok(to_view(row))
    }

    /// Delete a schedule (audited).
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<bool> {
        if !self.repo.delete(scope, id).await? {
            return Err(recurring_task_not_found(id).into());
        }
        let _ = self
            .audit
            .log_action(
                scope.org_id(),
                Some(scope.user_id()),
                "recurring_task.deleted",
                "recurring_task",
                Some(id),
                &audit_deleted_payload(id),
                None,
            )
            .await;
        Ok(true)
    }

    /// Enable or disable a schedule (audited).
    pub async fn set_enabled(&self, scope: &TenantScope, id: Uuid, enabled: bool) -> AppResult<RecurringTaskView> {
        let row = self.repo.set_enabled(scope, id, enabled).await?.ok_or_else(|| recurring_task_not_found(id))?;
        let _ = self
            .audit
            .log_action(
                scope.org_id(),
                Some(scope.user_id()),
                if enabled { "recurring_task.enabled" } else { "recurring_task.disabled" },
                "recurring_task",
                Some(id),
                &audit_enabled_payload(enabled),
                None,
            )
            .await;
        Ok(to_view(row))
    }

    /// Runner sweep: claim due schedules and create one task per schedule.
    /// Unassigned by design — the next available agent starts each run.
    /// Returns how many tasks were created.
    pub async fn run_due(&self) -> AppResult<i64> {
        let claimed = self.repo.claim_due(100).await?;
        let mut created = 0;
        for row in claimed {
            let scope = TenantScope::with_axes(row.organization_id, row.created_by, None, None, None);
            let initial = TaskCreationPolicy::initial_unassigned_state(&[], row.requires_approval, None, &[]);
            match self
                .task_repo
                .create(
                    &scope,
                    CreateTaskRow {
                        group_id: Some(row.group_id),
                        title: &row.title,
                        description: Some(&row.description),
                        priority: &row.priority,
                        params: None,
                        assigned_agent_id: None,
                        parent_task_id: None,
                        initial_status: initial.initial_status,
                        initial_blocked_reason: initial.initial_blocked_reason,
                        initial_blocked_metadata: initial.initial_blocked_metadata,
                        requires_approval: row.requires_approval,
                        self_fix: false,
                    },
                )
                .await
            {
                Ok(_task) => created += 1,
                Err(err) => {
                    tracing::error!(recurring_task_id = %row.id, error = %err, "recurring task run failed");
                }
            }
        }
        Ok(created)
    }
}

/// Persistence-free view of a recurring_tasks row (row adapter lives here).
fn to_view(row: agentforge_db::entities::RecurringTask) -> RecurringTaskView {
    RecurringTaskView {
        id: row.id,
        name: row.name,
        title: row.title,
        description: row.description,
        priority: row.priority,
        requires_approval: row.requires_approval,
        project_id: row.project_id,
        group_id: row.group_id,
        cadence_minutes: row.cadence_minutes,
        next_run_at: row.next_run_at,
        enabled: row.enabled,
        created_at: row.created_at,
    }
}
