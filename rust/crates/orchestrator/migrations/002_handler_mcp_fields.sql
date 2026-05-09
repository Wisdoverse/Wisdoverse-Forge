-- no-transaction
-- Add fields for MCP integration

BEGIN;

ALTER TABLE tasks ADD COLUMN agentforge_session_id TEXT;

ALTER TABLE code_reviews ADD COLUMN diff_snapshot JSONB;

ALTER TABLE tasks DROP CONSTRAINT tasks_state_check;
ALTER TABLE tasks ADD CONSTRAINT tasks_state_check
  CHECK (state IN ('pending', 'assigned', 'working', 'review', 'completed', 'failed', 'changes_requested'));

COMMIT;
