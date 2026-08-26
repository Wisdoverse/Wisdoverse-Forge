-- Project-scoped task templates: a template may carry an optional project,
-- so project-specific briefs (release checklist, deploy runbook) appear only
-- in that project's task form. NULL = team-wide.

ALTER TABLE task_templates
    ADD COLUMN IF NOT EXISTS project_id UUID REFERENCES projects(id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_task_templates_project
    ON task_templates (project_id) WHERE project_id IS NOT NULL;
