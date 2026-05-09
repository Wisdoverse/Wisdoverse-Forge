use agentforge_core::AgentStatus;

use crate::services::agent::{
    clamp_limit, floor_offset, is_valid_transition, validate_agent_name, validate_permission,
};

// ---------------------------------------------------------------------------
// State machine transitions
// ---------------------------------------------------------------------------

#[test]
fn test_all_valid_transitions() {
    // Idle -> Working (user starts working)
    assert!(is_valid_transition(AgentStatus::Idle, AgentStatus::Working));
    // Working -> Idle (task complete)
    assert!(is_valid_transition(AgentStatus::Working, AgentStatus::Idle));
    // Working -> Offline (container dies)
    assert!(is_valid_transition(AgentStatus::Working, AgentStatus::Offline));
    // Offline -> Idle (container recovers)
    assert!(is_valid_transition(AgentStatus::Offline, AgentStatus::Idle));
    // Offline -> Working (direct restart)
    assert!(is_valid_transition(AgentStatus::Offline, AgentStatus::Working));
    // Idle -> Offline (no activity timeout)
    assert!(is_valid_transition(AgentStatus::Idle, AgentStatus::Offline));
}

#[test]
fn test_same_status_is_not_valid_transition() {
    // Same-status is handled as a no-op in the service *before* calling
    // is_valid_transition, so the function itself returns false.
    assert!(!is_valid_transition(AgentStatus::Idle, AgentStatus::Idle));
    assert!(!is_valid_transition(AgentStatus::Working, AgentStatus::Working));
    assert!(!is_valid_transition(AgentStatus::Offline, AgentStatus::Offline));
}

#[test]
fn test_exhaustive_transitions_coverage() {
    // Every possible (from, to) pair — ensures we haven't missed anything.
    let all = [AgentStatus::Idle, AgentStatus::Working, AgentStatus::Offline];
    let mut valid_count = 0;
    for &from in &all {
        for &to in &all {
            if from == to {
                assert!(!is_valid_transition(from, to), "same-status should be false: {from:?}");
            } else {
                // All cross-status transitions are valid per the state machine.
                assert!(is_valid_transition(from, to), "should be valid: {from:?} -> {to:?}");
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
    assert!(validate_agent_name(None).is_ok());
}

#[test]
fn test_name_validation_normal_string() {
    assert!(validate_agent_name(Some("my agent")).is_ok());
}

#[test]
fn test_name_validation_empty_is_ok() {
    // Empty name is fine — it's optional.
    assert!(validate_agent_name(Some("")).is_ok());
}

#[test]
fn test_name_validation_max_length_ok() {
    assert!(validate_agent_name(Some(&"x".repeat(255))).is_ok());
}

#[test]
fn test_name_validation_too_long() {
    assert!(validate_agent_name(Some(&"x".repeat(256))).is_err());
}

#[test]
fn test_name_validation_way_too_long() {
    assert!(validate_agent_name(Some(&"x".repeat(10_000))).is_err());
}

// ---------------------------------------------------------------------------
// Pagination helpers
// ---------------------------------------------------------------------------

#[test]
fn test_clamp_limit() {
    assert_eq!(clamp_limit(0), 1);
    assert_eq!(clamp_limit(1), 1);
    assert_eq!(clamp_limit(50), 50);
    assert_eq!(clamp_limit(100), 100);
    assert_eq!(clamp_limit(200), 100);
    assert_eq!(clamp_limit(-5), 1);
    assert_eq!(clamp_limit(i64::MIN), 1);
    assert_eq!(clamp_limit(i64::MAX), 100);
}

#[test]
fn test_floor_offset() {
    assert_eq!(floor_offset(-10), 0);
    assert_eq!(floor_offset(0), 0);
    assert_eq!(floor_offset(50), 50);
    assert_eq!(floor_offset(i64::MIN), 0);
    assert_eq!(floor_offset(i64::MAX), i64::MAX);
}

// ---------------------------------------------------------------------------
// Permission validation
// ---------------------------------------------------------------------------

#[test]
fn test_valid_permissions() {
    assert!(validate_permission("view").is_ok());
    assert!(validate_permission("edit").is_ok());
    assert!(validate_permission("admin").is_ok());
}

#[test]
fn test_invalid_permissions() {
    assert!(validate_permission("").is_err());
    assert!(validate_permission("owner").is_err());
    assert!(validate_permission("read").is_err());
    assert!(validate_permission("write").is_err());
    assert!(validate_permission("superadmin").is_err());
}
