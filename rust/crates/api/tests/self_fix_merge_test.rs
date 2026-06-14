//! Integration tests for the self-fix guarded Merge Executor (`run_merge_executor`).
//!
//! These drive the gate-and-merge core against an IN-MEMORY fake `GitProvider`
//! (no real GitHub, no network, no database). They prove, end-to-end through the
//! same code path production uses:
//!   - Sensitive change → HARD refuse: no merge, no ready-flip, no comment.
//!   - Red CI → refuse: no merge.
//!   - Head moved between record and merge → refuse (head_moved): no merge.
//!   - Happy path → marks ready, merges with the EXPECTED head, comments.
//!   - Already-merged → idempotent success with no second merge.
//!
//! The fake records every mutating call so each assertion can prove that NOTHING
//! merged on a gate failure and that the merge used the re-verified fresh head.

use std::sync::Mutex;

use agentforge_api::testing::self_fix_merge::{run_merge_executor, GitProvider, MergeRequest, OpenedDraftPr};
use agentforge_core::{AppResult, ErrorKind};

/// In-memory GitHub stand-in. Models the minimum the Merge Executor reads/writes:
/// the PR head SHA, whether CI is green on that head, whether the PR is already
/// merged, and a log of every mutating call (ready-flip, merge, comment).
struct FakeGitProvider {
    /// Head SHA returned by `pr_head_sha`. A test can swap this between the
    /// first and second read to simulate a push during the ready transition.
    head: Mutex<String>,
    /// `all_checks_green` answer, keyed by head SHA. Missing key → false.
    green_for_head: Mutex<std::collections::HashMap<String, bool>>,
    /// `pr_is_merged` answer.
    already_merged: Mutex<bool>,
    /// Recorded mutating calls.
    ready_calls: Mutex<u32>,
    merge_calls: Mutex<Vec<(i32, String)>>, // (pr_number, expected_head)
    comment_calls: Mutex<Vec<String>>,
}

impl FakeGitProvider {
    fn new(head: &str) -> Self {
        Self {
            head: Mutex::new(head.to_string()),
            green_for_head: Mutex::new(std::collections::HashMap::new()),
            already_merged: Mutex::new(false),
            ready_calls: Mutex::new(0),
            merge_calls: Mutex::new(Vec::new()),
            comment_calls: Mutex::new(Vec::new()),
        }
    }
    fn set_green(&self, head: &str, green: bool) {
        self.green_for_head.lock().unwrap().insert(head.to_string(), green);
    }
}

#[async_trait::async_trait]
impl GitProvider for FakeGitProvider {
    // --- PR Bridge methods: unused by the Merge Executor; never called here. ---
    async fn default_branch_sha(&self) -> AppResult<String> {
        unreachable!("merge executor never calls default_branch_sha")
    }
    async fn authed_remote_url(&self) -> AppResult<String> {
        unreachable!("merge executor never calls authed_remote_url")
    }
    async fn create_draft_pr(&self, _h: &str, _b: &str, _t: &str, _body: &str) -> AppResult<OpenedDraftPr> {
        unreachable!("merge executor never calls create_draft_pr")
    }

    // --- Merge Executor methods. ---
    async fn all_checks_green(&self, head_sha: &str) -> AppResult<bool> {
        Ok(self.green_for_head.lock().unwrap().get(head_sha).copied().unwrap_or(false))
    }
    async fn pr_head_sha(&self, _pr_number: i32) -> AppResult<String> {
        Ok(self.head.lock().unwrap().clone())
    }
    async fn pr_is_merged(&self, _pr_number: i32) -> AppResult<bool> {
        Ok(*self.already_merged.lock().unwrap())
    }
    async fn mark_ready_for_review(&self, _pr_number: i32) -> AppResult<()> {
        *self.ready_calls.lock().unwrap() += 1;
        Ok(())
    }
    async fn merge_with_expected_head(&self, pr_number: i32, expected_head: &str) -> AppResult<()> {
        self.merge_calls.lock().unwrap().push((pr_number, expected_head.to_string()));
        *self.already_merged.lock().unwrap() = true;
        Ok(())
    }
    async fn comment(&self, _pr_number: i32, body: &str) -> AppResult<()> {
        self.comment_calls.lock().unwrap().push(body.to_string());
        Ok(())
    }
}

