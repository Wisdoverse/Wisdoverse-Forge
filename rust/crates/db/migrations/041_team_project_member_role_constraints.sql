-- Canonicalize team/project member role constraints.
--
-- Some legacy deployments carried CHECK constraints that only allowed the old
-- editor/viewer labels. The Rust permission model now uses
-- owner/admin/maintainer/member consistently across team and project members.

BEGIN;

ALTER TABLE public.team_members
    DROP CONSTRAINT IF EXISTS team_members_role_check;

ALTER TABLE public.project_members
    DROP CONSTRAINT IF EXISTS project_members_role_check;

UPDATE public.team_members
   SET role = CASE lower(trim(role))
       WHEN 'owner' THEN 'owner'
       WHEN 'admin' THEN 'admin'
       WHEN 'maintainer' THEN 'maintainer'
       WHEN 'member' THEN 'member'
       WHEN 'editor' THEN 'maintainer'
       WHEN 'viewer' THEN 'member'
       ELSE 'member'
   END;

UPDATE public.project_members
   SET role = CASE lower(trim(role))
       WHEN 'owner' THEN 'owner'
       WHEN 'admin' THEN 'admin'
       WHEN 'maintainer' THEN 'maintainer'
       WHEN 'member' THEN 'member'
       WHEN 'editor' THEN 'maintainer'
       WHEN 'viewer' THEN 'member'
       ELSE 'member'
   END;

ALTER TABLE public.team_members
    ADD CONSTRAINT team_members_role_check
    CHECK (role IN ('owner', 'admin', 'maintainer', 'member'));

ALTER TABLE public.project_members
    ADD CONSTRAINT project_members_role_check
    CHECK (role IN ('owner', 'admin', 'maintainer', 'member'));

COMMIT;
