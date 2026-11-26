-- Workflow execution reconciliation (#1352). New columns are nullable or defaulted; no backfill.
--   temporal_namespace_id   server-assigned id of the Temporal namespace the execution was started in;
--                           proves a later "not found" came from the same Temporal. NULL for rows
--                           started before this migration (never reconciled without operator ack).
--   status_cause            why reconciliation set the current status: execution_lost,
--                           execution_missing or reconciled_from_temporal. Cleared by a normal change.
--   execution_missing_since when Temporal was first seen answering "not found" for the execution.
--   execution_checked_at    when the reconciler last got an answer from Temporal for this row.
--   execution_observation   that answer: running, closed, missing or unverifiable.
--   reconciled_at           when reconciliation last set the status.
--   state_version           compare-and-set token: +1 on every change except reconciler bookkeeping.

ALTER TABLE workflows
    ADD COLUMN IF NOT EXISTS temporal_namespace_id TEXT,
    ADD COLUMN IF NOT EXISTS status_cause TEXT,
    ADD COLUMN IF NOT EXISTS execution_missing_since TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS execution_checked_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS execution_observation TEXT,
    ADD COLUMN IF NOT EXISTS reconciled_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS state_version BIGINT NOT NULL DEFAULT 0;

-- The reconciler scans running rows least-recently-checked first.
CREATE INDEX IF NOT EXISTS idx_workflows_running_reconcile
    ON workflows (execution_checked_at NULLS FIRST, updated_at, id)
    WHERE status = 'running';

-- Replaces migration 001's updated_at trigger for workflows, for every writer:
--   * an update that changes ONLY reconciler bookkeeping (execution_checked_at,
--     execution_missing_since, execution_observation) changes neither updated_at nor state_version;
--   * every other update sets updated_at = now() as before AND increments state_version, so two
--     changes in one transaction (same now()) still get two different versions.
CREATE OR REPLACE FUNCTION workflows_track_change()
RETURNS TRIGGER AS $$
DECLARE
    bookkeeping CONSTANT text[] := ARRAY['execution_checked_at', 'execution_missing_since', 'execution_observation'];
BEGIN
    IF to_jsonb(NEW) <> to_jsonb(OLD) AND to_jsonb(NEW) - bookkeeping = to_jsonb(OLD) - bookkeeping THEN
        RETURN NEW;
    END IF;
    NEW.updated_at = now();
    NEW.state_version = OLD.state_version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS workflows_updated_at ON workflows;
CREATE TRIGGER workflows_updated_at BEFORE UPDATE ON workflows FOR EACH ROW EXECUTE FUNCTION workflows_track_change();