const PR: i32 = 4242;

#[tokio::test]
async fn sensitive_change_is_hard_refused_and_nothing_merges() {
    let fake = FakeGitProvider::new("headsha");
    fake.set_green("headsha", true); // even with green CI + unchanged head...
    let req = MergeRequest { pr_number: PR, recorded_head_sha: "headsha", sensitive: true };

    let err = run_merge_executor(&fake, &req, "audit").await.expect_err("sensitive must refuse");
    assert!(
        matches!(err.kind, ErrorKind::ForbiddenWithCode { code, .. } if code == "errors.self_fix.sensitive_path_blocked"),
        "must refuse with the sensitive_path_blocked code"
    );
    assert_eq!(*fake.ready_calls.lock().unwrap(), 0, "must NOT flip to ready");
    assert!(fake.merge_calls.lock().unwrap().is_empty(), "must NOT merge");
    assert!(fake.comment_calls.lock().unwrap().is_empty(), "must NOT comment");
}

#[tokio::test]
async fn red_ci_refuses_and_nothing_merges() {
    let fake = FakeGitProvider::new("headsha");
    fake.set_green("headsha", false); // CI red
    let req = MergeRequest { pr_number: PR, recorded_head_sha: "headsha", sensitive: false };

    let err = run_merge_executor(&fake, &req, "audit").await.expect_err("red CI must refuse");
    assert!(
        matches!(err.kind, ErrorKind::ValidationWithCode { code, .. } if code == "errors.self_fix.checks_not_green"),
        "must refuse with the checks_not_green code"
    );
    assert_eq!(*fake.ready_calls.lock().unwrap(), 0, "must NOT flip to ready");
    assert!(fake.merge_calls.lock().unwrap().is_empty(), "must NOT merge");
}

#[tokio::test]
async fn head_moved_since_record_refuses_before_ready() {
    // Live head differs from the recorded (reviewed) head: the FIRST gate
    // already refuses with head_moved, before any ready-flip.
    let fake = FakeGitProvider::new("newhead");
    fake.set_green("newhead", true);
    let req = MergeRequest { pr_number: PR, recorded_head_sha: "oldhead", sensitive: false };

    let err = run_merge_executor(&fake, &req, "audit").await.expect_err("moved head must refuse");
    assert!(matches!(err.kind, ErrorKind::Conflict(_)), "must refuse with the head-moved conflict");
    assert_eq!(*fake.ready_calls.lock().unwrap(), 0, "must NOT flip to ready");
    assert!(fake.merge_calls.lock().unwrap().is_empty(), "must NOT merge");
}

#[tokio::test]
async fn head_moved_during_ready_transition_refuses_after_reverify() {
    // The recorded head matches the live head at the FIRST gate (so the run
    // proceeds past it and flips to ready), but the ready transition triggers a
    // push that advances the head. The post-ready re-verification against the
    // FRESH head must refuse (fresh "pushedhead" != recorded "headsha"), and
    // NOTHING merges even though CI is green on the new head.
    let fake = HeadAdvancingFake::new("headsha", "pushedhead");
    let req = MergeRequest { pr_number: PR, recorded_head_sha: "headsha", sensitive: false };

    let err = run_merge_executor(&fake, &req, "audit").await.expect_err("post-ready head move must refuse");
    assert!(matches!(err.kind, ErrorKind::Conflict(_)), "must refuse with head-moved after re-verify");
    assert_eq!(fake.ready_calls(), 1, "ready WAS flipped (the move happened during the transition)");
    assert!(fake.merged_with().is_none(), "must NOT merge after a post-ready head move");
}

