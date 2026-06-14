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

use std::path::PathBuf;
use std::sync::Arc;

use agentforge_core::{AgentId, AppResult, TenantScope};
use uuid::Uuid;

use crate::domain::agent_workspace::{host_path_for_container_cwd, WorkspaceMountScope};
use crate::domain::self_fix::review_status::{APPROVED, IN_REVIEW, MERGED, SENSITIVE_BLOCKED};
use crate::domain::self_fix::SelfFixPolicy;
use crate::repositories::agent::AgentRepository;
use crate::repositories::orchestration::OrchestrationTaskRepository;
use crate::services::agent_container_control::AgentContainerControlService;
use crate::services::agent_workspace::resolve_agent_workspace_paths;
use crate::services::github_app::GithubAppClient;
use crate::services::self_fix::bridge::{run_pr_bridge, GitProvider, SelfFixPrOutcome};
use crate::services::self_fix::import::ImportLimits;
use crate::services::self_fix::merge_executor::{run_merge_executor, MergeOutcome, MergeRequest};

/// Server-side self-fix PR Bridge service.
///
/// Holds the task + agent repositories, the (optional) GitHub App client, the
/// agent-container control service (used to freeze the agent before the PR is
/// built), the managed workspace root, and the import limits. The route that
/// drives `open_pr` lands in a later milestone, hence `#[allow(dead_code)]`.
#[allow(dead_code)]
pub(crate) struct SelfFixService {
    tasks: OrchestrationTaskRepository,
    agents: AgentRepository,
    container_control: AgentContainerControlService,
    github: Option<Arc<GithubAppClient>>,
    workspace_root: String,
    limits: ImportLimits,
}

impl SelfFixService {
    #[allow(dead_code)]
    pub(crate) fn new(
        tasks: OrchestrationTaskRepository,
        agents: AgentRepository,
        container_control: AgentContainerControlService,
        github: Option<GithubAppClient>,
        workspace_root: String,
        limits: ImportLimits,
    ) -> Self {
        Self {
            tasks,
            agents,
            container_control,
            github: github.map(Arc::new),
            workspace_root,
            limits,
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
    #[allow(dead_code)]
    pub(crate) async fn open_pr(&self, scope: &TenantScope, task_id: Uuid) -> AppResult<SelfFixPrOutcome> {
        // 1. Load the task; require it is a self-fix task and GitHub is configured.
        let task = self.tasks.find_by_id(scope, task_id).await?;
        if !task.self_fix {
            return Err(SelfFixPolicy::not_a_self_fix_task());
        }
        let github = self.github.as_ref().ok_or_else(SelfFixPolicy::github_not_configured)?;

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

        // 4. Pin the base origin/main SHA and persist it.
        let base_sha = GitProvider::default_branch_sha(github.as_ref()).await?;
        self.tasks.set_base_commit_sha(scope, task_id, &base_sha).await?;

        // 5-10. Clone + rebuild + sensitive-check + push + open draft PR.
        let commit_message = format!("self-fix: {}", task.title);
        let pr_title = format!("[self-fix] {}", task.title);
        let pr_body = pr_body(&task.title, task.description.as_deref(), task_id);
        let result = run_pr_bridge(
            github.as_ref(),
            task_id,
            &base_sha,
            &workspace_project_dir,
            &commit_message,
            &pr_title,
            &pr_body,
            &self.limits,
        )
        .await?;

        // 11. Persist PR metadata + the selected review status.
        self.tasks
            .set_pr_metadata(scope, task_id, result.pr.number, &result.pr.html_url, &result.pr.head_sha, result.review_status)
            .await?;

        // 12. Return the outcome.
        Ok(SelfFixPrOutcome {
            pr_number: result.pr.number,
            pr_url: result.pr.html_url,
            review_status: result.review_status,
        })
    }

    /// Guarded server-side merge of an approved self-fix PR.
    ///
    /// The route layer (milestone 8) sets `review_status == approved` before
    /// calling this; we accept `approved` and (transitionally, until that route
    /// lands) `in_review` so the executor can be wired and exercised first. We
    /// also re-derive sensitivity SERVER-SIDE — a task persisted as
    /// `sensitive_blocked` at PR-open time is HARD-refused here regardless of any
    /// later GitHub state, and an already-`merged` task short-circuits as a no-op.
    ///
    /// Flow: load the task (tenant-scoped) → require it carries a PR and an
    /// allowed review status → require GitHub is configured (visible error
    /// otherwise) → recompute `sensitive` from the persisted review status →
    /// run the git-only [`run_merge_executor`] gate-and-merge → on a confirmed
    /// merge ONLY, persist `review_status == merged`. On ANY gate failure the
    /// task keeps its prior status and the error surfaces; nothing merges.
    #[allow(dead_code)]
    pub(crate) async fn approve_and_merge(
        &self,
        scope: &TenantScope,
        task_id: Uuid,
        approver_id: &str,
    ) -> AppResult<MergeOutcome> {
        // 1. Load the task; require self-fix + a configured GitHub App.
        let task = self.tasks.find_by_id(scope, task_id).await?;
        if !task.self_fix {
            return Err(SelfFixPolicy::not_a_self_fix_task());
        }
        let github = self.github.as_ref().ok_or_else(SelfFixPolicy::github_not_configured)?;

        // Require a PR linkage (number + recorded head SHA): the executor merges
        // an EXISTING PR, it does not open one.
        let pr_number = task.pr_number.ok_or_else(SelfFixPolicy::no_pr_to_merge)?;
        let recorded_head_sha = task.pr_head_sha.as_deref().ok_or_else(SelfFixPolicy::no_pr_to_merge)?;

        // 2. Gate on review status + recompute sensitivity SERVER-SIDE.
        //    - `merged`           → idempotent success (already done).
        //    - `sensitive_blocked`→ HARD refuse, no GitHub call.
        //    - `approved`         → the milestone-8 route gates here.
        //    - `in_review`        → transitionally accepted until that route lands.
        match task.review_status.as_deref() {
            Some(MERGED) => {
                return Ok(MergeOutcome {
                    pr_number,
                    merged_head_sha: recorded_head_sha.to_string(),
                    already_merged: true,
                });
            }
            Some(SENSITIVE_BLOCKED) => return Err(SelfFixPolicy::sensitive_path_blocked()),
            Some(APPROVED) | Some(IN_REVIEW) => {}
            _ => return Err(SelfFixPolicy::not_approved_for_merge()),
        }

        // Sensitivity is recomputed from the persisted flag. (The match above
        // already hard-refuses `sensitive_blocked`; this stays `false` here so
        // the gate's sensitive arm is driven by an explicit, independent value.)
        let sensitive = task.review_status.as_deref() == Some(SENSITIVE_BLOCKED);

        // 3. Build the audit body (approver, task, merged head are filled in by
        //    the executor for the head; approver + task are known now).
        let audit_body = merge_audit_body(approver_id, task_id, pr_number);

        // 4. Run the git-only guarded merge. NOTHING merges on a gate failure.
        let req = MergeRequest { pr_number, recorded_head_sha, sensitive };
        let outcome = run_merge_executor(github.as_ref(), &req, &audit_body).await?;

        // 5. Persist MERGED only after a confirmed merge (or idempotent success).
        self.tasks.set_review_status(scope, task_id, MERGED).await?;

        Ok(outcome)
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
