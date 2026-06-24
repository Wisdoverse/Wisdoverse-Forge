-- Migration 010: extend orchestration_tasks for kanban + auto-pickup + blocked-distance hints.
--
-- - Adds kanban-friendly state model: backlog | queued | working | blocked | completed | failed | canceled
--   (replaces legacy pending/running with queued/working).
-- - Adds priority, progress, params (a2a request payload), error envelope, group scoping.
-- - Adds blocked_reason + blocked_metadata so a blocked task can answer
--   "what is missing to unblock?" without callers having to compute it.
-- - Records timestamps for the cancel transition so retry/cancel UX has data to show.

ALTER TABLE orchestration_tasks
    ADD COLUMN IF NOT EXISTS group_id UUID REFERENCES groups(id),
    ADD COLUMN IF NOT EXISTS priority TEXT NOT NULL DEFAULT 'normal',
    ADD COLUMN IF NOT EXISTS progress SMALLINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS params JSONB,
    ADD COLUMN IF NOT EXISTS error JSONB,
    ADD COLUMN IF NOT EXISTS canceled_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS blocked_reason TEXT,
    ADD COLUMN IF NOT EXISTS blocked_metadata JSONB;

-- Migrate legacy statuses to kanban statuses. Idempotent; safe on empty tables.
UPDATE orchestration_tasks SET status = 'queued'  WHERE status = 'pending';
UPDATE orchestration_tasks SET status = 'working' WHERE status = 'running';

-- Indexes for kanban grouping + auto-dispatch sweeps.
CREATE INDEX IF NOT EXISTS idx_orch_tasks_group ON orchestration_tasks(group_id);
CREATE INDEX IF NOT EXISTS idx_orch_tasks_assignee ON orchestration_tasks(assigned_agent_id);
CREATE INDEX IF NOT EXISTS idx_orch_tasks_priority ON orchestration_tasks(priority);
CREATE INDEX IF NOT EXISTS idx_orch_tasks_org_status ON orchestration_tasks(organization_id, status);
