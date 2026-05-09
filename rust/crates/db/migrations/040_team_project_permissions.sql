-- Team and project management permissions.
--
-- Organization owner/admin remains the broad management role. These tables add
-- narrower team/project roles for scoped management without widening org-level
-- access. The migration is idempotent so it tolerates environments that already
-- had legacy-shaped member tables after historical imports.

BEGIN;

CREATE TABLE IF NOT EXISTS public.team_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    team_id UUID NOT NULL REFERENCES public.teams(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'member',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE public.team_members
    ADD COLUMN IF NOT EXISTS id UUID DEFAULT gen_random_uuid(),
    ADD COLUMN IF NOT EXISTS team_id UUID,
    ADD COLUMN IF NOT EXISTS user_id UUID,
    ADD COLUMN IF NOT EXISTS role TEXT NOT NULL DEFAULT 'member',
    ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT now();

UPDATE public.team_members SET id = gen_random_uuid() WHERE id IS NULL;
ALTER TABLE public.team_members ALTER COLUMN id SET DEFAULT gen_random_uuid();
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'team_members'
           AND column_name = 'joined_at'
    ) THEN
        EXECUTE 'UPDATE public.team_members SET created_at = COALESCE(created_at, joined_at, now()) WHERE created_at IS NULL';
    END IF;
END;
$$;
UPDATE public.team_members SET created_at = now() WHERE created_at IS NULL;

ALTER TABLE public.team_members
    ALTER COLUMN id SET NOT NULL,
    ALTER COLUMN team_id SET NOT NULL,
    ALTER COLUMN user_id SET NOT NULL,
    ALTER COLUMN role SET NOT NULL,
    ALTER COLUMN created_at SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
         WHERE table_schema = 'public'
           AND table_name = 'team_members'
           AND constraint_type = 'PRIMARY KEY'
    ) THEN
        ALTER TABLE public.team_members ADD PRIMARY KEY (id);
    END IF;
END;
$$;

CREATE UNIQUE INDEX IF NOT EXISTS team_members_team_user_idx
    ON public.team_members(team_id, user_id);

CREATE INDEX IF NOT EXISTS team_members_user_idx
    ON public.team_members(user_id);

CREATE TABLE IF NOT EXISTS public.project_members (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES public.projects(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'member',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE public.project_members
    ADD COLUMN IF NOT EXISTS id UUID DEFAULT gen_random_uuid(),
    ADD COLUMN IF NOT EXISTS project_id UUID,
    ADD COLUMN IF NOT EXISTS user_id UUID,
    ADD COLUMN IF NOT EXISTS role TEXT NOT NULL DEFAULT 'member',
    ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT now();

UPDATE public.project_members SET id = gen_random_uuid() WHERE id IS NULL;
ALTER TABLE public.project_members ALTER COLUMN id SET DEFAULT gen_random_uuid();
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'project_members'
           AND column_name = 'joined_at'
    ) THEN
        EXECUTE 'UPDATE public.project_members SET created_at = COALESCE(created_at, joined_at, now()) WHERE created_at IS NULL';
    END IF;
END;
$$;
UPDATE public.project_members SET created_at = now() WHERE created_at IS NULL;

ALTER TABLE public.project_members
    ALTER COLUMN id SET NOT NULL,
    ALTER COLUMN project_id SET NOT NULL,
    ALTER COLUMN user_id SET NOT NULL,
    ALTER COLUMN role SET NOT NULL,
    ALTER COLUMN created_at SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
         WHERE table_schema = 'public'
           AND table_name = 'project_members'
           AND constraint_type = 'PRIMARY KEY'
    ) THEN
        ALTER TABLE public.project_members ADD PRIMARY KEY (id);
    END IF;
END;
$$;

CREATE UNIQUE INDEX IF NOT EXISTS project_members_project_user_idx
    ON public.project_members(project_id, user_id);

CREATE INDEX IF NOT EXISTS project_members_user_idx
    ON public.project_members(user_id);

COMMIT;
