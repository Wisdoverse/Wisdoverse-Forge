-- 029_cli_credentials_revoked.sql — revocation tracking for OAuth refresh failures.
--
-- The user_cli_credentials table was originally created by legacy TS Prisma
-- migrations. This migration (a) adopts ownership of the base table into Rust
-- migrations so #[sqlx::test] integration tests can stand up the schema from
-- scratch, and (b) adds the revocation-tracking columns. CREATE TABLE / ADD
-- COLUMN are both IF NOT EXISTS so this is idempotent on production databases
-- where the table already exists with the base columns.

CREATE TABLE IF NOT EXISTS user_cli_credentials (
    user_id               UUID NOT NULL,
    cli_tool              TEXT NOT NULL,
    encrypted_credentials TEXT NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, cli_tool)
);

ALTER TABLE user_cli_credentials
    ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS revoke_reason TEXT,
    ADD COLUMN IF NOT EXISTS refresh_fail_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_refresh_error TEXT,
    ADD COLUMN IF NOT EXISTS last_refresh_error_at TIMESTAMPTZ;

-- Partial index: the refresh worker scans only non-revoked rows. Keeps the
-- index small and avoids re-attempting revoked creds on every sweep.
CREATE INDEX IF NOT EXISTS idx_user_cli_credentials_active
    ON user_cli_credentials (cli_tool)
    WHERE revoked_at IS NULL;
