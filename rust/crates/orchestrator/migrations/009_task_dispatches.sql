-- Durable dispatch tracking for the assign endpoint (#809).
-- Records the lifecycle of each spawn (queued -> starting -> started / failed).

CREATE TABLE IF NOT EXISTS task_dispatches (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id    TEXT NOT NULL,
    org_id     TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'starting', 'started', 'failed')),
    attempt    INTEGER NOT NULL DEFAULT 1,
    last_error TEXT,
    session_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_task_dispatches_org_task ON task_dispatches(org_id, task_id);
