-- 030_user_llm_configs_adopt.sql — adopt user_llm_configs into Rust migrations.
--
-- Background: mirrors migration 029's adoption of user_cli_credentials. The
-- table was originally created by legacy TS Prisma migrations; this migration
-- (a) adopts ownership of the base table into Rust migrations so #[sqlx::test]
-- integration tests stand up the schema from scratch, and (b) carries only the
-- columns UserLlmConfigRepository currently queries. CREATE TABLE / ADD COLUMN
-- are both IF NOT EXISTS so this is idempotent on production databases that
-- already have the table.
--
-- Columns that exist in production but are not queried by Rust code today
-- (model, display_name, base_url, api_key_prefix) are deliberately NOT declared
-- here — Rust migrations own only what Rust reads. They remain owned by the
-- legacy TS/Prisma migration chain.

CREATE TABLE IF NOT EXISTS user_llm_configs (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id            UUID NOT NULL,
    provider           TEXT NOT NULL,
    encrypted_api_key  TEXT NOT NULL,
    is_default         BOOLEAN NOT NULL DEFAULT false,
    is_enabled         BOOLEAN NOT NULL DEFAULT true,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Supporting indexes for find_default_api_key's ORDER BY clause on a row-heavy
-- table (every user × every provider). Partial on is_enabled mirrors the
-- repository's WHERE clause.
CREATE INDEX IF NOT EXISTS idx_user_llm_configs_user_provider_enabled
    ON user_llm_configs (user_id, provider)
    WHERE is_enabled = TRUE;
