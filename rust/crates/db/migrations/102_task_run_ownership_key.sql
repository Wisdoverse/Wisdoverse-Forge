-- no-transaction
-- A completed interrupted build may leave an invalid index: inspect and drop
-- that invalid index before retrying. IF NOT EXISTS cannot repair it.
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS idx_task_runs_org_task_id
    ON task_runs (organization_id, orchestration_task_id, id);
