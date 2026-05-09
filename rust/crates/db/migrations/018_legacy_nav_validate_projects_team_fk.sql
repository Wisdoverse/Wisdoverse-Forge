-- no-transaction
-- Validate the projects.team_id FK added NOT VALID in 016. Single statement so
-- Postgres' implicit-tx-around-multi-statement-Query doesn't apply. Combined
-- with sqlx's `-- no-transaction` directive, this runs without any wrapping
-- transaction (required by VALIDATE CONSTRAINT for online execution).
ALTER TABLE public.projects VALIDATE CONSTRAINT projects_team_id_fkey;
