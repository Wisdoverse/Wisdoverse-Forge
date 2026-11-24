//! Administrator reconciliation of a changed pull-request head (#1315).
//!
//! Same serialization contract as the merge: the per-task merge mutex for the
//! whole call, and the verdict writers' task-row lock inside a budgeted
//! decision transaction. Tenant ownership is checked before any lock or
//! provider call; any provider read failure fails closed with nothing written.

use agentforge_core::{AppResult, TenantScope};
use sqlx::PgConnection;
use uuid::Uuid;

use super::SelfFixService;
use super::delivery_write;
use super::merge_lock::MergeLock;
use crate::domain::self_fix::review_status::SENSITIVE_BLOCKED;
use crate::domain::self_fix::{
    RECONCILE_HEAD_AUDIT_ACTION, RECONCILE_HEAD_AUDIT_RESOURCE, ReconcileAction, ReconcileHeadInput,
    ReconcileHeadResult, ReconcileObservation, SelfFixPolicy, reconcile_head_action, reconcile_head_audit_details,
    reconcile_refusal,
};
use crate::repositories::audit::AuditRepository;
use crate::repositories::orchestration::{MaintenanceRequestRepository, OrchestrationTaskRepository};
use crate::services::github_app::GithubAppClient;

impl SelfFixService {
    /// Adopt the pull request's new head as the task's current revision.
    pub(crate) async fn reconcile_head(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        input: ReconcileHeadInput,
    ) -> AppResult<ReconcileHeadResult> {
        self.require_platform_admin(scope).await?;
        let input = input.prepare()?;
        // Tenant-scoped: another tenant gets 404 here, before any lock or provider call.
        let task = self.tasks.find_by_id(scope, task_id).await?;
        if !task.self_fix {
            return Err(SelfFixPolicy::not_a_self_fix_task());
        }
        let pr_number = task.pr_number.ok_or_else(SelfFixPolicy::no_pr_to_merge)?;
        let github = self.github.as_ref().ok_or_else(SelfFixPolicy::github_not_configured)?;
        let Some(mut lock) = MergeLock::try_acquire(self.tasks.pool(), scope, task_id).await? else {
            return Err(SelfFixPolicy::merge_attempt_running());
        };
        let result = Self::reconcile_head_locked(lock.conn(), github, scope, task_id, pr_number, &input).await;
        lock.release().await;
        result
    }

    /// Body of [`Self::reconcile_head`] under the merge mutex, with every
    /// query on the mutex's connection.
    async fn reconcile_head_locked(
        conn: &mut PgConnection,
        github: &GithubAppClient,
        scope: &TenantScope,
        task_id: Uuid,
        pr_number: i32,
        input: &ReconcileHeadInput,
    ) -> AppResult<ReconcileHeadResult> {
        let task = OrchestrationTaskRepository::find_by_id_on(conn, scope, task_id).await?;
        match reconcile_head_action(task.review_status.as_deref(), task.pr_head_sha.as_deref(), input, None) {
            ReconcileAction::Observe => {}
            ReconcileAction::NoOp => {
                return Ok(ReconcileHeadResult {
                    task_id,
                    previous_head: input.expected_new_head.clone(),
                    current_head: input.expected_new_head.clone(),
                    changed: false,
                    sensitive: task.review_status.as_deref() == Some(SENSITIVE_BLOCKED),
                    review_status: task.review_status,
                });
            }
            refusal => return Err(reconcile_refusal(refusal)),
        }

        // The base the task was opened for: the maintenance source's recorded
        // default branch, else (plain self-fix tasks record no base branch)
        // the configured repository's default branch. The configured repository
        // identity is checked before any provider call.
        let source = MaintenanceRequestRepository::find_by_task_on(conn, scope, task_id).await?;
        let repository_matches =
            source.as_ref().is_none_or(|src| src.repository.eq_ignore_ascii_case(github.repository_slug()));
        let expected_base = match &source {
            Some(src) => src.default_branch.clone(),
            None => github.repository_setup().await?.default_branch,
        };

        // Live reads. A failed pull-request read fails closed. A failed or
        // truncated file listing counts as sensitive. The pull request is read
        // again after the listing so the listing describes the observed head.
        let before = github.maintenance_pull_request(pr_number).await?;
        let files = match github.pull_request_files(pr_number).await {
            Ok(listing) if listing.complete => Some(listing.paths),
            Ok(_) => None,
            Err(err) => {
                tracing::warn!(error = ?err, task_id = %task_id, "self-fix reconcile: file listing failed; treating the head as sensitive");
                None
            }
        };
        let after = github.maintenance_pull_request(pr_number).await?;
        let observation = ReconcileObservation {
            state: &after.state,
            base_unchanged: repository_matches && after.base_branch == expected_base,
            head: &after.head_sha,
            changed_files: files.as_deref().filter(|_| before.head_sha == after.head_sha),
        };
        let action = reconcile_head_action(
            task.review_status.as_deref(),
            task.pr_head_sha.as_deref(),
            input,
            Some(&observation),
        );
        if !matches!(action, ReconcileAction::Adopt { .. }) {
            return Err(reconcile_refusal(action));
        }

        let mut tx = Self::begin_decision(conn).await?;
        let decided = Self::adopt_head_in_tx(&mut tx, scope, task_id, pr_number, input, &observation).await;
        Self::finish_decision(tx, decided).await?.ok_or_else(SelfFixPolicy::merge_attempt_running)
    }

    /// The reconcile decision transaction body, between `begin_decision` and
    /// `finish_decision`: the verdict writers' row lock (with the live
    /// administrator re-check), a fresh decision on the locked row, the head
    /// write, and the audit record.
    async fn adopt_head_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        scope: &TenantScope,
        task_id: Uuid,
        pr_number: i32,
        input: &ReconcileHeadInput,
        observation: &ReconcileObservation<'_>,
    ) -> AppResult<ReconcileHeadResult> {
        let current = delivery_write::lock_task(tx, scope, task_id).await?;
        let action = reconcile_head_action(
            current.review_status.as_deref(),
            current.pr_head_sha.as_deref(),
            input,
            Some(observation),
        );
        let ReconcileAction::Adopt { sensitive, block } = action else {
            return Err(reconcile_refusal(action));
        };
        OrchestrationTaskRepository::set_pr_head_sha_on(tx, scope, task_id, &input.expected_new_head).await?;
        let review_status = if block {
            OrchestrationTaskRepository::set_review_status_on(tx, scope, task_id, SENSITIVE_BLOCKED).await?;
            Some(SENSITIVE_BLOCKED.to_string())
        } else {
            current.review_status
        };
        let details =
            reconcile_head_audit_details(&input.expected_stored_head, &input.expected_new_head, pr_number, sensitive);
        AuditRepository::create_in_tx(
            tx,
            scope.org_id(),
            Some(scope.user_id()),
            RECONCILE_HEAD_AUDIT_ACTION,
            RECONCILE_HEAD_AUDIT_RESOURCE,
            Some(task_id),
            &details,
            None,
        )
        .await?;
        Ok(ReconcileHeadResult {
            task_id,
            previous_head: input.expected_stored_head.clone(),
            current_head: input.expected_new_head.clone(),
            changed: true,
            sensitive,
            review_status,
        })
    }
}
