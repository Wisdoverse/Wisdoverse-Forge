use crate::domain::orchestration::{
    BlockedTaskPolicy, ParticipantName, ParticipantStatusPolicy, TaskPatchPolicy, TaskStatusPolicy, TaskTitle,
};
use crate::repositories::orchestration::{PARTICIPANT_COUNT_SQL, UNBLOCK_CHILDREN_SQL};
use agentforge_core::AgentId;
use serde_json::json;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Task status validation (kanban states)
// ---------------------------------------------------------------------------

#[test]
fn test_valid_task_statuses() {
    let valid = ["backlog", "queued", "working", "blocked", "completed", "failed", "canceled"];
    for s in valid {
        assert!(TaskStatusPolicy::is_valid(s), "should be valid: {s}");
    }
}

#[test]
fn test_invalid_task_statuses() {
    // Legacy status names should now be rejected — they were migrated to queued/working.
    assert!(!TaskStatusPolicy::is_valid("pending"));
    assert!(!TaskStatusPolicy::is_valid("running"));
    assert!(!TaskStatusPolicy::is_valid("unknown"));
    assert!(!TaskStatusPolicy::is_valid(""));
    assert!(!TaskStatusPolicy::is_valid("BACKLOG"));
    assert!(!TaskStatusPolicy::is_valid("cancelled")); // only one L is valid
}

// ---------------------------------------------------------------------------
// Participant status validation
// ---------------------------------------------------------------------------

#[test]
fn test_valid_participant_statuses() {
    let valid = ["available", "busy", "offline"];
    for s in valid {
        assert!(ParticipantStatusPolicy::is_valid(s), "should be valid: {s}");
    }
}

#[test]
fn test_invalid_participant_statuses() {
    assert!(!ParticipantStatusPolicy::is_valid("invalid"));
    assert!(!ParticipantStatusPolicy::is_valid(""));
    assert!(!ParticipantStatusPolicy::is_valid("AVAILABLE"));
    assert!(!ParticipantStatusPolicy::is_valid("online"));
    assert!(!ParticipantStatusPolicy::is_valid("idle"));
}

// ---------------------------------------------------------------------------
// Title validation
// ---------------------------------------------------------------------------

#[test]
fn test_title_valid() {
    assert!(TaskTitle::validate("Valid Task Title").is_ok());
    assert!(TaskTitle::validate("a").is_ok());
    assert!(TaskTitle::validate(&"x".repeat(500)).is_ok());
}

#[test]
fn test_title_empty() {
    assert!(TaskTitle::validate("").is_err());
}

#[test]
fn test_title_too_long() {
    assert!(TaskTitle::validate(&"x".repeat(501)).is_err());
}

// ---------------------------------------------------------------------------
// Participant name validation
// ---------------------------------------------------------------------------

#[test]
fn test_participant_name_valid() {
    assert!(ParticipantName::validate("worker-1").is_ok());
    assert!(ParticipantName::validate("a").is_ok());
    assert!(ParticipantName::validate(&"x".repeat(255)).is_ok());
}

#[test]
fn test_participant_name_empty() {
    assert!(ParticipantName::validate("").is_err());
}

#[test]
fn test_participant_name_too_long() {
    assert!(ParticipantName::validate(&"x".repeat(256)).is_err());
}

// ---------------------------------------------------------------------------
// Dispatch / complete / fail preconditions
// ---------------------------------------------------------------------------

#[test]
fn test_can_dispatch_kanban_states() {
    // Auto-dispatcher claims tasks only after explicit promotion into the
    // runnable lanes. `backlog` is the draft lane and must not auto-start.
    assert!(!TaskStatusPolicy::can_dispatch("backlog"));
    assert!(TaskStatusPolicy::can_dispatch("queued"));
    assert!(TaskStatusPolicy::can_dispatch("blocked"));
    assert!(!TaskStatusPolicy::can_dispatch("working"));
    assert!(!TaskStatusPolicy::can_dispatch("completed"));
    assert!(!TaskStatusPolicy::can_dispatch("failed"));
    assert!(!TaskStatusPolicy::can_dispatch("canceled"));
}

