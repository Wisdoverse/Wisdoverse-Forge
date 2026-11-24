//! Server-side self-fix loop: PR Bridge (import + rebuild + draft PR) and Merge Executor.
//! All privileged git runs in a server-owned clean clone; /workspace is read as plain files.

// `pub(crate)` in production; widened to `pub` under `test-support` so the
// `crate::testing::self_fix_rebuild` re-export can reach these from integration
// tests. The items themselves stay `pub` either way, so production callers see
// no change in reachability (the module gate is what scopes them).
#[cfg(not(any(test, feature = "test-support")))]
pub(crate) mod import;
#[cfg(any(test, feature = "test-support"))]
pub mod import;

#[cfg(not(any(test, feature = "test-support")))]
pub(crate) mod rebuild;
#[cfg(any(test, feature = "test-support"))]
pub mod rebuild;

#[cfg(not(any(test, feature = "test-support")))]
pub(crate) mod bridge;
#[cfg(any(test, feature = "test-support"))]
pub mod bridge;

#[cfg(not(any(test, feature = "test-support")))]
pub(crate) mod merge_executor;
#[cfg(any(test, feature = "test-support"))]
pub mod merge_executor;

mod delivery;
mod delivery_outcomes;
mod delivery_write;
mod maintenance;
mod merge_lock;

pub(crate) use crate::domain::maintenance::MaintenanceRequestInput;
pub(crate) use crate::domain::maintenance_delivery::{
    ComparisonQuery, DecisionInput, HandoffInput, OutcomeQuery, VerificationInput,
};
pub mod metrics;

use std::path::PathBuf;
use std::sync::Arc;

use agentforge_core::{AgentId, AppResult, TenantScope};
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

use crate::domain::admin::AdminRolePolicy;
use crate::domain::agent_workspace::{WorkspaceMountScope, host_path_for_container_cwd};
use crate::domain::maintenance_delivery::{ensure_merge_not_rejected, merge_blocked_by_verdict};
pub use crate::domain::self_fix::SelfFixRepositorySetup;
use crate::domain::self_fix::review_status::{APPROVED, CHANGES_REQUESTED, IN_REVIEW, MERGED, SENSITIVE_BLOCKED};
use crate::domain::self_fix::{
    MergeEntry, MergeEntryAction, SelfFixMergeMetricPolicy, SelfFixMergeResult, SelfFixPolicy, SelfFixReview,
};
use crate::repositories::agent::AgentRepository;
use crate::repositories::orchestration::maintenance_delivery::MaintenanceDeliveryRepository;
use crate::repositories::orchestration::{
    MaintenanceRequestRepository, OrchestrationTaskRepository, TaskRunRepository,
};
use crate::repositories::user::UserRepository;
use crate::services::agent_container_control::AgentContainerControlService;
use crate::services::agent_workspace::resolve_agent_workspace_paths;
use crate::services::github_app::GithubAppClient;
use crate::services::self_fix::bridge::{GitProvider, SelfFixPrOutcome, run_pr_bridge};
use crate::services::self_fix::import::ImportLimits;
use crate::services::self_fix::merge_executor::{MergeRequest, pull_request_state, run_merge_executor};
use crate::services::self_fix::merge_lock::MergeLock;

/// Server-side self-fix PR Bridge service.
///
/// Holds the task + agent repositories, the (optional) GitHub App client, the
/// agent-container control service (used to freeze the agent before the PR is
/// built), the managed workspace root, the import limits, and the configurable
/// per-task merge-attempt cap.
pub(crate) struct SelfFixService {
    tasks: OrchestrationTaskRepository,
    maintenance_requests: MaintenanceRequestRepository,
    delivery: MaintenanceDeliveryRepository,
    runs: TaskRunRepository,
    agents: AgentRepository,
    users: UserRepository,
    container_control: AgentContainerControlService,
    github: Option<Arc<GithubAppClient>>,
    workspace_root: String,
    limits: ImportLimits,
    max_merge_attempts: i32,
}

