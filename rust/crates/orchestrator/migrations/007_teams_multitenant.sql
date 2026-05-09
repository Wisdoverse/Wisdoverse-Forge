-- no-transaction
-- Multi-team and RBAC infrastructure

BEGIN;

CREATE SCHEMA IF NOT EXISTS legacy_orchestrator;

DO $$
DECLARE
    teams_id_type TEXT;
BEGIN
    SELECT c.data_type
    INTO teams_id_type
    FROM information_schema.columns c
    WHERE c.table_schema = 'public'
      AND c.table_name = 'teams'
      AND c.column_name = 'id';

    IF teams_id_type IS NOT NULL
       AND teams_id_type <> 'uuid'
       AND to_regclass('legacy_orchestrator.teams') IS NULL THEN
        IF to_regclass('public.team_members') IS NOT NULL THEN
            ALTER TABLE public.team_members SET SCHEMA legacy_orchestrator;
        END IF;
        ALTER TABLE public.teams SET SCHEMA legacy_orchestrator;
    END IF;
END;
$$;

-- Teams
CREATE TABLE IF NOT EXISTS teams (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL,
    org_id     TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (org_id, name)
);

CREATE INDEX IF NOT EXISTS idx_teams_org ON teams (org_id);

-- Team membership
CREATE TABLE IF NOT EXISTS team_members (
    team_id        UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    participant_id UUID NOT NULL REFERENCES participants(id) ON DELETE CASCADE,
    role           TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('owner', 'admin', 'member')),
    joined_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (team_id, participant_id)
);

CREATE INDEX IF NOT EXISTS idx_team_members_participant ON team_members (participant_id);

-- RBAC bindings
CREATE TABLE IF NOT EXISTS rbac_bindings (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    TEXT NOT NULL,
    role       TEXT NOT NULL CHECK (role IN ('superadmin', 'owner', 'admin', 'member')),
    scope_type TEXT NOT NULL CHECK (scope_type IN ('system', 'org', 'team')),
    scope_id   TEXT,
    granted_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, scope_type, scope_id)
);

CREATE INDEX IF NOT EXISTS idx_rbac_user ON rbac_bindings (user_id);
CREATE INDEX IF NOT EXISTS idx_rbac_scope ON rbac_bindings (scope_type, scope_id);

-- Add team_id to resource tables
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS team_id UUID;
ALTER TABLE workflows ADD COLUMN IF NOT EXISTS team_id UUID;
ALTER TABLE code_reviews ADD COLUMN IF NOT EXISTS team_id UUID;
ALTER TABLE knowledge_entries ADD COLUMN IF NOT EXISTS team_id UUID;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.tasks'::regclass
          AND conname = 'tasks_team_id_fkey'
    ) THEN
        ALTER TABLE tasks ADD CONSTRAINT tasks_team_id_fkey FOREIGN KEY (team_id) REFERENCES teams(id);
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.workflows'::regclass
          AND conname = 'workflows_team_id_fkey'
    ) THEN
        ALTER TABLE workflows ADD CONSTRAINT workflows_team_id_fkey FOREIGN KEY (team_id) REFERENCES teams(id);
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.code_reviews'::regclass
          AND conname = 'code_reviews_team_id_fkey'
    ) THEN
        ALTER TABLE code_reviews ADD CONSTRAINT code_reviews_team_id_fkey FOREIGN KEY (team_id) REFERENCES teams(id);
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.knowledge_entries'::regclass
          AND conname = 'knowledge_entries_team_id_fkey'
    ) THEN
        ALTER TABLE knowledge_entries
            ADD CONSTRAINT knowledge_entries_team_id_fkey FOREIGN KEY (team_id) REFERENCES teams(id);
    END IF;
END;
$$;

CREATE INDEX IF NOT EXISTS idx_tasks_team ON tasks (team_id) WHERE team_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_workflows_team ON workflows (team_id) WHERE team_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_reviews_team ON code_reviews (team_id) WHERE team_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_knowledge_team ON knowledge_entries (team_id) WHERE team_id IS NOT NULL;

-- Updated_at trigger for teams
DROP TRIGGER IF EXISTS teams_updated_at ON teams;
CREATE TRIGGER teams_updated_at BEFORE UPDATE ON teams FOR EACH ROW EXECUTE FUNCTION update_updated_at();

COMMIT;
