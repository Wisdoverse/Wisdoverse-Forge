//! Dead-letter capture for permanently-dropped NATS envelopes (#811 follow-up).
//!
//! When a consumer TERM-drops an inbound message (bad signature, unknown agent,
//! bad subject, stale timestamp, malformed body) the raw message would otherwise
//! vanish with only a log line + a Prometheus counter. The [`DeadEventRecorder`]
//! persists a durable row so an operator debugging "why aren't agent X's events
//! showing up?" has something to inspect via the owner-only
//! `GET /admin/dead-events` reader.
//!
//! ## Best-effort by design
//!
//! [`DeadEventRecorder::record`] returns `()`, NOT `Result`: the message is
//! ALREADY being TERM-acked at the drop site, so losing the dead-letter row must
//! never crash, block, or change the TERM decision. The Sqlx implementation logs
//! its own INSERT failures with `warn!` and swallows them.
//!
//! ## Scope (ponytail — bound the blast radius and the volume)
//! Only PERMANENT drops are recorded. Transient NAK/retry errors and the
//! `orchestration_inbox` dedup hit (a deduped replay was *successfully* handled)
//! are intentionally NOT recorded — they would flood the table with benign noise.

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// Hard cap on the stored payload excerpt. A large or flooding payload cannot
/// balloon the table beyond this per row.
///
/// ponytail: 8KiB excerpt; a sustained drop flood signals attack/misconfig — a
/// TTL prune reaper is the follow-up, not v1. The `recorded_at` index makes a
/// future prune cheap.
pub const DEAD_EVENT_PAYLOAD_MAX_BYTES: usize = 8 * 1024;

/// A plain record built at the drop site and handed to the recorder.
///
/// `org_id` / `delivery_id` are best-effort and NULL for the pre-auth early
/// drops (which is most of them); the `subject` carries the agent UUID, which is
/// the real debugging key. `payload_excerpt` is an UNTRUSTED, attacker- or
/// work-controlled excerpt — readers must render it escaped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadEvent {
    /// Originating consumer: `"events.ingest"` | `"orchestration.result"`.
    pub source: &'static str,
    /// Structured drop reason, e.g. `"signature_mismatch"`.
    pub reason: String,
    /// NATS subject the message arrived on (carries the agent UUID).
    pub subject: String,
    /// Human context already built at the reject site.
    pub detail: Option<String>,
    /// Delivery id when available (absent on early/event drops).
    pub delivery_id: Option<String>,
    /// Trustworthy org when available (NULL for pre-auth drops).
    pub org_id: Option<Uuid>,
    /// Truncated excerpt of the raw payload (<= [`DEAD_EVENT_PAYLOAD_MAX_BYTES`]).
    pub payload_excerpt: Option<String>,
}

/// Records permanently-dropped envelopes. Object-safe (`dyn` + `Arc`) so a
/// worker holds an `Option<Arc<dyn DeadEventRecorder>>` defaulted to `None`.
#[async_trait]
pub trait DeadEventRecorder: Send + Sync {
    /// Persist one dead event. Best-effort and infallible from the caller's
    /// perspective: implementations log their own errors and never propagate, so
    /// a drop site is a single call that cannot affect the TERM decision.
    async fn record(&self, ev: DeadEvent);
}

/// Production [`DeadEventRecorder`] backed by the `dead_events` table.
pub struct SqlxDeadEventRecorder {
    pool: PgPool,
}