impl SelfFixService {
    pub(crate) fn new(
        tasks: OrchestrationTaskRepository,
        agents: AgentRepository,
        users: UserRepository,
        container_control: AgentContainerControlService,
        github: Option<GithubAppClient>,
        workspace_root: String,
        limits: ImportLimits,
        max_merge_attempts: i32,
    ) -> Self {
        let maintenance_requests = MaintenanceRequestRepository::new(tasks.pool().clone());
        let runs = TaskRunRepository::new(tasks.pool().clone());
        let delivery = MaintenanceDeliveryRepository::new(tasks.pool().clone());
        Self {
            tasks,
            maintenance_requests,
            delivery,
            runs,
            agents,
            users,
            container_control,
            github: github.map(Arc::new),
            workspace_root,
            limits,
            max_merge_attempts,
        }
    }

    /// Open a self-fix draft PR for `task_id`.
    ///
    /// Flow: require self-fix + a configured GitHub App → best-effort freeze the
    /// agent container → resolve the host workspace dir → pin `base_sha` →
    /// clone + rebuild + force-push + open draft PR (in [`run_pr_bridge`]) →
    /// persist the base SHA, PR metadata, and review status. On ANY failure after
    /// the base SHA is pinned, the task is left with a visible error and no
    /// half-written PR. The branch name is deterministic and the push uses
    /// `--force`, so a retry that rebuilds a new sibling commit will succeed.
    pub(crate) async fn open_pr(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<SelfFixPrOutcome> {
        // 1. Load the task; require it is a self-fix task and GitHub is configured.
        let task = self.tasks.find_by_id(scope, task_id).await?;
        if !task.self_fix {
            return Err(SelfFixPolicy::not_a_self_fix_task());
        }
        let github = self.github.as_ref().ok_or_else(SelfFixPolicy::github_not_configured)?;

        // Check repository access before stopping the agent or touching git.
        let repository = GitProvider::repository_setup(github.as_ref()).await?;

        // 2. Best-effort TOCTOU freeze: stop the agent container so it cannot
        //    keep editing /workspace while we snapshot it. The agent has
        //    finished, so a stop failure must NOT abort the PR.
        if let Some(agent_id) = task.assigned_agent_id
            && let Err(err) = self.container_control.stop(scope, agent_id).await
        {
            tracing::warn!(
                error = ?err,
                task_id = %task_id,
                agent_id = %agent_id,
                "self-fix: best-effort stop of agent container failed; continuing (agent already finished)"
            );
        }

        // 3. Resolve the server-visible host workspace project dir for the task's
        //    agent. Reject (visible error) if it escapes the managed root.
        let workspace_project_dir = self.resolve_workspace_project_dir(scope, &task).await?;

        // 4. Persist the base revision observed with the default branch above.
        self.tasks.set_base_commit_sha(scope, task_id, &repository.base_sha).await?;

        // 5-10. Clone + rebuild + sensitive-check + push + open draft PR.
        let commit_message = format!("self-fix: {}", task.title);
        let pr_title = format!("[self-fix] {}", task.title);
        let pr_body = pr_body(&task.title, task.description.as_deref(), task_id);
        let result = run_pr_bridge(
            github.as_ref(),
            task_id,
            &repository,
            &workspace_project_dir,
            &commit_message,
            &pr_title,
            &pr_body,
            &self.limits,
        )
        .await?;

        // 11. Persist PR metadata + the selected review status.
        self.tasks
            .set_pr_metadata(
                scope,
                task_id,
                result.pr.number,
                &result.pr.html_url,
                &result.pr.head_sha,
                result.review_status,
            )
            .await?;

        // Realtime note: this persist flips `review_status` to `in_review` (or
        // `sensitive_blocked`). When this method is wired to the self-fix loop,
        // the caller — which holds `AppState` — should mirror the approve route
        // and emit `orchestration_service().broadcast_task_update_by_id(scope,
        // task_id, "self_fix.pr_opened")` so the Review tab appears live. The
        // broadcast is deliberately NOT emitted here: `SelfFixService` owns no
        // NATS handle (DDD layering), and the orchestration service is the owner
        // of the task-update broadcast contract.

        // 12. Return the outcome.
        Ok(SelfFixPrOutcome {
            pr_number: result.pr.number,
            pr_url: result.pr.html_url,
            review_status: result.review_status,
        })
    }

    /// Guarded server-side merge of an approved self-fix PR.
    ///
    /// Accepts `in_review` and `approved` (a started, unconfirmed merge). The
    /// merge decision commits `approved` before any GitHub call. We
    /// also re-derive sensitivity SERVER-SIDE — a task persisted as
    /// `sensitive_blocked` at PR-open time is HARD-refused here regardless of any
    /// later GitHub state, and an already-`merged` task short-circuits as a no-op.
    ///
    /// Flow: admin check → tenant-scoped load → per-task merge mutex → fresh
    /// load → `merged`/`sensitive_blocked` short-circuits → entry decision
    /// (`SelfFixPolicy::merge_entry_action`: pull-request state for `approved`,
    /// settle window, attempt cap) → locked decision transaction (rejection
    /// gate, attempt bump, `approved`) → executor → `merged`; after a definite
    /// refusal the status follows a fresh pull-request read; any other failure
    /// stays `approved`.
    /// Platform-admin gate for the self-fix merge path. Reads the caller's live
    /// `users.is_admin` flag and refuses non-admins with `Forbidden`. Keeps
    /// `is_admin` a server-side DB read (never trusting the JWT role claim),
    /// mirroring `AdminService::require_platform_admin`.
    async fn require_platform_admin(&self, scope: &TenantScope) -> AppResult<()> {
        let is_admin = self.users.find_is_admin_by_id(scope.user_id()).await?;
        AdminRolePolicy::require_platform_admin(is_admin)
    }

    /// Deployment-level repository setup is visible only to a live platform
    /// admin. No task or GitHub write is needed to check it.
    pub(crate) async fn repository_setup(&self, scope: &TenantScope) -> AppResult<SelfFixRepositorySetup> {
        self.require_platform_admin(scope).await?;
        self.github.as_ref().ok_or_else(SelfFixPolicy::github_not_configured)?.repository_setup().await
    }

    pub(crate) async fn approve_and_merge(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        approver_id: &str,
    ) -> AppResult<SelfFixMergeResult> {
        // 0. Elevated-privilege gate: merging agent-authored changes to the
        //    shared default branch is platform-admin only, verified against the
        //    server-side `users.is_admin` (never a self-assignable JWT role claim).
        self.require_platform_admin(scope).await?;

        // 1. Load the task (tenant-scoped, so another tenant gets 404 before any
        //    lock or provider call); require self-fix.
        let task = self.tasks.find_by_id(scope, task_id).await?;
        if !task.self_fix {
            return Err(SelfFixPolicy::not_a_self_fix_task());
        }

        // 2. Per-task merge mutex for the rest of the call (#1314): at most one
        //    attempt runs the entry decision, the decision transaction and the
        //    executor. All of its SQL runs on the lock's own connection.
        let Some(mut lock) = MergeLock::try_acquire(self.tasks.pool(), scope, task_id).await? else {
            metrics::record_merge_outcome("running");
            return Err(SelfFixPolicy::merge_attempt_running());
        };
        let result = self.approve_and_merge_locked(lock.conn(), scope, task_id, approver_id).await;
        lock.release().await;
        result
    }

    /// Body of [`Self::approve_and_merge`], run while holding the merge mutex,
    /// with every query on the mutex's connection.
    async fn approve_and_merge_locked(
        &self,
        conn: &mut PgConnection,
        scope: &TenantScope,
        task_id: Uuid,
        approver_id: &str,
    ) -> AppResult<SelfFixMergeResult> {
        // a. Fresh read under the mutex.
        let task = OrchestrationTaskRepository::find_by_id_on(conn, scope, task_id).await?;

        // Require a PR linkage (number + recorded head SHA): the executor merges
        // an EXISTING PR, it does not open one.
        let pr_number = task.pr_number.ok_or_else(SelfFixPolicy::no_pr_to_merge)?;
        let recorded_head_sha = task.pr_head_sha.as_deref().ok_or_else(SelfFixPolicy::no_pr_to_merge)?;
        let already_merged =
            || SelfFixMergeResult { pr_number, merged_head_sha: recorded_head_sha.to_string(), already_merged: true };

        // b. Gate on review status + recompute sensitivity SERVER-SIDE.
        //    - `merged`           → idempotent success (already done).
        //    - `sensitive_blocked`→ HARD refuse, no GitHub call.
        //    - `in_review`        → the platform-admin's approve action merges
        //                           directly; step d moves it to `approved`.
        //    - `approved`         → a merge was started but not confirmed (an
        //                           ambiguous failure, a crash or a cancelled
        //                           request): read the pull request first.
        let entry = match task.review_status.as_deref() {
            Some(MERGED) => {
                metrics::record_merge_outcome("already_merged");
                return Ok(already_merged());
            }
            Some(SENSITIVE_BLOCKED) => {
                metrics::record_merge_outcome("sensitive_blocked");
                return Err(SelfFixPolicy::sensitive_path_blocked());
            }
            Some(IN_REVIEW) => MergeEntry::InReview,
            Some(APPROVED) => MergeEntry::Approved,
            _ => return Err(SelfFixPolicy::not_approved_for_merge()),
        };

        // c. Entry decision (one pure domain function): the provider's actual
        //    pull-request state for an unconfirmed attempt, the settle window
        //    since the task row's last write, and the attempt cap. The cap is
        //    checked BEFORE the GitHub requirement for `in_review`, so a capped
        //    task is refused regardless of GitHub configuration.
        let pull_request = match entry {
            MergeEntry::Approved => {
                let github = self.github.as_ref().ok_or_else(SelfFixPolicy::github_not_configured)?;
                match pull_request_state(github.as_ref(), pr_number).await {
                    Ok(state) => Some(state),
                    Err(err) => {
                        metrics::record_merge_failure(&err);
                        return Err(err);
                    }
                }
            }
            MergeEntry::InReview => None,
        };
        let since_write = (chrono::Utc::now() - task.updated_at).num_seconds();
        let at_cap = task.merge_attempts >= self.max_merge_attempts;
        match SelfFixPolicy::merge_entry_action(entry, pull_request, since_write, at_cap) {
            MergeEntryAction::MarkMerged => {
                metrics::record_merge_outcome("already_merged");
                OrchestrationTaskRepository::set_review_status_on(conn, scope, task_id, MERGED).await?;
                return Ok(already_merged());
            }
            MergeEntryAction::MarkClosed => {
                metrics::record_merge_outcome("failed");
                OrchestrationTaskRepository::set_review_status_on(conn, scope, task_id, CHANGES_REQUESTED).await?;
                return Err(crate::domain::github_app::not_mergeable());
            }
            MergeEntryAction::WaitUnconfirmed => {
                metrics::record_merge_outcome("unconfirmed");
                return Err(SelfFixPolicy::merge_unconfirmed());
            }
            MergeEntryAction::ExhaustedUnconfirmed => {
                metrics::record_merge_outcome("exhausted");
                return Err(SelfFixPolicy::merge_attempts_exhausted_unconfirmed());
            }
            MergeEntryAction::Exhausted => {
                OrchestrationTaskRepository::set_review_status_on(conn, scope, task_id, CHANGES_REQUESTED).await?;
                metrics::record_merge_outcome("exhausted");
                return Err(SelfFixPolicy::merge_attempts_exhausted());
            }
            MergeEntryAction::Proceed => {}
        }

        let github = self.github.as_ref().ok_or_else(SelfFixPolicy::github_not_configured)?;

        // Sensitivity is recomputed from the persisted flag. (The match above
        // already hard-refuses `sensitive_blocked`; this stays `false` here so
        // the gate's sensitive arm is driven by an explicit, independent value.)
        let sensitive = task.review_status.as_deref() == Some(SENSITIVE_BLOCKED);

        // Build the audit body (approver, task, merged head are filled in by
        //    the executor for the head; approver + task are known now).
        let audit_body = merge_audit_body(approver_id, task_id, pr_number);

        // d. Decide under the task-row lock the verdict writers hold (#1314). A
        //    rejection of the current report committed first is seen here and
        //    refuses the merge. Committing `approved` makes every later verdict
        //    write wait (it refuses while `approved`) until the merge outcome is
        //    known. Count this attempt BEFORE the executor runs so the counter
        //    is incremented even when the executor gate fails (e.g. checks_red,
        //    head_moved). The cap is checked in step c, so this always runs
        //    within the allowed budget. The transaction runs on the mutex's
        //    connection; the session advisory lock survives it.
        let mut tx = conn.begin().await?;
        let current = delivery_write::lock_task(&mut tx, scope, task_id).await?;
        if current.pr_head_sha != task.pr_head_sha {
            return Err(SelfFixPolicy::head_moved());
        }
        if let Some(source) = MaintenanceRequestRepository::find_by_task_on(&mut tx, scope, task_id).await? {
            let verdict = delivery::current_report_verdict_in_tx(&mut tx, scope, &current, &source).await?;
            ensure_merge_not_rejected(verdict.as_deref())?;
        }
        OrchestrationTaskRepository::bump_merge_attempts_in_tx(&mut tx, scope, task_id).await?;
        OrchestrationTaskRepository::set_review_status_on(&mut tx, scope, task_id, APPROVED).await?;
        tx.commit().await?;

        // Run the git-only guarded merge. NOTHING merges on a gate failure.
        let req = MergeRequest { pr_number, recorded_head_sha, sensitive };
        let outcome = match run_merge_executor(github.as_ref(), &req, &audit_body).await {
            Ok(outcome) => {
                // Record before the DB persist so the metric is always emitted
                // even if the subsequent write fails.
                let label = if outcome.already_merged { "already_merged" } else { "merged" };
                metrics::record_merge_outcome(label);
                outcome
            }
            Err(err) => {
                metrics::record_merge_failure(&err);
                // A definite refusal (our own gates, or the provider refusing
                // the merge call) is settled from a fresh pull-request read: a
                // provider refusal can race a merge, so `merged` wins. Any other
                // failure, or a failed re-read, stays `approved` until a new
                // approve call reconciles it.
                if SelfFixMergeMetricPolicy::is_definite_refusal(&err) {
                    match pull_request_state(github.as_ref(), pr_number).await {
                        Ok(state) => {
                            let status = SelfFixPolicy::review_status_after_refusal(state);
                            if let Err(restore) = OrchestrationTaskRepository::transition_review_status_on(
                                conn, scope, task_id, APPROVED, status,
                            )
                            .await
                            {
                                tracing::warn!(
                                    error = ?restore,
                                    task_id = %task_id,
                                    "self-fix: could not record the status after a refused merge; it stays approved"
                                );
                            } else if status == MERGED {
                                return Ok(already_merged());
                            }
                        }
                        Err(read) => tracing::warn!(
                            error = ?read,
                            task_id = %task_id,
                            "self-fix: pull request re-read after a refused merge failed; it stays approved"
                        ),
                    }
                }
                return Err(err);
            }
        };

        // Persist MERGED only after a confirmed merge (or idempotent success).
        OrchestrationTaskRepository::set_review_status_on(conn, scope, task_id, MERGED).await?;

        // Project the merge-executor result onto the domain-owned wire shape.
        Ok(SelfFixMergeResult {
            pr_number: outcome.pr_number,
            merged_head_sha: outcome.merged_head_sha,
            already_merged: outcome.already_merged,
        })
    }

    /// Read-side review snapshot for the in-platform review surface (milestone 8).
    ///
    /// Loads the task (tenant-scoped), requires it is a self-fix task, and
    /// assembles a [`SelfFixReview`] from its persisted PR columns plus a freshly
    /// read CI verdict. The CI read FAILS CLOSED: `checks_green` is `false`
    /// whenever GitHub is unconfigured, no head SHA is recorded, or the check
    /// fetch errors (logged, not propagated) — so a transient GitHub hiccup can
    /// never widen the Approve gate, only narrow it. The reviewer can re-fetch.
    pub(crate) async fn review_snapshot(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<SelfFixReview> {
        let task = self.tasks.find_by_id(scope, task_id).await?;
        if !task.self_fix {
            return Err(SelfFixPolicy::not_a_self_fix_task());
        }

        // Live CI verdict — only when GitHub is configured AND a head is recorded.
        // Any error degrades to not-green (fail closed) with a warning.
        let checks_green = match (self.github.as_ref(), task.pr_head_sha.as_deref()) {
            (Some(github), Some(head_sha)) => match github.all_checks_green(head_sha).await {
                Ok(green) => green,
                Err(err) => {
                    tracing::warn!(
                        error = ?err,
                        task_id = %task_id,
                        "self-fix review: CI check fetch failed; reporting not-green (fail closed)"
                    );
                    false
                }
            },
            _ => false,
        };

        // Display-only projection of the merge gate; approve re-checks it under the lock.
        let rejected = match self.maintenance_requests.find_by_task(scope, task_id).await? {
            Some(source) => {
                let mut tx = self.tasks.pool().begin().await?;
                let verdict = delivery::current_report_verdict_in_tx(&mut tx, scope, &task, &source).await?;
                tx.commit().await?;
                merge_blocked_by_verdict(verdict.as_deref())
            }
            None => false,
        };

        let mut review = SelfFixReview::from_columns(
            task_id,
            task.pr_number,
            task.pr_url,
            task.pr_head_sha,
            task.review_status,
            checks_green,
        );
        review.current_revision_rejected = rejected;
        Ok(review)
    }

    /// Map the task's agent to the server-visible host project directory under
    /// the managed workspace root. The agent's `workspace_id` is the execution
    /// boundary; its `cwd` selects the project subdir inside `/workspace`.
    async fn resolve_workspace_project_dir(
        &self,
        scope: &TenantScope,
        task: &agentforge_db::entities::OrchestrationTask,
    ) -> AppResult<PathBuf> {
        let agent_id: AgentId = task.assigned_agent_id.ok_or_else(SelfFixPolicy::workspace_unresolved)?;
        let agent = self.agents.find_by_id(scope, agent_id).await?;

        let mount_scope =
            WorkspaceMountScope { org_id: scope.org_id().as_uuid(), workspace_id: agent.workspace_id.as_uuid() };
        let paths = resolve_agent_workspace_paths(&self.workspace_root, mount_scope, agent.cwd.as_deref())
            .map_err(|_| SelfFixPolicy::workspace_unresolved())?;
        host_path_for_container_cwd(&paths.host_projects_root, &paths.container_cwd)
            .map_err(|_| SelfFixPolicy::workspace_unresolved())
    }
}

/// Build the draft-PR body. Includes the repo's required
/// `## Beginner UX / Operator Path` section with plain `- Field:` bullets whose
/// values are each ≥12 characters, matching the PR-body gate.
#[allow(dead_code)]
fn pr_body(title: &str, description: Option<&str>, task_id: Uuid) -> String {
    let summary = description.filter(|d| !d.trim().is_empty()).unwrap_or(title);
    format!(
        "## Summary\n\nAutomated self-fix change for task `{task_id}`.\n\n{summary}\n\n\
         This pull request was opened by the Wisdoverse self-fix loop. It is a DRAFT \
         pending review; an operator must review and merge it.\n\n\
         ## Beginner UX / Operator Path\n\n\
         - What changed: An automated agent rebuilt the requested fix into this branch.\n\
         - Where to look: Review the file diff in this pull request before approving anything.\n\
         - How to verify: Run the project's standard checks against this branch locally.\n\
         - If it looks wrong: Close this draft and re-run the task with clearer instructions.\n\
         - Who can merge: A maintainer with write access after the required checks pass.\n\
         - Next step after merge: Confirm the originating task moved to a completed state.\n"
    )
}

/// Build the PR audit comment the Merge Executor posts on a confirmed merge.
/// Names the approver, the originating task, and the PR; the executor appends
/// nothing secret. Kept attacker-independent (no tokens, no internal URLs).
#[allow(dead_code)]
fn merge_audit_body(approver_id: &str, task_id: Uuid, pr_number: i32) -> String {
    format!(
        "Wisdoverse self-fix: this pull request (#{pr_number}) was merged by the server-side \
         Merge Executor after operator `{approver_id}` approved self-fix task `{task_id}`. \
         The server independently re-verified that the change is non-sensitive, that all CI \
         checks were green on the merged head, and that the head had not moved (expected-head \
         merge). No safety check was bypassed."
    )
}
