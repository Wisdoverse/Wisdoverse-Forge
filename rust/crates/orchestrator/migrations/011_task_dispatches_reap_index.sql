-- Partial index to speed up the dispatch reaper's sweep query.
-- The reaper scans all rows WHERE status IN ('queued', 'starting') AND updated_at < NOW() - TTL.
-- Without this index the query performs a full table scan on every tick.
CREATE INDEX IF NOT EXISTS idx_task_dispatches_stuck ON task_dispatches (updated_at) WHERE status IN ('queued', 'starting');
