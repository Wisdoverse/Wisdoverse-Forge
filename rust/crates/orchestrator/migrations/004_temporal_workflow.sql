-- no-transaction
-- Add Temporal integration columns to workflows and workflow_nodes.

BEGIN;

-- Track Temporal execution IDs on workflows.
ALTER TABLE workflows
    ADD COLUMN IF NOT EXISTS temporal_workflow_id TEXT,
    ADD COLUMN IF NOT EXISTS temporal_run_id TEXT;

-- Update status CHECK to include cancelled and paused.
ALTER TABLE workflows DROP CONSTRAINT IF EXISTS workflows_status_check;
ALTER TABLE workflows ADD CONSTRAINT workflows_status_check
    CHECK (status IN ('draft', 'running', 'completed', 'failed', 'cancelled', 'paused'));

-- Track per-node execution state.
ALTER TABLE workflow_nodes
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'completed', 'failed', 'skipped')),
    ADD COLUMN IF NOT EXISTS started_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS completed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS error TEXT,
    ADD COLUMN IF NOT EXISTS output JSONB;

COMMIT;
