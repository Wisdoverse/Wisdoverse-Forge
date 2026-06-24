-- no-transaction
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_run_context_injections_item_kind_applied
    ON run_context_injections(item_id, item_kind, applied_at DESC, id DESC);
