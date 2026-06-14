//! Guarded Merge Executor for the self-fix loop (milestone 7).
//!
//! After a human approves a self-fix PR (review_status -> approved), the server
//! independently RE-VERIFIES safety before merging:
//!
//! 1. Sensitivity is recomputed server-side and HARD-refused — no GitHub state
//!    (label, approval, branch protection) can override it.
//! 2. CI checks are re-read against the PR's current head.
//! 3. The recorded head is compared against the live head.
//!
//! All three must hold; the final merge is gated on the *expected head* so a
//! concurrent push can never sneak unreviewed code into the merge.
//!
//! Trust posture: this is the server's own gate. The fact that GitHub would also
//! enforce branch protection is defence-in-depth, not the primary control. The
//! sensitive-path refusal and the expected-head merge are the server-owned
//! guarantees that nothing unsafe merges.

use agentforge_core::AppResult;

use crate::domain::self_fix::SelfFixPolicy;
use crate::services::self_fix::bridge::GitProvider;

/// Pure merge gate. All three conditions must hold to merge. The evaluation
/// order is deliberate: sensitivity is a HARD refuse and is checked first, then
/// CI, then head-freshness. The returned error is the specific policy error for
/// the first failing condition so the caller can surface a precise reason.
pub(crate) struct MergeGate;

impl MergeGate {
    /// `Ok(())` only when the change is non-sensitive, CI is fully green, and the
    /// head has not moved since it was recorded. Otherwise the first failing
    /// condition's policy error (sensitive > checks > head).
    pub(crate) fn evaluate(sensitive: bool, checks_green: bool, head_unchanged: bool) -> AppResult<()> {
        if sensitive {
            return Err(SelfFixPolicy::sensitive_path_blocked());
        }
        if !checks_green {
            return Err(SelfFixPolicy::checks_not_green());
        }
        if !head_unchanged {
            return Err(SelfFixPolicy::head_moved());
        }
        Ok(())
    }
}

/// What a successful merge reports back to the caller / route.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct MergeOutcome {
    /// The PR that was merged.
    pub pr_number: i32,
    /// The head SHA that was actually merged (the fresh, re-verified head).
    pub merged_head_sha: String,
    /// `true` when the PR was already merged before this call (idempotent path):
    /// nothing was re-merged, the task is simply reconciled to `merged`.
    pub already_merged: bool,
}

/// The merge-time inputs the executor needs, pre-loaded from the task by the DB
/// wrapper so this git-only core takes no repository.
#[allow(dead_code)]
pub struct MergeRequest<'a> {
    pub pr_number: i32,
    /// The head SHA recorded when the PR was opened (the reviewed head).
    pub recorded_head_sha: &'a str,
    /// Server-recomputed sensitivity. `true` HARD-refuses the merge regardless
    /// of any GitHub state. The DB wrapper derives this from the persisted
    /// `review_status == sensitive_blocked` flag set at PR-open time.
    pub sensitive: bool,
}

