-- Reconcile function: returns one row per (table_name, drift_rows) tuple.
--
-- Wrapped in a SQL function (not a plain SELECT) so the body can `to_regclass`-
-- gate each per-table block via dynamic EXECUTE. Required because Postgres
-- resolves table references in plain SELECT at parse time — a fresh DB
-- without the `legacy.*` schema would otherwise reject the script with
-- `relation "legacy.teams" does not exist`, blocking #[sqlx::test] coverage
-- and breaking the Phase-4 transition window where legacy.* will be dropped
-- before the reconcile job is removed.
--
-- Each missing legacy table reports drift = 0 (no work to reconcile if there's
-- no source-of-truth). Production with legacy.* present runs the COUNT(*)
-- diff query as before.

CREATE OR REPLACE FUNCTION public.legacy_nav_reconcile()
RETURNS TABLE(table_name text, drift_rows bigint)
LANGUAGE plpgsql
STABLE
AS $fn$
BEGIN
    -- teams
    IF to_regclass('legacy.teams') IS NOT NULL THEN
        RETURN QUERY EXECUTE $sql$
            SELECT 'teams'::text, COUNT(*)::bigint
              FROM public.teams t
              JOIN legacy.teams lt ON lt.id = t.id
             WHERE t.slug IS DISTINCT FROM lt.slug
                OR t.visibility IS DISTINCT FROM COALESCE(lt.visibility, 'private')
                OR t.description IS DISTINCT FROM COALESCE(lt.description, '')
        $sql$;
    ELSE
        RETURN QUERY SELECT 'teams'::text, 0::bigint;
    END IF;

    -- projects
    IF to_regclass('legacy.projects') IS NOT NULL THEN
        RETURN QUERY EXECUTE $sql$
            SELECT 'projects'::text, COUNT(*)::bigint
              FROM public.projects p
              JOIN legacy.projects lp ON lp.id = p.id
             WHERE p.team_id IS DISTINCT FROM lp.team_id
                OR p.slug IS DISTINCT FROM lp.slug
                OR p.color IS DISTINCT FROM COALESCE(lp.color, '#007AFF')
                OR p.description IS DISTINCT FROM COALESCE(lp.description, '')
        $sql$;
    ELSE
        RETURN QUERY SELECT 'projects'::text, 0::bigint;
    END IF;

    -- groups
    IF to_regclass('legacy.groups') IS NOT NULL THEN
        RETURN QUERY EXECUTE $sql$
            SELECT 'groups'::text, COUNT(*)::bigint
              FROM public.groups g
              JOIN legacy.groups lg ON lg.id = g.id
             WHERE g.project_id IS DISTINCT FROM lg.project_id
        $sql$;
    ELSE
        RETURN QUERY SELECT 'groups'::text, 0::bigint;
    END IF;
END;
$fn$;
