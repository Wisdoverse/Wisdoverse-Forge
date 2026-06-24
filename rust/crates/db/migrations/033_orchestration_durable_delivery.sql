-- Migration 033: durable orchestration assignment/result delivery.
--
-- Adds task-level delivery metadata plus DB outbox/inbox tables so
-- assignment publish becomes durable-after-commit and result apply becomes
-- idempotent under replay.

ALTER TABLE orchestration_tasks
    ADD COLUMN IF NOT EXISTS attempt INT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS lease_expires_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS failure_code TEXT,
    ADD COLUMN IF NOT EXISTS retryable BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS last_assignment_id UUID;

CREATE INDEX IF NOT EXISTS idx_orch_tasks_working_lease
    ON orchestration_tasks(organization_id, lease_expires_at)
    WHERE status = 'working';

CREATE TABLE IF NOT EXISTS orchestration_outbox (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_orchestration_outbox_unpublished
    ON orchestration_outbox(created_at)
    WHERE published_at IS NULL;

CREATE TABLE IF NOT EXISTS orchestration_inbox (
    delivery_id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    task_id UUID NOT NULL REFERENCES orchestration_tasks(id) ON DELETE CASCADE,
    message_type TEXT NOT NULL,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_orchestration_inbox_task
    ON orchestration_inbox(organization_id, task_id);
