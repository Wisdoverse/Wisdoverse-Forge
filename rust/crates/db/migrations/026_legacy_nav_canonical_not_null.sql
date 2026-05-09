-- P3 MR-B — tighten canonical legacy-nav columns to NOT NULL.
--
-- Per ADR 0001 + the P3 plan, three columns get NOT NULL:
--   - public.teams.slug
--   - public.projects.team_id
--   - public.projects.slug
--
-- EXCLUDED intentionally:
--   - public.groups.project_id — ADR 0001 keeps it nullable for pre-project
--     groups
--   - public.teams.{visibility, description} — still surfaced via
--     COALESCE('private', '') in the legacy DTO; no business reason to deny
--     NULL
--   - public.projects.{color, description} — same
--
-- Pre-NOT-NULL safety: we backfill any remaining NULL rows first, so
-- SET NOT NULL takes the PG 12+ fast path (no table rewrite, ACCESS
-- EXCLUSIVE held only long enough to flip pg_attribute).
--
-- Collision handling: migrations 022 and 023 declare unique indexes
-- `teams_org_slug_idx (organization_id, slug) WHERE slug IS NOT NULL` and
-- `projects_team_slug_idx (team_id, slug) WHERE team_id IS NOT NULL AND
-- slug IS NOT NULL`. Two rows can slugify to the same string (e.g. two
-- projects named "AgentForge" under the same team). The backfill appends
-- the short form of the row's UUID when — and only when — a naïve
-- slugify collides with an existing row; a bare slugify otherwise.
--
-- Orphan orgs: if an org has zero surviving teams, there is no candidate
-- parent for a NULL-team_id project. We create a "Default" team for such
-- orgs rather than fail loud; the call graph that produced the orphan
-- (old project create path that predates `projects.team_id`) is the same
-- call graph MR-B fixes, so a human-intervention requirement here would
-- just block deploys that MR-B itself unblocks.

BEGIN;

-- Step 1: ensure every organization has at least one surviving team.
-- Needed before the projects.team_id backfill can resolve a parent.
INSERT INTO public.teams (organization_id, name, slug, visibility, description)
SELECT o.id, 'Default', 'default', 'private', 'Auto-created by migration 026 for an org with zero teams.'
  FROM public.organizations o
 WHERE NOT EXISTS (
         SELECT 1 FROM public.teams t
          WHERE t.organization_id = o.id AND t.deleted_at IS NULL
       );

-- Step 2: backfill teams.slug. Naïve slugify first; rows that collide
-- against the (organization_id, slug) unique index get an 8-char uuid
-- disambiguator. Safety net — the P2 backfill (migration 009) already
-- populated slug from legacy.teams so this is ~0 rows on prod.
UPDATE public.teams t
   SET slug = CASE
              WHEN EXISTS (
                   SELECT 1 FROM public.teams t2
                    WHERE t2.organization_id = t.organization_id
                      AND t2.slug = lower(regexp_replace(t.name, '[^a-zA-Z0-9]+', '-', 'g'))
                      AND t2.id <> t.id
                      AND t2.slug IS NOT NULL
              )
              THEN lower(regexp_replace(t.name, '[^a-zA-Z0-9]+', '-', 'g')) || '-' || substring(t.id::text, 1, 8)
              ELSE lower(regexp_replace(t.name, '[^a-zA-Z0-9]+', '-', 'g'))
              END
 WHERE t.slug IS NULL;

-- Step 3: backfill projects.team_id from the org's oldest surviving team.
UPDATE public.projects p
   SET team_id = (
        SELECT t.id FROM public.teams t
         WHERE t.organization_id = p.organization_id
           AND t.deleted_at IS NULL
         ORDER BY t.created_at ASC
         LIMIT 1
   )
 WHERE p.team_id IS NULL;

-- Step 4: backfill projects.slug, with the same collision-aware rule as
-- teams.slug. The uniqueness index is (team_id, slug), so the collision
-- scope is scoped to the target team.
UPDATE public.projects p
   SET slug = CASE
              WHEN EXISTS (
                   SELECT 1 FROM public.projects p2
                    WHERE p2.team_id = p.team_id
                      AND p2.slug = lower(regexp_replace(p.name, '[^a-zA-Z0-9]+', '-', 'g'))
                      AND p2.id <> p.id
                      AND p2.slug IS NOT NULL
              )
              THEN lower(regexp_replace(p.name, '[^a-zA-Z0-9]+', '-', 'g')) || '-' || substring(p.id::text, 1, 8)
              ELSE lower(regexp_replace(p.name, '[^a-zA-Z0-9]+', '-', 'g'))
              END
 WHERE p.slug IS NULL;

-- Step 5: sanity gate — refuse to ALTER COLUMN if the backfill left any
-- row NULL. Should be zero after steps 2-4; the DO block surfaces the
-- row counts in the error message so operators can grep the migration
-- log without digging into `_sqlx_migrations` to see what slipped
-- through.
DO $$
DECLARE
    null_teams_slug    bigint;
    null_projects_tid  bigint;
    null_projects_slug bigint;
BEGIN
    SELECT count(*) INTO null_teams_slug    FROM public.teams    WHERE slug    IS NULL;
    SELECT count(*) INTO null_projects_tid  FROM public.projects WHERE team_id IS NULL;
    SELECT count(*) INTO null_projects_slug FROM public.projects WHERE slug    IS NULL;
    IF null_teams_slug + null_projects_tid + null_projects_slug > 0 THEN
        RAISE EXCEPTION 'migration 026: backfill left NULL rows — '
            'teams.slug=%, projects.team_id=%, projects.slug=% — '
            'fix manually before re-running',
            null_teams_slug, null_projects_tid, null_projects_slug;
    END IF;
END $$;

ALTER TABLE public.teams    ALTER COLUMN slug    SET NOT NULL;
ALTER TABLE public.projects ALTER COLUMN team_id SET NOT NULL;
ALTER TABLE public.projects ALTER COLUMN slug    SET NOT NULL;

COMMIT;
