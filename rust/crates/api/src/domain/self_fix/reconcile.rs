//! Administrator reconciliation of a changed pull-request head (#1315). No I/O.

use agentforge_core::{AppError, AppResult, ErrorKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::SensitivePathPolicy;
use super::review_status::{APPROVED, MERGED, SENSITIVE_BLOCKED};

pub(crate) const RECONCILE_HEAD_AUDIT_ACTION: &str = "self_fix.head_reconciled";
pub(crate) const RECONCILE_HEAD_AUDIT_RESOURCE: &str = "orchestration_task";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReconcileHeadInput {
    pub expected_stored_head: String,
    pub expected_new_head: String,
}

impl ReconcileHeadInput {
    /// Both revisions must be full 40-character hexadecimal SHAs.
    pub(crate) fn prepare(self) -> AppResult<Self> {
        let full = |sha: &str| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit());
        if !full(&self.expected_stored_head) || !full(&self.expected_new_head) {
            return Err(reconcile_invalid());
        }
        Ok(Self {
            expected_stored_head: self.expected_stored_head.to_ascii_lowercase(),
            expected_new_head: self.expected_new_head.to_ascii_lowercase(),
        })
    }
}

/// One validated read of the pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PullRequestRead<'a> {
    /// `open`, `closed` or `merged`.
    pub state: &'a str,
    /// It still targets the repository and base branch the task was opened for.
    pub base_unchanged: bool,
    pub head: &'a str,
    /// The base commit; required, and identical in the first and last read.
    pub base_sha: Option<&'a str>,
}

/// Live provider facts for the decision, gathered after the pre-checks pass.
pub(crate) struct ReconcileObservation<'a> {
    /// The read whose base commit the comparison uses.
    pub first: PullRequestRead<'a>,
    /// Changed files between that base commit and the expected new head.
    pub comparison: Comparison<'a>,
    /// The re-read after the comparison; `None` until it was made.
    pub last: Option<PullRequestRead<'a>>,
}

