-- no-transaction
CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS teams_org_slug_idx
    ON public.teams(organization_id, slug)
    WHERE slug IS NOT NULL;