#[test]
fn test_can_complete_or_fail_only_working() {
    assert!(TaskStatusPolicy::can_complete_or_fail("working"));
    assert!(!TaskStatusPolicy::can_complete_or_fail("backlog"));
    assert!(!TaskStatusPolicy::can_complete_or_fail("queued"));
    assert!(!TaskStatusPolicy::can_complete_or_fail("blocked"));
    assert!(!TaskStatusPolicy::can_complete_or_fail("completed"));
    assert!(!TaskStatusPolicy::can_complete_or_fail("failed"));
    assert!(!TaskStatusPolicy::can_complete_or_fail("canceled"));
}

#[test]
fn test_patch_business_transitions_are_not_raw_column_updates() {
    let agent_id = AgentId::from(Uuid::nil());
    let no_assignment = None;
    let unassign = Some(None);
    let assign = Some(Some(agent_id));

    assert!(TaskPatchPolicy::is_business_transition(Some("working"), &no_assignment));
    assert!(TaskPatchPolicy::is_business_transition(Some("completed"), &no_assignment));
    assert!(TaskPatchPolicy::is_business_transition(Some("failed"), &no_assignment));
    assert!(TaskPatchPolicy::is_business_transition(Some("canceled"), &no_assignment));
    assert!(TaskPatchPolicy::is_business_transition(None, &assign));

    assert!(!TaskPatchPolicy::is_business_transition(Some("queued"), &no_assignment));
    assert!(!TaskPatchPolicy::is_business_transition(Some("backlog"), &no_assignment));
    assert!(!TaskPatchPolicy::is_business_transition(Some("blocked"), &no_assignment));
    assert!(!TaskPatchPolicy::is_business_transition(None, &unassign));
}

#[test]
fn test_patch_touches_assignment_for_assign_and_unassign() {
    let agent_id = AgentId::from(Uuid::nil());

    assert!(TaskPatchPolicy::touches_assignment(&Some(Some(agent_id))));
    assert!(TaskPatchPolicy::touches_assignment(&Some(None)));
    assert!(!TaskPatchPolicy::touches_assignment(&None));
}

// ---------------------------------------------------------------------------
// Blocked-distance hint rendering
// ---------------------------------------------------------------------------

#[test]
fn test_blocked_reason_validation() {
    for reason in ["waiting_agent", "waiting_dependency", "waiting_input", "waiting_approval", "quota_exceeded"] {
        assert!(BlockedTaskPolicy::is_valid_reason(reason), "should be valid: {reason}");
    }
    assert!(!BlockedTaskPolicy::is_valid_reason("unknown"));
    assert!(!BlockedTaskPolicy::is_valid_reason(""));
}

#[test]
fn test_blocked_hint_waiting_agent_with_busy() {
    let meta = json!({ "available": 0, "busy": 2, "offline": 1 });
    let hint = BlockedTaskPolicy::hint("waiting_agent", Some(&meta));
    assert!(hint.contains("2"), "should mention busy count: {hint}");
    assert!(hint.contains("1"), "should mention offline count: {hint}");
}

#[test]
fn test_blocked_hint_waiting_agent_no_participants() {
    let meta = json!({ "available": 0, "busy": 0, "offline": 0 });
    let hint = BlockedTaskPolicy::hint("waiting_agent", Some(&meta));
    assert!(hint.contains("没有"), "should call out missing participants: {hint}");
}

#[test]
fn test_blocked_hint_waiting_dependency() {
    let meta = json!({ "pending": 3 });
    let hint = BlockedTaskPolicy::hint("waiting_dependency", Some(&meta));
    assert!(hint.contains("3"), "should mention pending count: {hint}");
}

#[test]
fn test_blocked_hint_waiting_input_with_fields() {
    let meta = json!({ "missing": ["api_key", "model"] });
    let hint = BlockedTaskPolicy::hint("waiting_input", Some(&meta));
    assert!(hint.contains("api_key"), "should list missing fields: {hint}");
    assert!(hint.contains("model"), "should list missing fields: {hint}");
}