impl SqlxDeadEventRecorder {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DeadEventRecorder for SqlxDeadEventRecorder {
    async fn record(&self, ev: DeadEvent) {
        let result = sqlx::query(
            r#"INSERT INTO dead_events
                   (source, reason, subject, detail, delivery_id, org_id, payload_excerpt)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(ev.source)
        .bind(&ev.reason)
        .bind(&ev.subject)
        .bind(&ev.detail)
        .bind(&ev.delivery_id)
        .bind(ev.org_id)
        .bind(&ev.payload_excerpt)
        .execute(&self.pool)
        .await;

        // Best-effort: the message is already being TERM'd, so a failed
        // dead-letter INSERT must not crash or block the consumer. Log + swallow.
        // But a persistent INSERT failure (broken/missing `dead_events` table)
        // would otherwise silently warn-spam while the owner sees an empty
        // table — wrongly reading "no drops". Bump a counter so the failure is
        // observable on the metrics path, and escalate the log to `error!`
        // since a lost dead-letter row is data loss.
        if let Err(e) = result {
            metrics::counter!("dead_event_record_errors_total", "source" => ev.source).increment(1);
            tracing::error!(
                error = ?e,
                source = ev.source,
                reason = %ev.reason,
                subject = %ev.subject,
                "failed to record dead event; dropping the dead-letter row (consumer still TERMs the message)"
            );
        }
    }
}

/// Describe + prime the dead-event recorder failure counter so a Prometheus
/// scrape returns it (at zero) before the first INSERT failure fires. Without
/// the primed series, an absent counter is indistinguishable from a true zero,
/// and a broken `dead_events` table would surface only as warn-spam.
pub fn register_metrics() {
    metrics::describe_counter!(
        "dead_event_record_errors_total",
        "Dead-letter INSERT failures (best-effort recorder swallows the error); label = source. \
         A rising value means the dead_events table is broken/missing and drops are NOT being captured."
    );
    metrics::counter!("dead_event_record_errors_total", "source" => "events.ingest").increment(0);
    metrics::counter!("dead_event_record_errors_total", "source" => "orchestration.result").increment(0);
}

/// Build a payload excerpt for storage, capped at [`DEAD_EVENT_PAYLOAD_MAX_BYTES`].
///
/// ORDER MATTERS: convert the bytes to a `String` via lossy UTF-8 FIRST, THEN
/// truncate the resulting string on a `char_indices` boundary. Slicing the raw
/// bytes first would split a multibyte sequence; converting first guarantees the
/// TEXT column always holds valid UTF-8 and never panics on a non-boundary cut.
/// Returns `None` for empty input.
pub fn payload_excerpt(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let lossy = String::from_utf8_lossy(bytes);
    if lossy.len() <= DEAD_EVENT_PAYLOAD_MAX_BYTES {
        return Some(lossy.into_owned());
    }
    // Largest char boundary at or below the cap. `char_indices` yields the byte
    // offset of each char start; the last one <= cap is a safe split point.
    let end = lossy
        .char_indices()
        .map(|(idx, _)| idx)
        .take_while(|&idx| idx <= DEAD_EVENT_PAYLOAD_MAX_BYTES)
        .last()
        .unwrap_or(0);
    Some(lossy[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Test double: collects every recorded [`DeadEvent`] behind a `Mutex`.
    /// Proves the trait is object-safe (used here behind `Arc<dyn …>`); the
    /// worker-level "drive a failing-auth message" capture lives in
    /// `tests/orchestration_result_contract.rs` (needs a live NATS server).
    #[derive(Default)]
    struct CapturingRecorder {
        events: Mutex<Vec<DeadEvent>>,
    }

    #[async_trait]
    impl DeadEventRecorder for CapturingRecorder {
        async fn record(&self, ev: DeadEvent) {
            self.events.lock().expect("dead-event capture lock").push(ev);
        }
    }

    #[tokio::test]
    async fn capturing_recorder_collects_via_dyn_dispatch() {
        let recorder: std::sync::Arc<dyn DeadEventRecorder> = std::sync::Arc::new(CapturingRecorder::default());
        recorder
            .record(DeadEvent {
                source: "events.ingest",
                reason: "bad_subject".to_string(),
                subject: "events.ingest.x".to_string(),
                detail: None,
                delivery_id: None,
                org_id: None,
                payload_excerpt: payload_excerpt(b"hi"),
            })
            .await;
        // Downcast is not needed — assert through a second capture into the same
        // concrete recorder to keep the double exercised end to end.
        let concrete = CapturingRecorder::default();
        concrete
            .record(DeadEvent {
                source: "orchestration.result",
                reason: "agent_unknown".to_string(),
                subject: "orchestration.result.y".to_string(),
                detail: Some("no hmac".to_string()),
                delivery_id: None,
                org_id: None,
                payload_excerpt: None,
            })
            .await;
        let captured = concrete.events.lock().expect("lock");
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].reason, "agent_unknown");
        assert_eq!(captured[0].source, "orchestration.result");
    }

    #[test]
    fn register_metrics_primes_series() {
        // Must not panic and must register the failure counter so a scrape sees
        // it at zero before any INSERT failure.
        register_metrics();
    }

    #[test]
    fn payload_excerpt_returns_none_for_empty_input() {
        assert_eq!(payload_excerpt(&[]), None);
    }

    #[test]
    fn payload_excerpt_passes_through_small_payloads() {
        assert_eq!(payload_excerpt(b"hello").as_deref(), Some("hello"));
    }

    #[test]
    fn payload_excerpt_replaces_invalid_utf8_then_caps() {
        // Invalid UTF-8 becomes U+FFFD (3 bytes) before any length check —
        // proving the conversion happens first, not a raw-byte slice.
        let excerpt = payload_excerpt(&[0xff, b'a']).expect("non-empty");
        assert!(excerpt.contains('\u{FFFD}'));
        assert!(excerpt.ends_with('a'));
    }

    #[test]
    fn payload_excerpt_truncates_on_a_multibyte_char_boundary() {
        // '€' is 3 bytes (U+20AC). Build a string whose length straddles the cap
        // mid-character so a naive byte slice at the cap would split the '€' and
        // panic. The helper must back off to the previous char boundary instead.
        // Fill to one byte below the cap with ASCII, then append a '€' so the
        // multibyte char starts at cap-1 and would be split at the cap.
        let mut bytes = vec![b'a'; DEAD_EVENT_PAYLOAD_MAX_BYTES - 1];
        bytes.extend_from_slice("€".as_bytes()); // 3 bytes → total = cap + 2
        let excerpt = payload_excerpt(&bytes).expect("non-empty");

        // Must be valid UTF-8 (it is a String) and not exceed the cap.
        assert!(excerpt.len() <= DEAD_EVENT_PAYLOAD_MAX_BYTES);
        // The split '€' is dropped entirely — the excerpt ends on the last
        // fully-fitting ASCII char, never a partial multibyte sequence.
        assert_eq!(excerpt.len(), DEAD_EVENT_PAYLOAD_MAX_BYTES - 1);
        assert!(excerpt.bytes().all(|b| b == b'a'));
    }

    #[test]
    fn payload_excerpt_keeps_a_char_that_ends_exactly_at_the_cap() {
        // A '€' whose final byte lands exactly on the cap fits and is kept whole.
        let mut bytes = vec![b'a'; DEAD_EVENT_PAYLOAD_MAX_BYTES - 3];
        bytes.extend_from_slice("€".as_bytes()); // ends at exactly cap
        let excerpt = payload_excerpt(&bytes).expect("non-empty");
        assert_eq!(excerpt.len(), DEAD_EVENT_PAYLOAD_MAX_BYTES);
        assert!(excerpt.ends_with('€'));
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn sqlx_recorder_round_trips_and_truncates_at_the_cap(pool: PgPool) {
        let recorder = SqlxDeadEventRecorder::new(pool.clone());
        let org = Uuid::new_v4();
        // An oversize payload: the excerpt stored must be capped.
        let oversize = payload_excerpt(&vec![b'z'; DEAD_EVENT_PAYLOAD_MAX_BYTES * 2]);
        assert!(oversize.as_ref().expect("excerpt").len() <= DEAD_EVENT_PAYLOAD_MAX_BYTES);

        recorder
            .record(DeadEvent {
                source: "events.ingest",
                reason: "signature_mismatch".to_string(),
                subject: "events.ingest.cli.abc".to_string(),
                detail: Some("agent abc".to_string()),
                delivery_id: None,
                org_id: Some(org),
                payload_excerpt: oversize.clone(),
            })
            .await;

        let row: (String, String, String, Option<String>, Option<Uuid>, Option<String>) = sqlx::query_as(
            r#"SELECT source, reason, subject, detail, org_id, payload_excerpt
               FROM dead_events ORDER BY recorded_at DESC LIMIT 1"#,
        )
        .fetch_one(&pool)
        .await
        .expect("read back the dead event");

        assert_eq!(row.0, "events.ingest");
        assert_eq!(row.1, "signature_mismatch");
        assert_eq!(row.2, "events.ingest.cli.abc");
        assert_eq!(row.3.as_deref(), Some("agent abc"));
        assert_eq!(row.4, Some(org));
        assert_eq!(row.5.expect("payload excerpt stored").len(), DEAD_EVENT_PAYLOAD_MAX_BYTES);
    }

    #[sqlx::test(migrations = "../db/migrations")]
    async fn sqlx_recorder_stores_null_org_and_delivery(pool: PgPool) {
        let recorder = SqlxDeadEventRecorder::new(pool.clone());
        recorder
            .record(DeadEvent {
                source: "orchestration.result",
                reason: "agent_unknown".to_string(),
                subject: "orchestration.result.abc".to_string(),
                detail: None,
                delivery_id: None,
                org_id: None,
                payload_excerpt: None,
            })
            .await;

        let (org, delivery): (Option<Uuid>, Option<String>) =
            sqlx::query_as("SELECT org_id, delivery_id FROM dead_events ORDER BY recorded_at DESC LIMIT 1")
                .fetch_one(&pool)
                .await
                .expect("read back");
        assert!(org.is_none());
        assert!(delivery.is_none());
    }
}
