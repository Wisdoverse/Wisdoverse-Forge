-- no-transaction
-- F050 (1/2): per-consumer outbox index for the assignment publisher, which
-- polls `... WHERE published_at IS NULL AND event_type = 'assignment'`. Built
-- CONCURRENTLY so it never write-blocks the publishers on the exact backlogged
-- table this index is meant to speed up. CONCURRENTLY cannot run in a
-- transaction, so `-- no-transaction` is set and this file is a single
-- statement (split from the aggregate_type index in 077). Idempotent.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_orchestration_outbox_event_type_unpublished
    ON orchestration_outbox (event_type, created_at)
    WHERE published_at IS NULL;
