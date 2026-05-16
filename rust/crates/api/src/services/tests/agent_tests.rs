use agentforge_core::AgentStatus;

use crate::domain::agent::{AgentCollaboratorPermission, AgentLifecycle, AgentListPage, AgentName};

// ---------------------------------------------------------------------------
// State machine transitions
// ---------------------------------------------------------------------------

#[test]
fn test_all_valid_transitions() {
    // Idle -> Working (user starts working)
    assert!(AgentLifecycle::is_valid_transition(AgentStatus::Idle, AgentStatus::Working));
    // Working -> Idle (task complete)
    assert!(AgentLifecycle::is_valid_transition(AgentStatus::Working, AgentStatus::Idle));
    // Working -> Offline (container dies)
    assert!(AgentLifecycle::is_valid_transition(AgentStatus::Working, AgentStatus::Offline));
    // Offline -> Idle (container recovers)
    assert!(AgentLifecycle::is_valid_transition(AgentStatus::Offline, AgentStatus::Idle));
    // Offline -> Working (direct restart)
    assert!(AgentLifecycle::is_valid_transition(AgentStatus::Offline, AgentStatus::Working));
    // Idle -> Offline (no activity timeout)
    assert!(AgentLifecycle::is_valid_transition(AgentStatus::Idle, AgentStatus::Offline));
}

#[test]
fn test_same_status_is_not_valid_transition() {
    // Same-status is handled as a no-op in the service *before* calling
    // is_valid_transition, so the function itself returns false.
    assert!(!AgentLifecycle::is_valid_transition(AgentStatus::Idle, AgentStatus::Idle));
    assert!(!AgentLifecycle::is_valid_transition(AgentStatus::Working, AgentStatus::Working));
    assert!(!AgentLifecycle::is_valid_transition(AgentStatus::Offline, AgentStatus::Offline));
}

#[test]
fn test_exhaustive_transitions_coverage() {
    // Every possible (from, to) pair — ensures we haven't missed anything.
    let all = [AgentStatus::Idle, AgentStatus::Working, AgentStatus::Offline];
    let mut valid_count = 0;
    for &from in &all {
        for &to in &all {
            if from == to {
                assert!(!AgentLifecycle::is_valid_transition(from, to), "same-status should be false: {from:?}");
            } else {
                // All cross-status transitions are valid per the state machine.
                assert!(AgentLifecycle::is_valid_transition(from, to), "should be valid: {from:?} -> {to:?}");
                valid_count += 1;
            }
        }
    }
    assert_eq!(valid_count, 6); // 3 statuses * 2 possible targets each
}

// ---------------------------------------------------------------------------
// Name validation
// ---------------------------------------------------------------------------

#[test]
fn test_name_validation_none_is_ok() {
    assert!(AgentName::validate(None).is_ok());
}

#[test]
fn test_name_validation_normal_string() {
    assert!(AgentName::validate(Some("my agent")).is_ok());
}

#[test]
fn test_name_validation_empty_is_ok() {
    // Empty name is fine — it's optional.
    assert!(AgentName::validate(Some("")).is_ok());
}

#[test]
fn test_name_validation_max_length_ok() {
    assert!(AgentName::validate(Some(&"x".repeat(255))).is_ok());
}

#[test]
fn test_name_validation_too_long() {
    assert!(AgentName::validate(Some(&"x".repeat(256))).is_err());
}

#[test]
fn test_name_validation_way_too_long() {
    assert!(AgentName::validate(Some(&"x".repeat(10_000))).is_err());
}

// ---------------------------------------------------------------------------
// Pagination helpers
// ---------------------------------------------------------------------------

#[test]
fn test_clamp_limit() {
    assert_eq!(AgentListPage::new(0, 0).limit(), 1);
    assert_eq!(AgentListPage::new(1, 0).limit(), 1);
    assert_eq!(AgentListPage::new(50, 0).limit(), 50);
    assert_eq!(AgentListPage::new(100, 0).limit(), 100);
    assert_eq!(AgentListPage::new(200, 0).limit(), 100);
    assert_eq!(AgentListPage::new(-5, 0).limit(), 1);
    assert_eq!(AgentListPage::new(i64::MIN, 0).limit(), 1);
    assert_eq!(AgentListPage::new(i64::MAX, 0).limit(), 100);
}

#[test]
fn test_floor_offset() {
    assert_eq!(AgentListPage::new(20, -10).offset(), 0);
    assert_eq!(AgentListPage::new(20, 0).offset(), 0);
    assert_eq!(AgentListPage::new(20, 50).offset(), 50);
    assert_eq!(AgentListPage::new(20, i64::MIN).offset(), 0);
    assert_eq!(AgentListPage::new(20, i64::MAX).offset(), i64::MAX);
}

// ---------------------------------------------------------------------------
// Permission validation
// ---------------------------------------------------------------------------

#[test]
fn test_valid_permissions() {
    assert!(AgentCollaboratorPermission::parse("view").is_ok());
    assert!(AgentCollaboratorPermission::parse("edit").is_ok());
    assert!(AgentCollaboratorPermission::parse("admin").is_ok());
}

#[test]
fn test_invalid_permissions() {
    assert!(AgentCollaboratorPermission::parse("").is_err());
    assert!(AgentCollaboratorPermission::parse("owner").is_err());
    assert!(AgentCollaboratorPermission::parse("read").is_err());
    assert!(AgentCollaboratorPermission::parse("write").is_err());
    assert!(AgentCollaboratorPermission::parse("superadmin").is_err());
}
