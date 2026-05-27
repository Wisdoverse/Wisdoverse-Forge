-- 062: add runtime_kind column to agents, backfill from current shape.
--
-- The DEFAULT 'api' is set AFTER backfill so existing rows pick up their
-- correct kind. The DEFAULT then covers any INSERT from an old API instance
-- during the rolling deploy window between 062 and 063.
--
-- CHECK constraints land in 063 AFTER new application code is fully deployed
-- to avoid rolling-deploy CHECK violations.

SET lock_timeout      = '3s';
SET statement_timeout = '30s';

ALTER TABLE agents
    ADD COLUMN IF NOT EXISTS runtime_kind TEXT;

-- Batched backfill: 5000 rows per pass, FOR UPDATE SKIP LOCKED keeps WAL bounded
-- and lets concurrent writers proceed.
DO $$
DECLARE
    batch_size INT := 5000;
    affected   INT;
BEGIN
    LOOP
        WITH targets AS (
            SELECT id FROM agents
            WHERE runtime_kind IS NULL
            ORDER BY id
            LIMIT batch_size
            FOR UPDATE SKIP LOCKED
        )
        UPDATE agents a SET runtime_kind = CASE
            WHEN a.cli_tool IS NULL                                       THEN 'api'
            WHEN a.runtime_id IS NOT NULL AND a.runtime_id LIKE 'host-%'  THEN 'cli'
            ELSE                                                                'container'
        END
        FROM targets WHERE a.id = targets.id;
        GET DIAGNOSTICS affected = ROW_COUNT;
        EXIT WHEN affected = 0;
    END LOOP;
END $$;

-- Pre-flight invariant assertion. Any row that would later violate the
-- joint CHECK from 063 must surface NOW, before NOT NULL is set, so an
-- operator can intervene before 063 hard-locks the constraint.
DO $$
DECLARE
    bad_rows INT;
BEGIN
    SELECT COUNT(*) INTO bad_rows FROM agents
    WHERE NOT (
        (runtime_kind = 'container' AND cli_tool IS NOT NULL)
        OR (runtime_kind = 'cli'    AND cli_tool IS NOT NULL AND container_id IS NULL)
        OR (runtime_kind = 'api'    AND cli_tool IS NULL)
    );
    IF bad_rows > 0 THEN
        RAISE EXCEPTION
            'Migration 062: % rows would violate agents_runtime_kind_invariants. '
            'Inspect: SELECT id, runtime_kind, cli_tool, container_id, runtime_id FROM agents WHERE NOT (...). '
            'Resolve before 063 ships.',
            bad_rows;
    END IF;
END $$;

ALTER TABLE agents
    ALTER COLUMN runtime_kind SET NOT NULL,
    ALTER COLUMN runtime_kind SET DEFAULT 'api';
