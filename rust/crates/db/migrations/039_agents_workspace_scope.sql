-- Agents need an explicit workspace execution boundary.
--
-- `agents.project_id` is the primary/default project for UI context and task
-- routing. It is not the filesystem boundary: Container CLI agents mount the
-- selected workspace's projects root at /workspace and may work across projects
-- inside that workspace.

ALTER TABLE agents
    ADD COLUMN IF NOT EXISTS workspace_id UUID;

-- Ensure every organization with agents has at least one workspace to attach.
INSERT INTO workspaces (organization_id, name)
SELECT DISTINCT a.organization_id, 'Default Workspace'
FROM agents a
WHERE NOT EXISTS (
    SELECT 1
    FROM workspaces w
    WHERE w.organization_id = a.organization_id
      AND w.deleted_at IS NULL
);

-- Project-scoped agents inherit the project's workspace.
UPDATE agents a
SET workspace_id = p.workspace_id
FROM projects p
WHERE a.project_id = p.id
  AND a.workspace_id IS NULL;

-- Workspace-level/provider agents fall back to the org's oldest live workspace.
UPDATE agents a
SET workspace_id = (
    SELECT id
    FROM workspaces
    WHERE organization_id = a.organization_id
      AND deleted_at IS NULL
    ORDER BY created_at ASC
    LIMIT 1
)
WHERE a.workspace_id IS NULL;

ALTER TABLE agents
    ALTER COLUMN workspace_id SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'agents_workspace_id_fkey'
          AND conrelid = 'agents'::regclass
    ) THEN
        ALTER TABLE agents
            ADD CONSTRAINT agents_workspace_id_fkey
            FOREIGN KEY (workspace_id) REFERENCES workspaces(id) NOT VALID;
    END IF;
END $$;

ALTER TABLE agents VALIDATE CONSTRAINT agents_workspace_id_fkey;

CREATE INDEX IF NOT EXISTS idx_agents_workspace
    ON agents(workspace_id);

COMMENT ON COLUMN agents.workspace_id IS
    'Workspace execution/access boundary for the agent. project_id remains the primary project context.';
