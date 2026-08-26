-- Team-space reusable task templates.
--
-- A template is a saved starter brief (name + title + description + priority +
-- approval flag) that any org member can apply when writing a task and manage
-- from Settings -> Task templates. Templates are organization-scoped so the
-- task form can offer them before a project is selected; deleting is limited
-- to the creator or an owner/admin.

CREATE TABLE IF NOT EXISTS task_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    priority TEXT NOT NULL DEFAULT 'normal',
    requires_approval BOOLEAN NOT NULL DEFAULT FALSE,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_task_templates_org_created
    ON task_templates (organization_id, created_at DESC);
