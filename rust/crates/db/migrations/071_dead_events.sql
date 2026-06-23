-- 071: dead-letter capture for permanently-dropped NATS events (#811 follow-up).
-- Additive + idempotent. A consumer that TERM-drops an inbound envelope (bad
-- signature, unknown agent, bad subject, stale timestamp, malformed body) records
-- one row here so an operator debugging "why aren't agent X's events showing up?"
-- has a durable record. org_id / delivery_id are NULLABLE: most drops are pre-auth
-- and carry no trustworthy org — the `subject` carries the agent UUID, which is the
-- real debugging key. payload_excerpt is a truncated (<= 8 KiB) excerpt of the raw,
-- UNTRUSTED message; render it escaped (it is stored-XSS-capable and may contain
-- task output). Exposed only to platform owners via GET /admin/dead-events.
CREATE TABLE IF NOT EXISTS dead_events (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source          TEXT NOT NULL,          -- 'events.ingest' | 'orchestration.result'
    reason          TEXT NOT NULL,          -- e.g. 'signature_mismatch'
    subject         TEXT NOT NULL,          -- NATS subject (carries agent UUID)
    detail          TEXT,                   -- human context already built at the reject site
    delivery_id     TEXT,                   -- nullable (absent on early/event drops)
    org_id          UUID,                   -- nullable: dropped events are pre-auth, no trustworthy org
    payload_excerpt TEXT,                   -- truncated raw payload (<= 8 KiB), UNTRUSTED
    recorded_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Default list (newest first, unfiltered).
CREATE INDEX IF NOT EXISTS idx_dead_events_recorded_at ON dead_events (recorded_at DESC);
-- Filtered-and-sorted query `WHERE reason = $1 ORDER BY recorded_at DESC`. The
-- composite supersedes a standalone reason index, so there is no standalone one.
CREATE INDEX IF NOT EXISTS idx_dead_events_reason_recorded ON dead_events (reason, recorded_at DESC);
