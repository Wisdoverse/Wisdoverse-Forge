-- Recurring (scheduled) tasks: a saved brief plus a project + waiting place
-- that the server re-creates on its cadence. Unassigned by design — the
-- next available agent starts each run (approval flag supported).

CREATE TABLE IF NOT EXISTS recurring_tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    priority TEXT NOT NULL DEFAULT 'normal',
    requires_approval BOOLEAN NOT NULL DEFAULT FALSE,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    group_id UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    cadence_minutes INTEGER NOT NULL,
    next_run_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_recurring_tasks_due
    ON recurring_tasks (next_run_at) WHERE enabled;
