//! Subject taxonomy for the agent -> platform event-ingest channel.
//!
//! Issue #457 phase 1 namespaces the event-ingest subject by the publishing
//! agent's `runtime_kind` so the per-agent NATS grant, JetStream routing, and
//! operational dashboards can tell container / cli / api traffic apart.
//!
//! Two subject shapes coexist during the migration window:
//!
//! - legacy:     `events.ingest.<agent_uuid>`                (3 tokens)
//! - namespaced: `events.ingest.<runtime_kind>.<agent_uuid>` (4 tokens)
//!
//! New sidecars publish ONLY the namespaced shape. The platform consumer
//! accepts both — the `EVENTS` stream subject (`events.ingest.>`) and the
//! `events.>` consumer filter already capture the 4-token shape, so no
//! JetStream reconfiguration is needed — and counts every legacy receipt so
//! operators can watch the legacy tail drain to zero before the eventual
//! legacy-drop deploy. The legacy shapes for the `orchestration.result` and
//! `orchestration.assigned` subjects are intentionally NOT changed in this
//! phase; they keep single-token (`.*`) stream wildcards and are tracked as
//! phase 1b. See `docs/architecture/nats-subjects.md`.

use crate::runtime_capability::RuntimeKind;
use uuid::Uuid;

/// Subject prefix shared by both the legacy and namespaced shapes.
pub const EVENTS_INGEST_PREFIX: &str = "events.ingest";

/// Namespaced publish subject a sidecar of `kind` emits for `agent_id`:
/// `events.ingest.<kind>.<agent_uuid>`. This is what current sidecars publish.
pub fn events_ingest_subject(kind: RuntimeKind, agent_id: Uuid) -> String {
    format!("{EVENTS_INGEST_PREFIX}.{}.{}", kind.as_str(), agent_id)
}

/// Legacy publish subject (pre-#457): `events.ingest.<agent_uuid>`. Retained so
/// the callout can keep granting it while older agent containers drain.
pub fn events_ingest_legacy_subject(agent_id: Uuid) -> String {
    format!("{EVENTS_INGEST_PREFIX}.{agent_id}")
}

/// A successfully parsed event-ingest subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedEventSubject {
    /// Trailing agent UUID. Always the message's owning agent regardless of
    /// shape, so the consumer's `subject vs envelope` cross-check is stable.
    pub agent_id: Uuid,
    /// `Some(kind)` for the namespaced shape; `None` for the legacy shape.
    pub runtime_kind: Option<RuntimeKind>,
}

impl ParsedEventSubject {
    /// True when the subject used the pre-#457 (un-namespaced) shape. Drives
    /// the `agentforge_nats_legacy_subject_received_total` drain metric.
    pub fn is_legacy(&self) -> bool {
        self.runtime_kind.is_none()
    }
}

/// Parse a received event-ingest subject into `(agent_id, runtime_kind?)`.
///
/// Accepts exactly the two supported shapes. Anything else — wrong prefix,
/// extra trailing tokens, an unknown runtime-kind token, or a non-UUID tail —
/// returns `None` so the caller rejects the message as a forged/unsupported
/// subject rather than guessing an identity.
pub fn parse_events_ingest_subject(subject: &str) -> Option<ParsedEventSubject> {
    let rest = subject.strip_prefix(EVENTS_INGEST_PREFIX)?.strip_prefix('.')?;
    let mut tokens = rest.split('.');
    match (tokens.next(), tokens.next(), tokens.next()) {
        // legacy: events.ingest.<uuid>
        (Some(uuid), None, None) => {
            let agent_id = Uuid::parse_str(uuid).ok()?;
            Some(ParsedEventSubject { agent_id, runtime_kind: None })
        }
        // namespaced: events.ingest.<kind>.<uuid>
        (Some(kind), Some(uuid), None) => {
            let kind = RuntimeKind::parse_legacy(kind).ok()?;
            let agent_id = Uuid::parse_str(uuid).ok()?;
            Some(ParsedEventSubject { agent_id, runtime_kind: Some(kind) })
        }
        // 3+ trailing tokens, or empty — not a recognised shape.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Uuid {
        Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap()
    }

    #[test]
    fn namespaced_builder_matches_shape() {
        assert_eq!(
            events_ingest_subject(RuntimeKind::Cli, id()),
            "events.ingest.cli.11111111-2222-3333-4444-555555555555"
        );
        assert_eq!(
            events_ingest_subject(RuntimeKind::Container, id()),
            "events.ingest.container.11111111-2222-3333-4444-555555555555"
        );
    }

    #[test]
    fn legacy_builder_matches_shape() {
        assert_eq!(events_ingest_legacy_subject(id()), "events.ingest.11111111-2222-3333-4444-555555555555");
    }

    #[test]
    fn parses_legacy_shape_as_legacy() {
        let parsed = parse_events_ingest_subject("events.ingest.11111111-2222-3333-4444-555555555555").unwrap();
        assert_eq!(parsed.agent_id, id());
        assert_eq!(parsed.runtime_kind, None);
        assert!(parsed.is_legacy());
    }

    #[test]
    fn parses_namespaced_shape_with_kind() {
        for (kind_token, kind) in
            [("container", RuntimeKind::Container), ("cli", RuntimeKind::Cli), ("api", RuntimeKind::Api)]
        {
            let subject = format!("events.ingest.{kind_token}.11111111-2222-3333-4444-555555555555");
            let parsed = parse_events_ingest_subject(&subject).unwrap();
            assert_eq!(parsed.agent_id, id());
            assert_eq!(parsed.runtime_kind, Some(kind));
            assert!(!parsed.is_legacy());
        }
    }

    #[test]
    fn roundtrips_namespaced_builder_through_parser() {
        let subject = events_ingest_subject(RuntimeKind::Cli, id());
        let parsed = parse_events_ingest_subject(&subject).unwrap();
        assert_eq!(parsed.agent_id, id());
        assert_eq!(parsed.runtime_kind, Some(RuntimeKind::Cli));
    }

    #[test]
    fn rejects_unknown_kind_token() {
        // A bogus middle token is NOT silently accepted as a legacy uuid:
        // `events.ingest.bogus.<uuid>` must be rejected, not mis-attributed.
        assert!(parse_events_ingest_subject("events.ingest.bogus.11111111-2222-3333-4444-555555555555").is_none());
    }

    #[test]
    fn rejects_wrong_prefix() {
        assert!(parse_events_ingest_subject("events.broadcast.11111111-2222-3333-4444-555555555555").is_none());
        assert!(parse_events_ingest_subject("orchestration.result.11111111-2222-3333-4444-555555555555").is_none());
    }

    #[test]
    fn rejects_non_uuid_tail() {
        assert!(parse_events_ingest_subject("events.ingest.not-a-uuid").is_none());
        assert!(parse_events_ingest_subject("events.ingest.cli.not-a-uuid").is_none());
        // kind token present but no uuid at all.
        assert!(parse_events_ingest_subject("events.ingest.cli").is_none());
    }

    #[test]
    fn rejects_extra_trailing_tokens() {
        assert!(parse_events_ingest_subject("events.ingest.cli.11111111-2222-3333-4444-555555555555.extra").is_none());
    }

    #[test]
    fn rejects_wildcards() {
        // Defense: a subscriber must never resolve a wildcard to a concrete id.
        assert!(parse_events_ingest_subject("events.ingest.>").is_none());
        assert!(parse_events_ingest_subject("events.ingest.*").is_none());
        assert!(parse_events_ingest_subject("events.ingest.cli.*").is_none());
    }
}
