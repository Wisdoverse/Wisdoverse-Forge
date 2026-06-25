-- 076: per-consumer transactional-outbox indexes (F050).
--
-- The two outbox consumers (assignment + clone) each poll for their OWN oldest
-- unpublished row with `FOR UPDATE SKIP LOCKED LIMIT 1`, filtering by the
-- discriminator (`event_type` / `aggregate_type`). The existing
-- `idx_orchestration_outbox_unpublished(created_at) WHERE published_at IS NULL`
-- does not cover the discriminator, so a sustained one-sided backlog (e.g. NATS
-- down for one stream) forces the healthy consumer to scan-and-skip the other
-- stream's head-of-line backlog every tick — an index-seek that degrades into a
-- growing scan.
--
-- Two partial composite indexes give each consumer an index-only seek to its own
-- oldest unpublished row regardless of the other stream's backlog. Additive and
-- idempotent.
CREATE INDEX IF NOT EXISTS idx_orchestration_outbox_event_type_unpublished
    ON orchestration_outbox (event_type, created_at)
    WHERE published_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_orchestration_outbox_aggregate_type_unpublished
    ON orchestration_outbox (aggregate_type, created_at)
    WHERE published_at IS NULL;
