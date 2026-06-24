-- no-transaction
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS projects_team_slug_idx
    ON public.projects(team_id, slug)
    WHERE team_id IS NOT NULL AND slug IS NOT NULL;