#[test]
fn test_blocked_hint_quota_exceeded() {
    let meta = json!({ "used": 1000, "limit": 800 });
    let hint = BlockedTaskPolicy::hint("quota_exceeded", Some(&meta));
    assert!(hint.contains("1000") && hint.contains("800"), "should show usage/limit: {hint}");
}

#[test]
fn test_blocked_hint_unknown_reason_falls_back() {
    let hint = BlockedTaskPolicy::hint("mystery", None);
    assert!(hint.contains("mystery"), "unknown reason should echo: {hint}");
}

// ---------------------------------------------------------------------------
// Dependency-blocking (parent → child gating on create)
// ---------------------------------------------------------------------------

#[test]
fn test_needs_dependency_block_no_parent() {
    // Tasks without a parent are always free to schedule.
    assert!(!BlockedTaskPolicy::needs_dependency_block(None));
}

#[test]
fn test_needs_dependency_block_completed_parent() {
    // A parent that already completed means the child can run immediately.
    assert!(!BlockedTaskPolicy::needs_dependency_block(Some("completed")));
}

#[test]
fn test_needs_dependency_block_unfinished_parent() {
    // Every non-terminal-success parent status gates the child.
    for s in ["backlog", "queued", "working", "blocked"] {
        assert!(BlockedTaskPolicy::needs_dependency_block(Some(s)), "should block on parent status {s}");
    }
}

#[test]
fn test_needs_dependency_block_failed_parent_keeps_child_blocked() {
    // Failed/canceled parents: child stays blocked until a human decides to
    // promote or cancel — don't silently auto-run subtasks of a failed flow.
    assert!(BlockedTaskPolicy::needs_dependency_block(Some("failed")));
    assert!(BlockedTaskPolicy::needs_dependency_block(Some("canceled")));
}

// ---------------------------------------------------------------------------
// SQL query-shape guards (issue #35)
//
// We don't have a DB integration harness yet. These tests pin the WHERE-clause
// invariants of two SQL strings that protect tenant isolation and stale-row
// filtering. If a future edit drops one of these guards, the test fails before
// the change reaches review.
// ---------------------------------------------------------------------------

#[test]
fn test_unblock_children_sql_has_tenant_and_status_guards() {
    // Drop any of these and we either silently unblock the wrong tenant's
    // tasks, unblock children of the wrong parent, or unblock rows that
    // weren't blocked-on-dependency in the first place.
    assert!(
        UNBLOCK_CHILDREN_SQL.contains("organization_id = $1"),
        "tenant guard missing — unblock_children_of would cross-tenant write"
    );
    assert!(
        UNBLOCK_CHILDREN_SQL.contains("parent_task_id = $2"),
        "parent guard missing — unblock_children_of would touch unrelated tasks"
    );
    assert!(
        UNBLOCK_CHILDREN_SQL.contains("status = 'blocked'"),
        "status guard missing — would overwrite non-blocked rows"
    );
    assert!(
        UNBLOCK_CHILDREN_SQL.contains("blocked_reason = 'waiting_dependency'"),
        "blocked_reason guard missing — would unblock rows blocked for other reasons"
    );
}

#[test]
fn test_count_by_status_sql_excludes_stale_offline() {
    // The 24h heartbeat window keeps stale 'offline' rows from polluting the
    // pool-status hint. Drop the window and the UI shows phantom participants.
    assert!(
        PARTICIPANT_COUNT_SQL.contains("organization_id = $1"),
        "tenant guard missing — count_by_status would leak cross-tenant counts"
    );
    assert!(
        PARTICIPANT_COUNT_SQL.contains("status <> 'offline'"),
        "active-status filter missing — stale offline rows would be counted"
    );
    assert!(
        PARTICIPANT_COUNT_SQL.contains("last_heartbeat_at > NOW() - INTERVAL '24 hours'"),
        "24h heartbeat window missing — recently-offline rows would be excluded incorrectly"
    );
}
