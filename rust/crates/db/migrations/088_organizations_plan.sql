-- Legacy navigation reads `COALESCE(o.plan, 'free')` for `/api/v1/orgs`.
-- The fresh `001_init.sql` schema never created `organizations.plan` (the Rust
-- plan model lives in `billing_plans`; see 009_legacy_backfill), so a fresh
-- install 500s on the orgs list and the whole navigation switcher is dead.
-- This corrective migration re-adds the legacy column so the read path works
-- on both legacy-upgraded and fresh databases; nothing writes it, so a fresh
-- row simply resolves to the `'free'` fallback.
ALTER TABLE organizations
    ADD COLUMN IF NOT EXISTS plan TEXT;