/// Run the guarded merge against a [`GitProvider`] with NO database access. The
/// caller (`SelfFixService::approve_and_merge`) loads the task, derives the
/// [`MergeRequest`], and persists the resulting status; this core owns ONLY the
/// safety re-verification and the atomic merge.
///
/// Flow:
/// 0. Idempotency: if the PR is already merged, report success without merging.
/// 1. Read the live head + CI state; gate (sensitive > checks > head). On any
///    failure NOTHING merges and the error propagates.
/// 2. `mark_ready_for_review` (draft -> ready).
/// 3. RE-READ the head and RE-CHECK CI (the ready transition could trigger a
///    push); re-gate against the FRESH head.
/// 4. `merge_with_expected_head(fresh_head)` — GitHub's 409 is the atomic guard
///    against a head that moved between re-check and merge.
/// 5. Post the audit comment.
#[allow(dead_code)]
pub async fn run_merge_executor<G: GitProvider + ?Sized>(
    provider: &G,
    req: &MergeRequest<'_>,
    audit_body: &str,
) -> AppResult<MergeOutcome> {
    // 0. Idempotency: a retry after a successful merge must succeed, not error
    //    on a no-longer-mergeable PR.
    if provider.pr_is_merged(req.pr_number).await? {
        return Ok(MergeOutcome {
            pr_number: req.pr_number,
            merged_head_sha: req.recorded_head_sha.to_string(),
            already_merged: true,
        });
    }

    // 1. First gate, against the live head as it stands BEFORE we touch the PR.
    //    Sensitivity is recomputed server-side and refused first; nothing about
    //    GitHub's state can override it.
    let live_head = provider.pr_head_sha(req.pr_number).await?;
    let checks_green = provider.all_checks_green(&live_head).await?;
    let head_unchanged = live_head == req.recorded_head_sha;
    MergeGate::evaluate(req.sensitive, checks_green, head_unchanged)?;

    // 2. Draft -> ready. (A draft PR is not mergeable.)
    provider.mark_ready_for_review(req.pr_number).await?;

    // 3. The ready transition can trigger automation that pushes a new commit.
    //    RE-READ the head and RE-CHECK CI, then re-gate against the FRESH head.
    let fresh_head = provider.pr_head_sha(req.pr_number).await?;
    let fresh_checks_green = provider.all_checks_green(&fresh_head).await?;
    let fresh_head_unchanged = fresh_head == req.recorded_head_sha;
    MergeGate::evaluate(req.sensitive, fresh_checks_green, fresh_head_unchanged)?;

    // 4. Atomic merge gated on the fresh head. If anything pushed between the
    //    re-check and here, GitHub returns 409 -> head_moved and we DO NOT merge.
    provider.merge_with_expected_head(req.pr_number, &fresh_head).await?;

    // 5. Audit trail. A comment failure must not un-merge the PR, but it is part
    //    of the contract, so surface it; the merge already happened atomically.
    provider.comment(req.pr_number, audit_body).await?;

    Ok(MergeOutcome { pr_number: req.pr_number, merged_head_sha: fresh_head, already_merged: false })
}

#[cfg(test)]
mod gate_tests {
    use super::*;
    use agentforge_core::ErrorKind;

    #[test]
    fn all_clear_passes() {
        assert!(MergeGate::evaluate(false, true, true).is_ok());
    }

    #[test]
    fn sensitive_is_hard_refused_first() {
        let err = MergeGate::evaluate(true, true, true).expect_err("sensitive must refuse");
        assert!(
            matches!(err.kind, ErrorKind::ForbiddenWithCode { code, .. } if code == "errors.self_fix.sensitive_path_blocked"),
            "sensitive must map to the sensitive_path_blocked forbidden code"
        );
    }

    #[test]
    fn sensitive_wins_even_when_checks_red_and_head_moved() {
        // Order guarantee: sensitive is evaluated before checks/head, so a
        // sensitive change with red CI and a moved head still reports SENSITIVE.
        let err = MergeGate::evaluate(true, false, false).expect_err("sensitive must refuse first");
        assert!(matches!(err.kind, ErrorKind::ForbiddenWithCode { code, .. } if code == "errors.self_fix.sensitive_path_blocked"));
    }

    #[test]
    fn red_ci_refuses() {
        let err = MergeGate::evaluate(false, false, true).expect_err("red CI must refuse");
        assert!(
            matches!(err.kind, ErrorKind::ValidationWithCode { code, .. } if code == "errors.self_fix.checks_not_green"),
            "red CI must map to checks_not_green"
        );
    }

    #[test]
    fn moved_head_refuses() {
        let err = MergeGate::evaluate(false, true, false).expect_err("moved head must refuse");
        assert!(
            matches!(err.kind, ErrorKind::Conflict(_)),
            "moved head must map to the head-moved conflict"
        );
    }
}