/// The provider's comparison of two immutable commits (base commit ... new head).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Comparison<'a> {
    /// Not requested yet: the first read must pass before it is.
    Pending,
    Complete(&'a [String]),
    /// The listing may be incomplete (at the provider's file cap): sensitive.
    Capped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReconcileAction {
    /// A finish attempt is in progress or unconfirmed (`approved`).
    RefuseMergeUnconfirmed,
    RefuseMerged,
    /// The stored head is not the one the caller saw.
    RefuseStale,
    /// The new head is already stored: idempotent success, nothing written.
    NoOp,
    /// Pre-checks passed; read the pull request, then decide again.
    Observe,
    /// The first read passed; compare its base commit with the new head and
    /// read the pull request again, then decide again.
    Compare,
    RefusePullRequestNotOpen,
    RefuseBaseChanged,
    RefuseHeadMismatch,
    /// A provider read failed or returned unusable data: nothing written, safe to retry.
    RefuseProviderUnavailable,
    /// Store the new head. `block` additionally sets `sensitive_blocked`.
    Adopt {
        sensitive: bool,
        block: bool,
    },
}

/// The single reconcile decision: stored status x stored head x request x
/// (optional) live observation and sensitivity.
pub(crate) fn reconcile_head_action(
    review_status: Option<&str>,
    stored_head: Option<&str>,
    input: &ReconcileHeadInput,
    observation: Option<&ReconcileObservation<'_>>,
) -> ReconcileAction {
    match review_status {
        Some(APPROVED) => return ReconcileAction::RefuseMergeUnconfirmed,
        Some(MERGED) => return ReconcileAction::RefuseMerged,
        _ => {}
    }
    let stored = stored_head.map(str::to_ascii_lowercase);
    if stored.as_deref() == Some(input.expected_new_head.as_str()) {
        return ReconcileAction::NoOp;
    }
    if stored.as_deref() != Some(input.expected_stored_head.as_str()) {
        return ReconcileAction::RefuseStale;
    }
    let Some(observed) = observation else {
        return ReconcileAction::Observe;
    };
    // The base is checked before any comparison is requested.
    if let Some(refusal) = refuse_read(&observed.first, input) {
        return refusal;
    }
    let Some(base_sha) = observed.first.base_sha else {
        return ReconcileAction::RefuseProviderUnavailable;
    };
    let sensitive = match observed.comparison {
        Comparison::Pending => return ReconcileAction::Compare,
        Comparison::Complete(files) => SensitivePathPolicy::touches_sensitive(files),
        Comparison::Capped => true,
    };
    // The re-read after the comparison must still show the confirmed state.
    let Some(last) = &observed.last else {
        return ReconcileAction::Compare;
    };
    if let Some(refusal) = refuse_read(last, input) {
        return refusal;
    }
    // The compared base commit must still be the pull request's base: a
    // rewritten base branch changes the merge base and so the checked diff.
    match last.base_sha {
        None => return ReconcileAction::RefuseProviderUnavailable,
        Some(last_base) if !last_base.eq_ignore_ascii_case(base_sha) => return ReconcileAction::RefuseHeadMismatch,
        Some(_) => {}
    }
    // An existing `sensitive_blocked` is never cleared, and needs no write.
    ReconcileAction::Adopt { sensitive, block: sensitive && review_status != Some(SENSITIVE_BLOCKED) }
}

fn refuse_read(read: &PullRequestRead<'_>, input: &ReconcileHeadInput) -> Option<ReconcileAction> {
    if read.state != "open" {
        return Some(ReconcileAction::RefusePullRequestNotOpen);
    }
    if !read.base_unchanged {
        return Some(ReconcileAction::RefuseBaseChanged);
    }
    if !read.head.eq_ignore_ascii_case(&input.expected_new_head) {
        return Some(ReconcileAction::RefuseHeadMismatch);
    }
    None
}

/// The typed error for a refusing action. A non-refusing action seen where a
/// final decision is required means the row changed under the caller: stale.
pub(crate) fn reconcile_refusal(action: ReconcileAction) -> AppError {
    match action {
        ReconcileAction::RefuseMergeUnconfirmed => conflict(
            "errors.self_fix.reconcile_merge_unconfirmed",
            "A finish attempt for this change is in progress or not confirmed. Select Finish this fix to complete it, then use the new revision.",
        ),
        // One coded 503 for every failed provider read during a reconcile.
        ReconcileAction::RefuseProviderUnavailable => ErrorKind::UnavailableWithCode {
            code: "errors.self_fix.reconcile_provider_unavailable",
            message: "GitHub could not be read, so nothing was changed. Try again. If it keeps failing, check that the pull request still exists and that the GitHub App can access the repository.".into(),
        }
        .into(),
        ReconcileAction::RefuseMerged => conflict(
            "errors.self_fix.reconcile_merged",
            "This change is already merged. There is no newer revision to adopt.",
        ),
        ReconcileAction::RefusePullRequestNotOpen => conflict(
            "errors.self_fix.reconcile_pull_request_not_open",
            "The pull request is not open on GitHub. Reopen it there, or finish the change on GitHub.",
        ),
        ReconcileAction::RefuseBaseChanged => conflict(
            "errors.self_fix.reconcile_base_changed",
            "The pull request no longer targets the repository and branch this task was opened for. Finish the change on GitHub.",
        ),
        ReconcileAction::RefuseHeadMismatch => conflict(
            "errors.self_fix.reconcile_head_mismatch",
            "The pull request changed while Forge checked it (its revision or its base moved). Refresh, check the new revision, then try again.",
        ),
        ReconcileAction::RefuseStale
        | ReconcileAction::NoOp
        | ReconcileAction::Observe
        | ReconcileAction::Compare
        | ReconcileAction::Adopt { .. } => conflict(
            "errors.self_fix.reconcile_stale",
            "The recorded revision changed since this page loaded. Refresh, check both revisions, then try again.",
        ),
    }
}

pub(crate) fn reconcile_invalid() -> AppError {
    ErrorKind::ValidationWithCode {
        code: "errors.self_fix.reconcile_invalid",
        message: "Send the full 40-character recorded revision and the new revision shown on the page.".into(),
    }
    .into()
}

fn conflict(code: &'static str, message: &str) -> AppError {
    ErrorKind::ConflictWithCode { code, message: message.into() }.into()
}

/// Audit details: the actor and task are the audit row's user and resource.
pub(crate) fn reconcile_head_audit_details(
    previous_head: &str,
    new_head: &str,
    pr_number: i32,
    sensitive: bool,
) -> Value {
    json!({ "previousHead": previous_head, "newHead": new_head, "pullRequest": pr_number, "sensitive": sensitive })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReconcileHeadResult {
    pub task_id: Uuid,
    pub previous_head: String,
    pub current_head: String,
    /// `false` for an idempotent repeat: the new head was already stored.
    pub changed: bool,
    pub sensitive: bool,
    pub review_status: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ReconcileAction::*;

    const OLD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const NEW: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const OTHER: &str = "cccccccccccccccccccccccccccccccccccccccc";

    fn input() -> ReconcileHeadInput {
        ReconcileHeadInput { expected_stored_head: OLD.into(), expected_new_head: NEW.into() }
    }

    #[test]
    fn input_requires_full_shas_and_normalizes_case() {
        assert!(input().prepare().is_ok());
        let upper = ReconcileHeadInput { expected_stored_head: OLD.to_uppercase(), expected_new_head: NEW.into() };
        assert_eq!(upper.prepare().unwrap().expected_stored_head, OLD);
        for (stored, new) in [(&OLD[..7], NEW), (OLD, "z".repeat(40).as_str()), (OLD, "")] {
            let bad = ReconcileHeadInput { expected_stored_head: stored.into(), expected_new_head: new.into() };
            assert!(bad.prepare().is_err(), "{stored} {new}");
        }
    }

    #[test]
    fn reconcile_head_action_covers_every_combination() {
        let clean = vec!["src/lib.rs".to_string()];
        let sensitive = vec!["rust/crates/db/migrations/999_x.sql".to_string()];
        let (clean, sensitive) = (Comparison::Complete(&clean), Comparison::Complete(&sensitive));
        const BASE: &str = "dddddddddddddddddddddddddddddddddddddddd";
        const REWRITTEN: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
        let read = |state, base_unchanged, head| PullRequestRead { state, base_unchanged, head, base_sha: Some(BASE) };
        let ok = read("open", true, NEW);
        let no_base = PullRequestRead { base_sha: None, ..ok };
        let rewritten = PullRequestRead { base_sha: Some(REWRITTEN), ..ok };
        // (status, stored head, (first read, comparison, last read)) -> action
        #[allow(clippy::type_complexity)]
        let table: Vec<(
            Option<&str>,
            Option<&str>,
            Option<(PullRequestRead<'_>, Comparison<'_>, Option<PullRequestRead<'_>>)>,
            ReconcileAction,
        )> = vec![
            // Status gates come first, whatever the heads.
            (Some("approved"), Some(OLD), None, RefuseMergeUnconfirmed),
            (Some("approved"), Some(NEW), None, RefuseMergeUnconfirmed),
            (Some("merged"), Some(OLD), None, RefuseMerged),
            (Some("merged"), Some(NEW), None, RefuseMerged),
            // Stored head equal to the new head: idempotent no-op, before the stale check.
            (Some("in_review"), Some(NEW), None, NoOp),
            (Some("sensitive_blocked"), Some(NEW), Some((ok, clean, Some(ok))), NoOp),
            // Stored head is neither: stale.
            (Some("in_review"), Some(OTHER), None, RefuseStale),
            (Some("in_review"), None, None, RefuseStale),
            (Some("changes_requested"), Some(OTHER), Some((ok, clean, Some(ok))), RefuseStale),
            // Pre-checks pass without an observation: read the pull request.
            (Some("in_review"), Some(OLD), None, Observe),
            (Some("changes_requested"), Some(OLD), None, Observe),
            (Some("sensitive_blocked"), Some(OLD), None, Observe),
            (None, Some(OLD), None, Observe),
            // First read: refused before any comparison.
            (
                Some("in_review"),
                Some(OLD),
                Some((read("closed", true, NEW), Comparison::Pending, None)),
                RefusePullRequestNotOpen,
            ),
            (
                Some("in_review"),
                Some(OLD),
                Some((read("merged", true, NEW), Comparison::Pending, None)),
                RefusePullRequestNotOpen,
            ),
            (
                Some("in_review"),
                Some(OLD),
                Some((read("open", false, NEW), Comparison::Pending, None)),
                RefuseBaseChanged,
            ),
            (
                Some("in_review"),
                Some(OLD),
                Some((read("open", true, OTHER), Comparison::Pending, None)),
                RefuseHeadMismatch,
            ),
            (
                Some("in_review"),
                Some(OLD),
                Some((read("open", true, OLD), Comparison::Pending, None)),
                RefuseHeadMismatch,
            ),
            // First read passed: compare next.
            (Some("in_review"), Some(OLD), Some((ok, Comparison::Pending, None)), Compare),
            (Some("in_review"), Some(OLD), Some((ok, clean, None)), Compare),
            // The first read must carry the base commit used for the comparison.
            (Some("in_review"), Some(OLD), Some((no_base, Comparison::Pending, None)), RefuseProviderUnavailable),
            // Last read after the comparison: the confirmed state must still hold.
            (
                Some("in_review"),
                Some(OLD),
                Some((ok, clean, Some(read("closed", true, NEW)))),
                RefusePullRequestNotOpen,
            ),
            (Some("in_review"), Some(OLD), Some((ok, clean, Some(read("open", false, NEW)))), RefuseBaseChanged),
            (Some("in_review"), Some(OLD), Some((ok, clean, Some(read("open", true, OTHER)))), RefuseHeadMismatch),
            // The base commit must be present and unchanged in the last read.
            (Some("in_review"), Some(OLD), Some((ok, clean, Some(no_base))), RefuseProviderUnavailable),
            (Some("in_review"), Some(OLD), Some((ok, clean, Some(rewritten))), RefuseHeadMismatch),
            (Some("sensitive_blocked"), Some(OLD), Some((ok, sensitive, Some(rewritten))), RefuseHeadMismatch),
            // Adoption and sensitivity.
            (Some("in_review"), Some(OLD), Some((ok, clean, Some(ok))), Adopt { sensitive: false, block: false }),
            (
                Some("changes_requested"),
                Some(OLD),
                Some((ok, clean, Some(ok))),
                Adopt { sensitive: false, block: false },
            ),
            (Some("in_review"), Some(OLD), Some((ok, sensitive, Some(ok))), Adopt { sensitive: true, block: true }),
            (
                Some("in_review"),
                Some(OLD),
                Some((ok, Comparison::Capped, Some(ok))),
                Adopt { sensitive: true, block: true },
            ),
            // `sensitive_blocked` is never cleared and needs no second write.
            (
                Some("sensitive_blocked"),
                Some(OLD),
                Some((ok, clean, Some(ok))),
                Adopt { sensitive: false, block: false },
            ),
            (
                Some("sensitive_blocked"),
                Some(OLD),
                Some((ok, sensitive, Some(ok))),
                Adopt { sensitive: true, block: false },
            ),
            (
                Some("sensitive_blocked"),
                Some(OLD),
                Some((ok, Comparison::Capped, Some(ok))),
                Adopt { sensitive: true, block: false },
            ),
        ];
        let input = input();
        for (status, stored, observed, expected) in table {
            let observation =
                observed.map(|(first, comparison, last)| ReconcileObservation { first, comparison, last });
            assert_eq!(
                reconcile_head_action(status, stored, &input, observation.as_ref()),
                expected,
                "{status:?} {stored:?} {observed:?}"
            );
        }
    }

    #[test]
    fn every_refusal_is_a_typed_conflict_with_guidance() {
        for action in [
            RefuseMerged,
            RefuseStale,
            RefusePullRequestNotOpen,
            RefuseBaseChanged,
            RefuseHeadMismatch,
            RefuseMergeUnconfirmed,
        ] {
            let err = reconcile_refusal(action);
            assert!(matches!(err.kind, ErrorKind::ConflictWithCode { .. }), "{action:?}");
        }
        assert!(matches!(
            reconcile_refusal(RefuseMergeUnconfirmed).kind,
            ErrorKind::ConflictWithCode { code, ref message }
                if code == "errors.self_fix.reconcile_merge_unconfirmed" && message.ends_with("then use the new revision.")
        ));
        // Every failed provider read answers with one coded 503.
        assert!(matches!(
            reconcile_refusal(RefuseProviderUnavailable).kind,
            ErrorKind::UnavailableWithCode { code, .. } if code == "errors.self_fix.reconcile_provider_unavailable"
        ));
    }
}
