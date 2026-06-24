-- P4 — close Issue #15. Drop the `legacy.*` schema, the reconcile
-- function, and the feature_flags rows that gated the P3 dual-read.
--
-- Per ADR 0004:
--   - Drop schema outright (no rename-to-archive — canonical has all the data)
--   - Delete `legacy_nav.canonical_read_enabled` feature_flag rows (no readers)
--   - Drop `public.legacy_nav_reconcile()` (its only caller — the job — is
--     deleted in the same MR)
--
-- Pre-migration sanity: refuse to run if any tenant still has an explicit
-- `enabled = false` opt-out row. Post-MR-B, the selector's opt-OUT kill
-- switch is the only consumer; opt-out rows mean a tenant is still being
-- served legacy SQL and deleting `legacy.*` would 500 their reads. The
-- operator must resolve each opt-out before this migration can land.
--
-- Rollback: DB backup restore. `DROP SCHEMA CASCADE` is irreversible
-- without it. Verify backup + retention before executing in prod.

BEGIN;

DO $$
DECLARE
    opt_out_rows bigint;
BEGIN
    SELECT count(*) INTO opt_out_rows
      FROM feature_flags
     WHERE name = 'legacy_nav.canonical_read_enabled'
       AND enabled = false;
    IF opt_out_rows > 0 THEN
        RAISE EXCEPTION 'migration 027: % tenant(s) still opted out of '
            'canonical reads (enabled = false) — resolve them before '
            'dropping legacy.*, see ADR 0004', opt_out_rows;
    END IF;
END $$;

-- Drop the reconcile function BEFORE the schema so a stray scheduler
-- tick cannot panic on a half-dropped dependency chain.
DROP FUNCTION IF EXISTS public.legacy_nav_reconcile();

-- Drop the schema and every table under it in one CASCADE. Includes any
-- auto-created indexes, GRANTs, and stored snippets. `IF EXISTS` keeps
-- the migration idempotent for fresh databases where `legacy.*` was
-- never populated (migration 000_legacy_prepare.sql only creates the
-- schema when a TS-era installation is being upgraded).
DROP SCHEMA IF EXISTS legacy CASCADE;

-- Flag rows have no readers — selector service is deleted in the same
-- MR. Single DELETE, no ON CONFLICT needed.
DELETE FROM feature_flags WHERE name = 'legacy_nav.canonical_read_enabled';

COMMIT;
