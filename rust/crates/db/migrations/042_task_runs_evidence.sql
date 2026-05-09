-- Unit 1.3: governed execution runs and run-scoped evidence projection.
--
-- `task_runs` records one execution attempt per orchestration assignment.
-- Existing evidence rows stay valid with NULL `run_id`; new rows can attach to
-- a run without rewriting legacy event/message/attachment history.

CREATE TABLE IF NOT EXISTS task_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    orchestration_task_id UUID NOT NULL REFERENCES orchestration_tasks(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'working', 'completed', 'failed', 'canceled')),
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    capability_profile JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (orchestration_task_id, idempotency_key)
);

ALTER TABLE events
    ADD COLUMN IF NOT EXISTS run_id UUID;

ALTER TABLE agent_messages
    ADD COLUMN IF NOT EXISTS run_id UUID;

ALTER TABLE attachments
    ADD COLUMN IF NOT EXISTS run_id UUID;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'events_run_id_fkey'
          AND conrelid = 'events'::regclass
    ) THEN
        ALTER TABLE events
            ADD CONSTRAINT events_run_id_fkey
            FOREIGN KEY (run_id) REFERENCES task_runs(id) ON DELETE SET NULL NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'agent_messages_run_id_fkey'
          AND conrelid = 'agent_messages'::regclass
    ) THEN
        ALTER TABLE agent_messages
            ADD CONSTRAINT agent_messages_run_id_fkey
            FOREIGN KEY (run_id) REFERENCES task_runs(id) ON DELETE SET NULL NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'attachments_run_id_fkey'
          AND conrelid = 'attachments'::regclass
    ) THEN
        ALTER TABLE attachments
            ADD CONSTRAINT attachments_run_id_fkey
            FOREIGN KEY (run_id) REFERENCES task_runs(id) ON DELETE SET NULL NOT VALID;
    END IF;
END $$;

ALTER TABLE events VALIDATE CONSTRAINT events_run_id_fkey;
ALTER TABLE agent_messages VALIDATE CONSTRAINT agent_messages_run_id_fkey;
ALTER TABLE attachments VALIDATE CONSTRAINT attachments_run_id_fkey;

CREATE INDEX IF NOT EXISTS idx_task_runs_org_workspace_task_started
    ON task_runs(organization_id, workspace_id, orchestration_task_id, started_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_task_runs_org_status_started
    ON task_runs(organization_id, status, started_at DESC, id DESC);

CREATE OR REPLACE VIEW v_run_evidence AS
SELECT
    e.run_id,
    e.organization_id,
    COALESCE(tr.workspace_id, a.workspace_id) AS workspace_id,
    e.agent_id,
    'event'::text AS source_type,
    e.id AS source_id,
    jsonb_build_object(
        'event_type', e.event_type,
        'payload', e.payload,
        'session_id', e.session_id
    ) AS payload,
    e.created_at
FROM events e
JOIN agents a
  ON a.id = e.agent_id
 AND a.organization_id = e.organization_id
LEFT JOIN task_runs tr
  ON tr.id = e.run_id
 AND tr.organization_id = e.organization_id
WHERE e.run_id IS NULL OR tr.id IS NOT NULL

UNION ALL

SELECT
    m.run_id,
    m.organization_id,
    COALESCE(tr.workspace_id, a.workspace_id) AS workspace_id,
    m.agent_id,
    'agent_message'::text AS source_type,
    m.id AS source_id,
    jsonb_build_object(
        'role', m.role,
        'content', m.content,
        'tokens_in', m.tokens_in,
        'tokens_out', m.tokens_out,
        'model', m.model,
        'finish_reason', m.finish_reason
    ) AS payload,
    m.created_at
FROM agent_messages m
JOIN agents a
  ON a.id = m.agent_id
 AND a.organization_id = m.organization_id
LEFT JOIN task_runs tr
  ON tr.id = m.run_id
 AND tr.organization_id = m.organization_id
WHERE m.run_id IS NULL OR tr.id IS NOT NULL

UNION ALL

SELECT
    att.run_id,
    att.organization_id,
    COALESCE(tr.workspace_id, a.workspace_id) AS workspace_id,
    att.agent_id,
    'attachment'::text AS source_type,
    att.id AS source_id,
    jsonb_build_object(
        'filename', att.filename,
        'content_type', att.content_type,
        'size_bytes', att.size_bytes,
        'storage_path', att.storage_path,
        'storage_backend', att.storage_backend
    ) AS payload,
    att.created_at
FROM attachments att
LEFT JOIN agents a
  ON a.id = att.agent_id
 AND a.organization_id = att.organization_id
LEFT JOIN task_runs tr
  ON tr.id = att.run_id
 AND tr.organization_id = att.organization_id
WHERE att.run_id IS NULL OR tr.id IS NOT NULL

UNION ALL

SELECT
    tr.id AS run_id,
    tr.organization_id,
    tr.workspace_id,
    tr.agent_id,
    'task_result'::text AS source_type,
    task.id AS source_id,
    jsonb_build_object(
        'status', task.status,
        'result', task.result,
        'error', task.error,
        'completed_at', task.completed_at,
        'canceled_at', task.canceled_at
    ) AS payload,
    COALESCE(task.completed_at, task.canceled_at, tr.finished_at, tr.started_at) AS created_at
FROM task_runs tr
JOIN orchestration_tasks task
  ON task.id = tr.orchestration_task_id
 AND task.organization_id = tr.organization_id
WHERE task.status IN ('completed', 'failed', 'canceled');
