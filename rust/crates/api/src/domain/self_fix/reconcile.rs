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

/// Live provider facts for the decision, read after the pre-checks pass.
pub(crate) struct ReconcileObservation<'a> {
    /// `open`, `closed` or `merged`.
    pub state: &'a str,
    /// The pull request still targets the repository and base branch the task was opened for.
    pub base_unchanged: bool,
    /// Head read after the file listing.
    pub head: &'a str,
    /// Head read before the file listing; it must equal `head`.
    pub head_before_listing: &'a str,
    pub listing: FileListing<'a>,
}

/// The provider's changed-file listing for the pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileListing<'a> {
    Complete(&'a [String]),
    /// Listed to the provider's cap (3000 files): may be truncated, so sensitive.
    Capped,
    /// The provider read failed: refuse, nothing written, safe to retry.
    Failed,
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
    /// Pre-checks passed; read the provider, then decide again.
    Observe,
    RefusePullRequestNotOpen,
    RefuseBaseChanged,
    RefuseHeadMismatch,
    /// The changed files could not be read: nothing written, safe to retry.
    RefuseListingUnavailable,
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
    if observed.state != "open" {
        return ReconcileAction::RefusePullRequestNotOpen;
    }
    if !observed.base_unchanged {
        return ReconcileAction::RefuseBaseChanged;
    }
    // The listing must describe the head the caller confirmed: unchanged
    // across the two reads and equal to the expected new head.
    if !observed.head.eq_ignore_ascii_case(&input.expected_new_head)
        || !observed.head_before_listing.eq_ignore_ascii_case(observed.head)
    {
        return ReconcileAction::RefuseHeadMismatch;
    }
    let sensitive = match observed.listing {
        FileListing::Complete(files) => SensitivePathPolicy::touches_sensitive(files),
        FileListing::Capped => true,
        FileListing::Failed => return ReconcileAction::RefuseListingUnavailable,
    };
    // An existing `sensitive_blocked` is never cleared, and needs no write.
    ReconcileAction::Adopt { sensitive, block: sensitive && review_status != Some(SENSITIVE_BLOCKED) }
}

