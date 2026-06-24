-- Migration 035: production triggers for orchestration blocked reasons.
--
-- Approval blocks need durable state so approving a task is idempotent and
-- auditable. The existing blocked_reason/blocked_metadata columns carry the
-- UI-facing reason; these fields record the approval contract itself.

ALTER TABLE orchestration_tasks
    ADD COLUMN IF NOT EXISTS requires_approval BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS approved_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS approved_by UUID REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_orch_tasks_waiting_approval
    ON orchestration_tasks(organization_id, status, blocked_reason)
    WHERE status = 'blocked' AND blocked_reason = 'waiting_approval';
