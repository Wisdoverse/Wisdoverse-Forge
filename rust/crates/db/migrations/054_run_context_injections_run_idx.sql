-- no-transaction
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_run_context_injections_run_applied
    ON run_context_injections(run_id, position ASC, applied_at DESC, id DESC);