#[tokio::test]
async fn happy_path_marks_ready_merges_expected_head_and_comments() {
    let fake = FakeGitProvider::new("headsha");
    fake.set_green("headsha", true);
    let req = MergeRequest { pr_number: PR, recorded_head_sha: "headsha", sensitive: false };

    let outcome = run_merge_executor(&fake, &req, "audit-body-text").await.expect("happy path must merge");

    assert!(!outcome.already_merged, "this is a fresh merge");
    assert_eq!(outcome.pr_number, PR);
    assert_eq!(outcome.merged_head_sha, "headsha", "merged the re-verified fresh head");
    assert_eq!(*fake.ready_calls.lock().unwrap(), 1, "flipped to ready exactly once");
    let merges = fake.merge_calls.lock().unwrap();
    assert_eq!(merges.len(), 1, "merged exactly once");
    assert_eq!(merges[0], (PR, "headsha".to_string()), "merged with the EXPECTED (fresh) head");
    let comments = fake.comment_calls.lock().unwrap();
    assert_eq!(comments.len(), 1, "posted exactly one audit comment");
    assert_eq!(comments[0], "audit-body-text");
}

#[tokio::test]
async fn already_merged_is_idempotent_success_without_a_second_merge() {
    let fake = FakeGitProvider::new("headsha");
    *fake.already_merged.lock().unwrap() = true; // GitHub says it's already merged
    let req = MergeRequest { pr_number: PR, recorded_head_sha: "headsha", sensitive: false };

    let outcome = run_merge_executor(&fake, &req, "audit").await.expect("already-merged must succeed");

    assert!(outcome.already_merged, "must report the idempotent path");
    assert_eq!(outcome.pr_number, PR);
    assert!(fake.merge_calls.lock().unwrap().is_empty(), "must NOT merge a second time");
    assert_eq!(*fake.ready_calls.lock().unwrap(), 0, "must NOT flip to ready");
    assert!(fake.comment_calls.lock().unwrap().is_empty(), "must NOT comment on the idempotent path");
}

/// A fake whose head ADVANCES the moment `mark_ready_for_review` is called,
/// modelling a ready-for-review automation that pushes a new commit. Used to
/// prove the post-ready re-verification catches the move.
struct HeadAdvancingFake {
    first_head: String,
    advanced_head: String,
    ready: Mutex<u32>,
    merged_with: Mutex<Option<String>>,
}

impl HeadAdvancingFake {
    fn new(first_head: &str, advanced_head: &str) -> Self {
        Self {
            first_head: first_head.to_string(),
            advanced_head: advanced_head.to_string(),
            ready: Mutex::new(0),
            merged_with: Mutex::new(None),
        }
    }
    fn ready_calls(&self) -> u32 {
        *self.ready.lock().unwrap()
    }
    fn merged_with(&self) -> Option<String> {
        self.merged_with.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl GitProvider for HeadAdvancingFake {
    async fn default_branch_sha(&self) -> AppResult<String> {
        unreachable!()
    }
    async fn authed_remote_url(&self) -> AppResult<String> {
        unreachable!()
    }
    async fn create_draft_pr(&self, _h: &str, _b: &str, _t: &str, _body: &str) -> AppResult<OpenedDraftPr> {
        unreachable!()
    }
    async fn all_checks_green(&self, _head_sha: &str) -> AppResult<bool> {
        Ok(true) // green on every head; the freshness check is what refuses
    }
    async fn pr_head_sha(&self, _pr_number: i32) -> AppResult<String> {
        // Before the ready-flip: the original (reviewed) head. After: advanced.
        if *self.ready.lock().unwrap() == 0 {
            Ok(self.first_head.clone())
        } else {
            Ok(self.advanced_head.clone())
        }
    }
    async fn pr_is_merged(&self, _pr_number: i32) -> AppResult<bool> {
        Ok(false)
    }
    async fn mark_ready_for_review(&self, _pr_number: i32) -> AppResult<()> {
        *self.ready.lock().unwrap() += 1;
        Ok(())
    }
    async fn merge_with_expected_head(&self, _pr_number: i32, expected_head: &str) -> AppResult<()> {
        *self.merged_with.lock().unwrap() = Some(expected_head.to_string());
        Ok(())
    }
    async fn comment(&self, _pr_number: i32, _body: &str) -> AppResult<()> {
        Ok(())
    }
}
