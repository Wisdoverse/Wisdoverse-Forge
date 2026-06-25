-- no-transaction
-- F050 (2/2): per-consumer outbox index for the clone publisher, which polls
-- `... WHERE published_at IS NULL AND aggregate_type = $1`. Built CONCURRENTLY
-- (single statement, `-- no-transaction`) for the same write-availability reason
-- as 076. Idempotent.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_orchestration_outbox_aggregate_type_unpublished
    ON orchestration_outbox (aggregate_type, created_at)
    WHERE published_at IS NULL;
