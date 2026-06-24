-- 016_legacy_nav_canonical_columns.sql
--
-- Promote legacy.* nav fields into canonical public.* per ADR 0001.
-- Additive only: every column nullable, every FK NOT VALID.
--
-- This is the first of three migrations for Issue #15 P2:
--   016 (this file)  — additive columns + reconcile log table
--   017              — idempotent batched backfill of those columns
--   018              — VALIDATE CONSTRAINT + CONCURRENTLY indexes
--
-- Idempotency: every ALTER uses ADD COLUMN IF NOT EXISTS; FK adds are wrapped
-- in DO $$ ... IF NOT EXISTS ... $$ blocks; CREATE TABLE / INDEX use
-- IF NOT EXISTS. Re-applying this migration is a no-op.
--
-- Forward-only: sqlx records a checksum on first apply. Any correction must
-- land as a new migration (017+); editing this file after it has applied on
-- staging/prod will cause sqlx checksum mismatch.
--
-- Operational note: ADD COLUMN of a nullable column with no default is
-- metadata-only on Postgres 11+ (no table rewrite, brief ACCESS EXCLUSIVE
-- lock). ADD CONSTRAINT ... NOT VALID similarly only takes a brief metadata
-- lock. Constraint validation is deferred to migration 018 (post-backfill).

BEGIN;

-- teams: slug / visibility / description
ALTER TABLE public.teams
    ADD COLUMN IF NOT EXISTS slug TEXT,
    ADD COLUMN IF NOT EXISTS visibility TEXT,
    ADD COLUMN IF NOT EXISTS description TEXT;

-- projects: team_id (FK to teams) + slug / color / description
ALTER TABLE public.projects
    ADD COLUMN IF NOT EXISTS team_id UUID,
    ADD COLUMN IF NOT EXISTS slug TEXT,
    ADD COLUMN IF NOT EXISTS color TEXT,
    ADD COLUMN IF NOT EXISTS description TEXT;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'projects'
          AND constraint_name = 'projects_team_id_fkey'
    ) THEN
        ALTER TABLE public.projects
            ADD CONSTRAINT projects_team_id_fkey
            FOREIGN KEY (team_id) REFERENCES public.teams(id)
            ON DELETE RESTRICT
            NOT VALID;
    END IF;
END;
$$;

-- groups: project_id (FK to projects, nullable permanently per ADR 0001)
ALTER TABLE public.groups
    ADD COLUMN IF NOT EXISTS project_id UUID;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'groups'
          AND constraint_name = 'groups_project_id_fkey'
    ) THEN
        ALTER TABLE public.groups
            ADD CONSTRAINT groups_project_id_fkey
            FOREIGN KEY (project_id) REFERENCES public.projects(id)
            ON DELETE SET NULL
            NOT VALID;
    END IF;
END;
$$;

-- Reconcile log table — one row per nightly reconcile run per table.
-- The nightly job (added in Task 5) inserts a row per table_name with the
-- count of rows whose canonical state differs from legacy state.
CREATE TABLE IF NOT EXISTS legacy_nav_reconcile_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    ran_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    table_name TEXT NOT NULL,
    -- BIGINT (i64) so a future explosion in drift count cannot wrap to negative
    -- and trip the CHECK; same width that pg COUNT(*) returns natively.
    drift_rows BIGINT NOT NULL,
    sample JSONB,
    -- BIGINT for parity with drift_rows; long-running reconcile runs (>24 days
    -- of accumulated ms) cannot silently truncate.
    duration_ms BIGINT NOT NULL,
    CHECK (drift_rows >= 0)
);

CREATE INDEX IF NOT EXISTS legacy_nav_reconcile_log_table_ran_idx
    ON legacy_nav_reconcile_log(table_name, ran_at DESC);

COMMIT;
