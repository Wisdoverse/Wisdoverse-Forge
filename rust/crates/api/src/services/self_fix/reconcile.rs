//! Administrator reconciliation of a changed pull-request head (#1315).
//!
//! Same serialization contract as the merge: the per-task merge mutex for the
//! whole call, and the verdict writers' task-row lock inside a budgeted
//! decision transaction. Tenant ownership is checked before any lock or
//! provider call; any provider read failure fails closed with nothing written.

use agentforge_core::{AppError, AppResult, TenantScope};
use sqlx::PgConnection;
use uuid::Uuid;

use super::SelfFixService;
use super::delivery_write;
use super::merge_lock::MergeLock;
use crate::domain::maintenance::MaintenancePullRequest;
use crate::domain::self_fix::review_status::SENSITIVE_BLOCKED;
use crate::domain::self_fix::{
    Comparison, PullRequestRead, RECONCILE_HEAD_AUDIT_ACTION, RECONCILE_HEAD_AUDIT_RESOURCE, ReconcileAction,
    ReconcileHeadInput, ReconcileHeadResult, ReconcileObservation, SelfFixPolicy, reconcile_head_action,
    reconcile_head_audit_details, reconcile_refusal,
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
        let task = OrchestrationTaskRepository::find_for_delivery_decision(self.tasks.pool(), scope, task_id).await?;
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
        let task = OrchestrationTaskRepository::find_for_delivery_decision(&mut *conn, scope, task_id).await?;
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
        // Every provider read below fails closed the same way: one coded 503
        // refusal, nothing written, safe to retry (the cause is logged only).
        let unavailable = |what: &str, err: AppError| {
            tracing::warn!(error = ?err, task_id = %task_id, read = what, "self-fix reconcile: provider read failed; refusing");
            reconcile_refusal(ReconcileAction::RefuseProviderUnavailable)
        };
        let expected_base = match &source {
            Some(src) => src.default_branch.clone(),
            None => github.repository_setup().await.map_err(|err| unavailable("repository", err))?.default_branch,
        };

        // Live reads, at most three, plus the repository read for a plain task.
        // 1. The first read: refused here if it is not open, targets another
        //    base or repository, has another head, or lacks a base commit,
        //    before any comparison.
        let status = task.review_status.as_deref();
        let stored = task.pr_head_sha.as_deref();
        let first =
            github.maintenance_pull_request(pr_number).await.map_err(|err| unavailable("first pull request", err))?;
        let read = |pr: &MaintenancePullRequest| -> bool { repository_matches && pr.base_branch == expected_base };
        let first_read = PullRequestRead {
            state: &first.state,
            base_unchanged: read(&first),
            head: &first.head_sha,
            base_sha: first.base_sha.as_deref(),
        };
        let pending = ReconcileObservation { first: first_read, comparison: Comparison::Pending, last: None };
        let base_sha = match (reconcile_head_action(status, stored, input, Some(&pending)), first_read.base_sha) {
            (ReconcileAction::Compare, Some(base_sha)) => base_sha,
            (ReconcileAction::Compare, None) => {
                return Err(reconcile_refusal(ReconcileAction::RefuseProviderUnavailable));
            }
            (refusal, _) => return Err(reconcile_refusal(refusal)),
        };
        // 2. Sensitivity from two IMMUTABLE commits: the validated base commit
        //    and the confirmed new head (not the pull request's mutable diff).
        let compared = github
            .compare_files(base_sha, &input.expected_new_head)
            .await
            .map_err(|err| unavailable("comparison", err))?;
        let comparison = if compared.complete { Comparison::Complete(&compared.paths) } else { Comparison::Capped };
        // 3. Consistency re-read after the comparison: still open, same base
        //    branch AND base commit, head still the confirmed one.
        let last =
            github.maintenance_pull_request(pr_number).await.map_err(|err| unavailable("final pull request", err))?;
        let observation = ReconcileObservation {
            first: first_read,
            comparison,
            last: Some(PullRequestRead {
                state: &last.state,
                base_unchanged: read(&last),
                head: &last.head_sha,
                base_sha: last.base_sha.as_deref(),
            }),
        };
        let action = reconcile_head_action(status, stored, input, Some(&observation));
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
