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

use agentforge_core::event_protocol::{events_ingest_legacy_subject, events_ingest_subject};
use agentforge_core::orchestration_protocol::{
    assignment_consumer_ack_subject_pattern, assignment_consumer_create_subject,
    assignment_consumer_create_subject_kind, assignment_consumer_info_subject, assignment_consumer_next_subject,
    result_subject, result_subject_kind,
};
use agentforge_core::runtime_capability::RuntimeKind;
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
///   - `events.ingest.<kind>.<uuid>` — hook events the sidecar emits, in the
///     #457 kind-namespaced shape (`kind` is THIS agent's `runtime_kind`, so a
///     `cli` agent is never granted a `container` subject).
///   - `events.ingest.<uuid>` — the legacy un-namespaced shape, still granted
///     while pre-#457 agent containers drain. Both shapes are additive; the
///     legacy grant is dropped in a later post-observation deploy.
///   - `sidecar.<uuid>.heartbeat` — liveness pings.
///   - `orchestration.result.<kind>.<uuid>` — task outcomes, in the #457
///     kind-namespaced shape (this agent's own `runtime_kind`), plus the legacy
///     `orchestration.result.<uuid>` retained while pre-#457 containers drain.
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
pub fn build_agent_permissions(agent_id: Uuid, runtime_kind: RuntimeKind) -> NatsPermissions {
    let uuid = agent_id.to_string();
    NatsPermissions {
        pub_allow: vec![
            // #457: kind-namespaced event ingest, scoped to THIS agent's kind.
            events_ingest_subject(runtime_kind, agent_id),
            // Legacy un-namespaced ingest, retained during the migration window.
            events_ingest_legacy_subject(agent_id),
            format!("sidecar.{uuid}.heartbeat"),
            // #457 phase 1b: kind-namespaced result, scoped to THIS agent's kind.
            result_subject_kind(runtime_kind, agent_id),
            // Legacy un-namespaced result, retained during the migration window.
            result_subject(agent_id),
            format!("creds.{uuid}"),
            // Legacy single-filter assignment-consumer CREATE (retained during
            // the #457 phase-1c drain so pre-1c sidecars keep binding).
            assignment_consumer_create_subject(agent_id),
            // #457 phase 1c: kind-namespaced single-filter CREATE for the SAME
            // per-agent durable. The filter token is embedded in the API subject
            // ON PURPOSE — it pins the consumer to this agent's OWN assignment
            // subject. Do NOT collapse to a filter-less CREATE grant (the
            // `filter_subjects` plural form): that lets a rooted sidecar filter
            // another agent's subject and drain its assignments.
            assignment_consumer_create_subject_kind(runtime_kind, agent_id),
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
        let perms = build_agent_permissions(a, RuntimeKind::Container);
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
        let perms_a = build_agent_permissions(a, RuntimeKind::Container);
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
        let perms = build_agent_permissions(a, RuntimeKind::Container);
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
        let perms = build_agent_permissions(uuid_of("11111111-2222-3333-4444-555555555555"), RuntimeKind::Container);
        for needle in ["$SYS.>", "broadcast.>"] {
            assert!(perms.pub_deny.iter().any(|s| s == needle), "pub_deny missing {needle}");
            assert!(perms.sub_deny.iter().any(|s| s == needle), "sub_deny missing {needle}");
        }
        assert!(perms.pub_deny.iter().any(|s| s == "orchestration.assigned.>"), "pub_deny missing task injection deny");
    }

    /// A `broadcast.>` deny entry NATS-covers `subject` (prefix `broadcast.`
    /// plus at least one more token, which `>` matches).
    fn broadcast_wildcard_covers(subject: &str) -> bool {
        subject.strip_prefix("broadcast.").is_some_and(|rest| !rest.is_empty())
    }

    #[test]
    fn admin_cli_image_toast_subject_is_denied_for_agents() {
        // The admin CLI-image toast rides `broadcast.admin.cli_image`. A rooted
        // sidecar must be able to neither READ it (leak) nor PUBLISH it (spoof a
        // fake "updated" toast). Both are covered by the `broadcast.>` deny;
        // pin it so a future grant refactor can't silently expose the subject.
        use agentforge_core::broadcast_protocol::ADMIN_CLI_IMAGE_SUBJECT;
        assert!(broadcast_wildcard_covers(ADMIN_CLI_IMAGE_SUBJECT));
        let perms = build_agent_permissions(uuid_of("11111111-2222-3333-4444-555555555555"), RuntimeKind::Container);
        assert!(perms.pub_deny.iter().any(|s| s == "broadcast.>"), "agent could spoof the admin toast");
        assert!(perms.sub_deny.iter().any(|s| s == "broadcast.>"), "agent could read the admin toast");
    }

    #[test]
    fn deny_list_is_identical_for_different_agents() {
        // Static invariant: the deny list is the same shape regardless of
        // which agent we mint perms for. Only the pub/sub allow lists
        // template the UUID.
        let a = build_agent_permissions(uuid_of("11111111-2222-3333-4444-555555555555"), RuntimeKind::Container);
        let b = build_agent_permissions(uuid_of("99999999-aaaa-bbbb-cccc-dddddddddddd"), RuntimeKind::Container);
        assert_eq!(a.pub_deny, b.pub_deny);
        assert_eq!(a.sub_deny, b.sub_deny);
    }

    #[test]
    fn inbox_subject_is_in_both_allow_lists() {
        // _INBOX.> is required for request/reply round-trips on both
        // directions. Its inclusion is deliberate, not a leak.
        let perms = build_agent_permissions(uuid_of("11111111-2222-3333-4444-555555555555"), RuntimeKind::Container);
        assert!(perms.pub_allow.iter().any(|s| s == "_INBOX.>"));
        assert!(perms.sub_allow.iter().any(|s| s == "_INBOX.>"));
    }

    #[test]
    fn permissions_allow_publish_on_own_creds_subject() {
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        let perms = build_agent_permissions(a, RuntimeKind::Container);
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
        let perms = build_agent_permissions(a, RuntimeKind::Container);
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
        let perms_a = build_agent_permissions(a, RuntimeKind::Container);
        let bad = format!("creds.{b}");
        assert!(
            !perms_a.pub_allow.iter().any(|s| s == &bad || s == "creds.>" || s == "creds.*"),
            "agent A's creds pub_allow leaked another agent or a wildcard",
        );
    }

    #[test]
    fn namespaced_event_subject_uses_agents_own_kind() {
        // #457: the kind-namespaced ingest grant carries the agent's own
        // runtime_kind, not a hard-coded one.
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        for (kind, token) in
            [(RuntimeKind::Container, "container"), (RuntimeKind::Cli, "cli"), (RuntimeKind::Api, "api")]
        {
            let perms = build_agent_permissions(a, kind);
            let expected = format!("events.ingest.{token}.11111111-2222-3333-4444-555555555555");
            assert!(
                perms.pub_allow.iter().any(|s| s == &expected),
                "missing kind-namespaced ingest grant {expected} for {kind:?}",
            );
        }
    }

    #[test]
    fn legacy_event_subject_still_granted_during_migration() {
        // The un-namespaced ingest subject must remain granted so agent
        // containers built before #457 keep publishing successfully until the
        // legacy-drop deploy. Verified for every kind.
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        for kind in [RuntimeKind::Container, RuntimeKind::Cli, RuntimeKind::Api] {
            let perms = build_agent_permissions(a, kind);
            assert!(
                perms.pub_allow.iter().any(|s| s == "events.ingest.11111111-2222-3333-4444-555555555555"),
                "legacy ingest grant dropped for {kind:?} — would break draining containers",
            );
        }
    }

    #[test]
    fn namespaced_result_subject_uses_agents_own_kind() {
        // #457 phase 1b: the result grant carries the agent's own runtime_kind.
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        for (kind, token) in
            [(RuntimeKind::Container, "container"), (RuntimeKind::Cli, "cli"), (RuntimeKind::Api, "api")]
        {
            let perms = build_agent_permissions(a, kind);
            let expected = format!("orchestration.result.{token}.11111111-2222-3333-4444-555555555555");
            assert!(perms.pub_allow.iter().any(|s| s == &expected), "missing namespaced result grant {expected}");
        }
    }

    #[test]
    fn legacy_result_subject_still_granted_during_migration() {
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        for kind in [RuntimeKind::Container, RuntimeKind::Cli, RuntimeKind::Api] {
            let perms = build_agent_permissions(a, kind);
            assert!(
                perms.pub_allow.iter().any(|s| s == "orchestration.result.11111111-2222-3333-4444-555555555555"),
                "legacy result grant dropped for {kind:?} — would break draining containers",
            );
        }
    }

    #[test]
    fn agent_is_not_granted_other_kinds_result_subjects() {
        // A cli agent must never be granted a container/api result subject nor a
        // result kind-wildcard.
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        let perms = build_agent_permissions(a, RuntimeKind::Cli);
        for forbidden_token in ["container", "api"] {
            let forbidden = format!("orchestration.result.{forbidden_token}.11111111-2222-3333-4444-555555555555");
            assert!(
                !perms.pub_allow.iter().any(|s| s == &forbidden),
                "cli agent granted other kind's result: {forbidden}"
            );
        }
        assert!(!perms.pub_allow.iter().any(|s| s == "orchestration.result.>" || s == "orchestration.result.*"));
    }

    #[test]
    fn assignment_create_grant_is_namespaced_and_legacy_single_filter() {
        // #457 phase 1c: each agent gets BOTH the legacy and the kind-namespaced
        // single-filter CONSUMER.CREATE grant for its OWN durable.
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        let perms = build_agent_permissions(a, RuntimeKind::Cli);
        assert!(
            perms.pub_allow.iter().any(|s| s == &assignment_consumer_create_subject(a)),
            "legacy assignment-create grant must remain during drain",
        );
        assert!(
            perms.pub_allow.iter().any(|s| s == &assignment_consumer_create_subject_kind(RuntimeKind::Cli, a)),
            "namespaced assignment-create grant for own kind must be present",
        );
    }

    #[test]
    fn assignment_create_grant_is_not_cross_kind_and_never_filter_less() {
        // Security: a cli agent must NOT get another kind's assignment-create
        // grant, and — critically — must NEVER get a FILTER-LESS create grant
        // (`$JS.API.CONSUMER.CREATE.<stream>.<durable>` with no trailing filter).
        // The filter-less form is what `filter_subjects` plural needs, and it
        // lets a rooted sidecar create a consumer under its own durable name but
        // filtering ANOTHER agent's subject — draining that agent's assignments.
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        let perms = build_agent_permissions(a, RuntimeKind::Cli);
        for forbidden in [
            assignment_consumer_create_subject_kind(RuntimeKind::Container, a),
            assignment_consumer_create_subject_kind(RuntimeKind::Api, a),
        ] {
            assert!(
                !perms.pub_allow.iter().any(|s| s == &forbidden),
                "cli agent granted other kind's assignment-create: {forbidden}"
            );
        }
        // The filter-less create form: CREATE.<stream>.<durable> with nothing after.
        let filter_less = format!(
            "$JS.API.CONSUMER.CREATE.ORCHESTRATION_ASSIGNMENTS.{}",
            agentforge_core::orchestration_protocol::assignment_consumer_name(a)
        );
        assert!(
            !perms.pub_allow.iter().any(|s| s == &filter_less),
            "filter-less assignment-create grant present — cross-agent assignment theft vector",
        );
        // Every granted assignment-create subject must carry a trailing filter
        // ending in THIS agent's own uuid.
        for s in perms.pub_allow.iter().filter(|s| s.starts_with("$JS.API.CONSUMER.CREATE.ORCHESTRATION_ASSIGNMENTS")) {
            assert!(s.ends_with(&a.to_string()), "assignment-create grant not pinned to own uuid: {s}");
        }
    }

    #[test]
    fn agent_is_not_granted_other_kinds_event_subjects() {
        // The security point of #457: a `cli` agent's JWT must NOT carry a
        // `container`- or `api`-namespaced ingest subject (and vice versa).
        let a = uuid_of("11111111-2222-3333-4444-555555555555");
        let perms = build_agent_permissions(a, RuntimeKind::Cli);
        for forbidden_token in ["container", "api"] {
            let forbidden = format!("events.ingest.{forbidden_token}.11111111-2222-3333-4444-555555555555");
            assert!(
                !perms.pub_allow.iter().any(|s| s == &forbidden),
                "cli agent was granted another kind's ingest subject: {forbidden}",
            );
        }
        // And never a kind wildcard.
        assert!(!perms.pub_allow.iter().any(|s| s == "events.ingest.>" || s == "events.ingest.*"));
    }
}
