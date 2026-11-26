-- Workflow execution reconciliation (#1352). All columns are nullable; no backfill.
--   temporal_namespace_id   server-assigned id of the Temporal namespace the execution was started in;
--                           proves a later "not found" came from the same Temporal. NULL for rows
--                           started before this migration (never reconciled without operator ack).
--   status_cause            why reconciliation set the current status: execution_lost,
--                           execution_missing or reconciled_from_temporal. Cleared by a normal change.
--   execution_missing_since when Temporal was first seen answering "not found" for the execution.
--   execution_checked_at    when the reconciler last got an answer from Temporal for this row.
--   reconciled_at           when reconciliation last set the status.

ALTER TABLE workflows
    ADD COLUMN IF NOT EXISTS temporal_namespace_id TEXT,
    ADD COLUMN IF NOT EXISTS status_cause TEXT,
    ADD COLUMN IF NOT EXISTS execution_missing_since TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS execution_checked_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS reconciled_at TIMESTAMPTZ;

-- The reconciler scans running rows least-recently-checked first.
CREATE INDEX IF NOT EXISTS idx_workflows_running_reconcile
    ON workflows (execution_checked_at NULLS FIRST, updated_at, id)
    WHERE status = 'running';

-- The reconciler's bookkeeping writes (execution_checked_at, execution_missing_since) must not
-- move updated_at: it is the compare-and-set token and the time of the last real change.
-- Every write that changes execution_checked_at and is a real change sets updated_at itself.
DROP TRIGGER IF EXISTS workflows_updated_at ON workflows;
CREATE TRIGGER workflows_updated_at BEFORE UPDATE ON workflows FOR EACH ROW
    WHEN (OLD.execution_checked_at IS NOT DISTINCT FROM NEW.execution_checked_at)
    EXECUTE FUNCTION update_updated_at();
