-- 017_legacy_nav_backfill.sql
-- Populate the 8 PROMOTE columns from legacy.* — idempotent + batched.
-- Re-runs are no-ops (WHERE clause filters unchanged rows).
-- Order: teams → projects → groups (FK chain).

-- Section A — teams
DO $$
DECLARE
    updated_count INTEGER := 1;
    total INTEGER := 0;
BEGIN
    IF to_regclass('legacy.teams') IS NULL THEN
        RAISE NOTICE 'legacy.teams missing — skipping teams backfill';
        RETURN;
    END IF;

    WHILE updated_count > 0 LOOP
        WITH batch AS (
            SELECT t.id, lt.slug, lt.visibility, lt.description
            FROM public.teams t
            JOIN legacy.teams lt ON lt.id = t.id
            WHERE
                t.slug IS DISTINCT FROM lt.slug
                OR t.visibility IS DISTINCT FROM COALESCE(lt.visibility, 'private')
                OR t.description IS DISTINCT FROM COALESCE(lt.description, '')
            LIMIT 5000
        ),
        upd AS (
            UPDATE public.teams pt
            SET slug = b.slug,
                visibility = COALESCE(b.visibility, 'private'),
                description = COALESCE(b.description, '')
            FROM batch b
            WHERE pt.id = b.id
            RETURNING pt.id
        )
        SELECT COUNT(*) INTO updated_count FROM upd;

        total := total + updated_count;
        EXIT WHEN updated_count = 0;
    END LOOP;

    RAISE NOTICE 'teams backfill: % rows updated', total;
END;
$$;

-- Section B — projects (team_id + slug + color + description)
DO $$
DECLARE
    updated_count INTEGER := 1;
    total INTEGER := 0;
BEGIN
    IF to_regclass('legacy.projects') IS NULL THEN
        RAISE NOTICE 'legacy.projects missing — skipping projects backfill';
        RETURN;
    END IF;

    WHILE updated_count > 0 LOOP
        WITH batch AS (
            SELECT p.id,
                   lp.team_id,
                   lp.slug,
                   COALESCE(lp.color, '#007AFF') AS color,
                   COALESCE(lp.description, '') AS description
            FROM public.projects p
            JOIN legacy.projects lp ON lp.id = p.id
            WHERE
                p.team_id IS DISTINCT FROM lp.team_id
                OR p.slug IS DISTINCT FROM lp.slug
                OR p.color IS DISTINCT FROM COALESCE(lp.color, '#007AFF')
                OR p.description IS DISTINCT FROM COALESCE(lp.description, '')
            LIMIT 5000
        ),
        upd AS (
            UPDATE public.projects pp
            SET team_id = b.team_id,
                slug = b.slug,
                color = b.color,
                description = b.description
            FROM batch b
            WHERE pp.id = b.id
            RETURNING pp.id
        )
        SELECT COUNT(*) INTO updated_count FROM upd;

        total := total + updated_count;
        EXIT WHEN updated_count = 0;
    END LOOP;

    RAISE NOTICE 'projects backfill: % rows updated', total;
END;
$$;

-- Section C — groups (project_id)
DO $$
DECLARE
    updated_count INTEGER := 1;
    total INTEGER := 0;
BEGIN
    IF to_regclass('legacy.groups') IS NULL THEN
        RAISE NOTICE 'legacy.groups missing — skipping groups backfill';
        RETURN;
    END IF;

    WHILE updated_count > 0 LOOP
        WITH batch AS (
            SELECT g.id, lg.project_id
            FROM public.groups g
            JOIN legacy.groups lg ON lg.id = g.id
            WHERE g.project_id IS DISTINCT FROM lg.project_id
            LIMIT 5000
        ),
        upd AS (
            UPDATE public.groups pg
            SET project_id = b.project_id
            FROM batch b
            WHERE pg.id = b.id
            RETURNING pg.id
        )
        SELECT COUNT(*) INTO updated_count FROM upd;

        total := total + updated_count;
        EXIT WHEN updated_count = 0;
    END LOOP;

    RAISE NOTICE 'groups backfill: % rows updated', total;
END;
$$;
