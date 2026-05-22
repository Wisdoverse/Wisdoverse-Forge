//! Per-agent NATS subject permission builder (issue #38 phase 2).
//!
//! Every subject in Wisdoverse Forge's workload protocol ends in an
//! `<agent_uuid>` segment (confirmed against the full subject taxonomy in
//! `rust/crates/core/src/orchestration_protocol.rs`, `rust/bins/sidecar/src/`
//! and `rust/crates/jobs/src/event_consumer.rs`). This module templates the
//! connecting agent's UUID into each subject so the inner User JWT the
//! callout service mints can ONLY publish or subscribe on subjects scoped
//! to that agent's own UUID — no wildcards, no other-tenant subjects.
//!
//! The DENY lists are static (same for every agent): they lock out
//! `$SYS.>`, `$SRV.>`, `broadcast.>`, and the publish half of
//! `orchestration.assigned.>` (task injection would let a compromised
//! sidecar hand itself another tenant's task).

use agentforge_core::orchestration_protocol::{
    assignment_consumer_ack_subject_pattern, assignment_consumer_create_subject, assignment_consumer_info_subject,
    assignment_consumer_next_subject,
};
use uuid::Uuid;

use crate::domain::auth_callout::NatsPermissions;

/// Build the scoped publish/subscribe allowlist for a specific agent.
///
/// The returned `NatsPermissions` goes straight into the inner User JWT
/// the callout service signs; NATS enforces these permissions for the
/// lifetime of the connection (or until JWT expiry triggers re-auth).
///
/// Subjects templated with the agent's UUID:
///
/// Publish allow:
///   - `events.ingest.<uuid>` — hook events the sidecar emits.
///   - `sidecar.<uuid>.heartbeat` — liveness pings.
///   - `orchestration.result.<uuid>` — task outcomes.
///   - `creds.<uuid>` — Container CLI credential sync (issue #41).
///   - the exact four JetStream subjects needed to create/read/pull/ack the
///     sidecar's own durable assignment consumer
///   - `_INBOX.>` — NATS request-reply scratch namespace (not per-agent;
///     NATS itself enforces reply-subject ownership at the client level).
///
/// Subscribe allow:
///   - `sidecar.<uuid>.cmd` — backend-issued admin commands.
///   - `orchestration.assigned.<uuid>` — task assignments.
///   - `_INBOX.>` — reply subjects for outbound requests.
///
/// Static deny (every JWT, regardless of UUID):
///   - `$SYS.>`, `$SRV.>` — NATS system + service APIs. No workload user
///     should touch these.
///   - `broadcast.>` — per-tenant WebSocket fanout. Reading it would
///     exfiltrate every org's event stream (the primary phase-1
///     finding).
///   - `orchestration.assigned.>` (publish only) — agents must not
///     inject tasks into each other's queues.
pub fn build_agent_permissions(agent_id: Uuid) -> NatsPermissions {
    let uuid = agent_id.to_string();
    NatsPermissions {
        pub_allow: vec![
            format!("events.ingest.{uuid}"),
            format!("sidecar.{uuid}.heartbeat"),
            format!("orchestration.result.{uuid}"),
            format!("creds.{uuid}"),
            assignment_consumer_create_subject(agent_id),
            assignment_consumer_info_subject(agent_id),
            assignment_consumer_next_subject(agent_id),
            assignment_consumer_ack_subject_pattern(agent_id),
            "_INBOX.>".to_string(),
        ],
        pub_deny: vec![
            "$SYS.>".to_string(),
            "$SRV.>".to_string(),
            "broadcast.>".to_string(),
            "orchestration.assigned.>".to_string(),
        ],
        sub_allow: vec![
            format!("sidecar.{uuid}.cmd"),
            format!("orchestration.assigned.{uuid}"),
            "_INBOX.>".to_string(),
        ],
        sub_deny: vec!["$SYS.>".to_string(), "$SRV.>".to_string(), "broadcast.>".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid_of(hex: &str) -> Uuid {
        Uuid::parse_str(hex).unwrap()
    }

    #[test]
    fn permissions_contain_own_uuid_subjects() {
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        let perms = build_agent_permissions(a);
        assert!(perms.pub_allow.iter().any(|s| s == "events.ingest.11111111-2222-3333-4444-555555555555"));
        assert!(perms.pub_allow.iter().any(|s| s == "sidecar.11111111-2222-3333-4444-555555555555.heartbeat"));
        assert!(perms.pub_allow.iter().any(|s| s == "orchestration.result.11111111-2222-3333-4444-555555555555"));
        assert!(perms.pub_allow.iter().any(|s| s == &assignment_consumer_create_subject(a)));
        assert!(perms.pub_allow.iter().any(|s| s == &assignment_consumer_info_subject(a)));
        assert!(perms.pub_allow.iter().any(|s| s == &assignment_consumer_next_subject(a)));
        assert!(perms.pub_allow.iter().any(|s| s == &assignment_consumer_ack_subject_pattern(a)));
        assert!(perms.sub_allow.iter().any(|s| s == "sidecar.11111111-2222-3333-4444-555555555555.cmd"));
        assert!(perms.sub_allow.iter().any(|s| s == "orchestration.assigned.11111111-2222-3333-4444-555555555555"));
    }

    #[test]
    fn permissions_do_not_contain_other_agents_subjects() {
        // Cross-agent spoofing — the exact gap phase 2 closes. Agent A's
        // JWT must NOT list agent B's subjects in allow.
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        let b = uuid_of("99999999-aaaa-bbbb-cccc-dddddddddddd");
        let perms_a = build_agent_permissions(a);
        let b_str = b.to_string();
        for s in perms_a.pub_allow.iter().chain(perms_a.sub_allow.iter()) {
            if s == "_INBOX.>" {
                continue;
            }
            assert!(!s.contains(&b_str), "agent A's perms leaked agent B's UUID: {s}");
        }
    }

    #[test]
    fn permissions_contain_no_wildcards_in_per_agent_segments() {
        // Structural guard: the per-agent subjects must be exact UUIDs,
        // not `>` or `*`. Wildcards would reintroduce the phase-1 gap.
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        let perms = build_agent_permissions(a);
        let assignment_ack = assignment_consumer_ack_subject_pattern(a);
        for s in &perms.pub_allow {
            // _INBOX.> is the exception — inbox reply subjects are not
            // per-agent and the NATS server handles the ownership check.
            //
            // JetStream pull-consumer ACKs also require a terminal wildcard,
            // but the durable consumer name embeds the agent UUID. That keeps
            // the wildcard inside the agent's own assignment consumer scope.
            if s == "_INBOX.>" || s == &assignment_ack {
                continue;
            }
            assert!(!s.ends_with(".>"), "per-agent pub_allow has wildcard: {s}");
            assert!(!s.ends_with(".*"), "per-agent pub_allow has wildcard: {s}");
        }
        for s in &perms.sub_allow {
            if s == "_INBOX.>" {
                continue;
            }
            assert!(!s.ends_with(".>"), "per-agent sub_allow has wildcard: {s}");
            assert!(!s.ends_with(".*"), "per-agent sub_allow has wildcard: {s}");
        }
    }

    #[test]
    fn deny_list_always_contains_sys_and_broadcast() {
        // Invariant across every agent. Phase-1 deny-by-omission becomes
        // explicit deny here so a future refactor can't accidentally
        // unlock `broadcast.>` or `$SYS.>` for a single UUID.
        let perms = build_agent_permissions(uuid_of("11111111-2222-3333-4444-555555555555"));
        for needle in ["$SYS.>", "broadcast.>"] {
            assert!(perms.pub_deny.iter().any(|s| s == needle), "pub_deny missing {needle}");
            assert!(perms.sub_deny.iter().any(|s| s == needle), "sub_deny missing {needle}");
        }
        assert!(perms.pub_deny.iter().any(|s| s == "orchestration.assigned.>"), "pub_deny missing task injection deny");
    }

    #[test]
    fn deny_list_is_identical_for_different_agents() {
        // Static invariant: the deny list is the same shape regardless of
        // which agent we mint perms for. Only the pub/sub allow lists
        // template the UUID.
        let a = build_agent_permissions(uuid_of("11111111-2222-3333-4444-555555555555"));
        let b = build_agent_permissions(uuid_of("99999999-aaaa-bbbb-cccc-dddddddddddd"));
        assert_eq!(a.pub_deny, b.pub_deny);
        assert_eq!(a.sub_deny, b.sub_deny);
    }

    #[test]
    fn inbox_subject_is_in_both_allow_lists() {
        // _INBOX.> is required for request/reply round-trips on both
        // directions. Its inclusion is deliberate, not a leak.
        let perms = build_agent_permissions(uuid_of("11111111-2222-3333-4444-555555555555"));
        assert!(perms.pub_allow.iter().any(|s| s == "_INBOX.>"));
        assert!(perms.sub_allow.iter().any(|s| s == "_INBOX.>"));
    }

    #[test]
    fn permissions_allow_publish_on_own_creds_subject() {
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        let perms = build_agent_permissions(a);
        assert!(
            perms.pub_allow.iter().any(|s| s == "creds.11111111-2222-3333-4444-555555555555"),
            "credential sync publish (issue #41) must be in pub_allow",
        );
    }

    #[test]
    fn permissions_do_not_allow_subscribing_to_creds() {
        // Only the backend consumer reads `creds.>` — sidecars must never
        // subscribe (that would let a rooted container read another tenant's
        // credentials on the shared stream).
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        let perms = build_agent_permissions(a);
        for s in &perms.sub_allow {
            assert!(!s.starts_with("creds."), "sub_allow leaks creds subject: {s}");
        }
    }

    #[test]
    fn permissions_do_not_allow_publishing_to_other_creds_subjects() {
        // Structural: the per-agent pub list templates the UUID. A bug that
        // swapped to a wildcard would let any agent overwrite any tenant's
        // credentials. Guard with explicit cross-agent check.
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        let b = uuid_of("99999999-aaaa-bbbb-cccc-dddddddddddd");
        let perms_a = build_agent_permissions(a);
        let bad = format!("creds.{b}");
        assert!(
            !perms_a.pub_allow.iter().any(|s| s == &bad || s == "creds.>" || s == "creds.*"),
            "agent A's creds pub_allow leaked another agent or a wildcard",
        );
    }
}