/// The typed error for a refusing action. A non-refusing action seen where a
/// final decision is required means the row changed under the caller: stale.
pub(crate) fn reconcile_refusal(action: ReconcileAction) -> AppError {
    match action {
        ReconcileAction::RefuseMergeUnconfirmed => conflict(
            "errors.self_fix.reconcile_merge_unconfirmed",
            "A finish attempt for this change is in progress or not confirmed. Select Finish this fix to complete it, then use the new revision.",
        ),
        // The same 503-class response as the other failed provider reads.
        ReconcileAction::RefuseListingUnavailable => ErrorKind::Unavailable(
            "GitHub could not list the pull request's changed files. Nothing was changed; try again.".into(),
        )
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
            "The pull request moved to another revision. Refresh, check the new revision, then try again.",
        ),
        ReconcileAction::RefuseStale
        | ReconcileAction::NoOp
        | ReconcileAction::Observe
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
        let (clean, sensitive) = (FileListing::Complete(&clean), FileListing::Complete(&sensitive));
        // (state, base unchanged, head after listing, head before listing, listing)
        let obs = |state, base_unchanged, head, before, listing| (state, base_unchanged, head, before, listing);
        // (status, stored head, observation) -> action
        #[allow(clippy::type_complexity)]
        let table: Vec<(
            Option<&str>,
            Option<&str>,
            Option<(&str, bool, &str, &str, FileListing<'_>)>,
            ReconcileAction,
        )> = vec![
            // Status gates come first, whatever the heads.
            (Some("approved"), Some(OLD), None, RefuseMergeUnconfirmed),
            (Some("approved"), Some(NEW), None, RefuseMergeUnconfirmed),
            (Some("merged"), Some(OLD), None, RefuseMerged),
            (Some("merged"), Some(NEW), None, RefuseMerged),
            // Stored head equal to the new head: idempotent no-op, before the stale check.
            (Some("in_review"), Some(NEW), None, NoOp),
            (Some("sensitive_blocked"), Some(NEW), Some(obs("open", true, NEW, NEW, clean)), NoOp),
            // Stored head is neither: stale.
            (Some("in_review"), Some(OTHER), None, RefuseStale),
            (Some("in_review"), None, None, RefuseStale),
            (Some("changes_requested"), Some(OTHER), Some(obs("open", true, NEW, NEW, clean)), RefuseStale),
            // Pre-checks pass without an observation: read the provider.
            (Some("in_review"), Some(OLD), None, Observe),
            (Some("changes_requested"), Some(OLD), None, Observe),
            (Some("sensitive_blocked"), Some(OLD), None, Observe),
            (None, Some(OLD), None, Observe),
            // Live checks.
            (Some("in_review"), Some(OLD), Some(obs("closed", true, NEW, NEW, clean)), RefusePullRequestNotOpen),
            (Some("in_review"), Some(OLD), Some(obs("merged", true, NEW, NEW, clean)), RefusePullRequestNotOpen),
            (Some("in_review"), Some(OLD), Some(obs("open", false, NEW, NEW, clean)), RefuseBaseChanged),
            (Some("in_review"), Some(OLD), Some(obs("open", true, OTHER, OTHER, clean)), RefuseHeadMismatch),
            (Some("in_review"), Some(OLD), Some(obs("open", true, OLD, OLD, clean)), RefuseHeadMismatch),
            // The head moved between the two reads: the listing may describe another head.
            (Some("in_review"), Some(OLD), Some(obs("open", true, NEW, OTHER, clean)), RefuseHeadMismatch),
            // A failed listing is a refusal, never a block.
            (
                Some("in_review"),
                Some(OLD),
                Some(obs("open", true, NEW, NEW, FileListing::Failed)),
                RefuseListingUnavailable,
            ),
            (
                Some("sensitive_blocked"),
                Some(OLD),
                Some(obs("open", true, NEW, NEW, FileListing::Failed)),
                RefuseListingUnavailable,
            ),
            // Adoption and sensitivity.
            (
                Some("in_review"),
                Some(OLD),
                Some(obs("open", true, NEW, NEW, clean)),
                Adopt { sensitive: false, block: false },
            ),
            (
                Some("changes_requested"),
                Some(OLD),
                Some(obs("open", true, NEW, NEW, clean)),
                Adopt { sensitive: false, block: false },
            ),
            (
                Some("in_review"),
                Some(OLD),
                Some(obs("open", true, NEW, NEW, sensitive)),
                Adopt { sensitive: true, block: true },
            ),
            (
                Some("in_review"),
                Some(OLD),
                Some(obs("open", true, NEW, NEW, FileListing::Capped)),
                Adopt { sensitive: true, block: true },
            ),
            // `sensitive_blocked` is never cleared and needs no second write.
            (
                Some("sensitive_blocked"),
                Some(OLD),
                Some(obs("open", true, NEW, NEW, clean)),
                Adopt { sensitive: false, block: false },
            ),
            (
                Some("sensitive_blocked"),
                Some(OLD),
                Some(obs("open", true, NEW, NEW, sensitive)),
                Adopt { sensitive: true, block: false },
            ),
            (
                Some("sensitive_blocked"),
                Some(OLD),
                Some(obs("open", true, NEW, NEW, FileListing::Capped)),
                Adopt { sensitive: true, block: false },
            ),
        ];
        let input = input();
        for (status, stored, observed, expected) in table {
            let observation = observed.map(|(state, base_unchanged, head, head_before_listing, listing)| {
                ReconcileObservation { state, base_unchanged, head, head_before_listing, listing }
            });
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
        // A failed listing answers like the other failed provider reads (503).
        assert!(matches!(reconcile_refusal(RefuseListingUnavailable).kind, ErrorKind::Unavailable(_)));
    }
}
