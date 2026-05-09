-- no-transaction
-- CREATE INDEX CONCURRENTLY cannot run inside a transaction. Split per-index
-- across migrations 020-023 — each file is a single statement so Postgres'
-- implicit-tx-around-multi-statement-Query doesn't kick in.
CREATE INDEX CONCURRENTLY IF NOT EXISTS projects_team_id_idx
    ON public.projects(team_id)
    WHERE team_id IS NOT NULL;
