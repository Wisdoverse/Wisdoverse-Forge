-- no-transaction
-- Same as 018 for the groups.project_id FK.
ALTER TABLE public.groups VALIDATE CONSTRAINT groups_project_id_fkey;
