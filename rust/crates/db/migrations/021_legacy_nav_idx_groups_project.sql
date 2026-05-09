-- no-transaction
CREATE INDEX CONCURRENTLY IF NOT EXISTS groups_project_id_idx
    ON public.groups(project_id)
    WHERE project_id IS NOT NULL;
