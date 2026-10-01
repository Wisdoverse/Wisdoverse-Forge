-- no-transaction
-- Composite key for the source-to-task tenant boundary. Build without blocking
-- task writes; the following migration adds the referencing table.
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS idx_orchestration_tasks_org_id
    ON orchestration_tasks (organization_id, id);
