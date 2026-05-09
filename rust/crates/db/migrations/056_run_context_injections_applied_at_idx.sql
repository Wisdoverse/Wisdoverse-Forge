-- no-transaction
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_run_context_injections_applied_at
    ON run_context_injections(applied_at DESC, id DESC);
