-- 036_user_credential_schema_ownership.sql
--
-- Promote the legacy-owned user credential tables into the Rust migration
-- contract without editing already-applied migrations 029/030. Editing those
-- files would change sqlx migration checksums on deployed databases. This
-- migration is additive/idempotent and makes a fresh Rust test database land
-- on the same base schema shape observed in production.

-- user_cli_credentials ------------------------------------------------------

ALTER TABLE user_cli_credentials
    ADD COLUMN IF NOT EXISTS id UUID DEFAULT gen_random_uuid();

UPDATE user_cli_credentials
SET id = gen_random_uuid()
WHERE id IS NULL;

ALTER TABLE user_cli_credentials
    ALTER COLUMN id SET NOT NULL,
    ALTER COLUMN id SET DEFAULT gen_random_uuid(),
    ALTER COLUMN cli_tool TYPE VARCHAR(20) USING cli_tool::VARCHAR(20),
    ALTER COLUMN created_at DROP NOT NULL,
    ALTER COLUMN updated_at DROP NOT NULL;

DO $$
DECLARE
    pk_name TEXT;
    pk_cols TEXT;
BEGIN
    SELECT c.conname, string_agg(a.attname, ',' ORDER BY ord.ordinality)
      INTO pk_name, pk_cols
      FROM pg_constraint c
      JOIN unnest(c.conkey) WITH ORDINALITY AS ord(attnum, ordinality) ON TRUE
      JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = ord.attnum
     WHERE c.conrelid = 'user_cli_credentials'::regclass
       AND c.contype = 'p'
     GROUP BY c.conname;

    IF pk_name IS NULL THEN
        ALTER TABLE user_cli_credentials
            ADD CONSTRAINT user_cli_credentials_pkey PRIMARY KEY (id);
    ELSIF pk_cols <> 'id' THEN
        EXECUTE format('ALTER TABLE user_cli_credentials DROP CONSTRAINT %I', pk_name);
        ALTER TABLE user_cli_credentials
            ADD CONSTRAINT user_cli_credentials_pkey PRIMARY KEY (id);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
          FROM pg_constraint
         WHERE conrelid = 'user_cli_credentials'::regclass
           AND conname = 'user_cli_credentials_user_id_cli_tool_key'
    ) THEN
        ALTER TABLE user_cli_credentials
            ADD CONSTRAINT user_cli_credentials_user_id_cli_tool_key UNIQUE (user_id, cli_tool);
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
          FROM pg_constraint
         WHERE conrelid = 'user_cli_credentials'::regclass
           AND conname = 'user_cli_credentials_user_id_fkey'
    ) THEN
        ALTER TABLE user_cli_credentials
            ADD CONSTRAINT user_cli_credentials_user_id_fkey
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_user_cli_credentials_user
    ON user_cli_credentials (user_id);

-- user_llm_configs ----------------------------------------------------------

ALTER TABLE user_llm_configs
    ADD COLUMN IF NOT EXISTS model VARCHAR(100),
    ADD COLUMN IF NOT EXISTS display_name VARCHAR(100),
    ADD COLUMN IF NOT EXISTS base_url TEXT,
    ADD COLUMN IF NOT EXISTS api_key_prefix VARCHAR(20),
    ADD COLUMN IF NOT EXISTS settings JSONB DEFAULT '{}'::jsonb;

ALTER TABLE user_llm_configs
    ALTER COLUMN provider TYPE VARCHAR(50) USING provider::VARCHAR(50),
    ALTER COLUMN model TYPE VARCHAR(100) USING model::VARCHAR(100),
    ALTER COLUMN display_name TYPE VARCHAR(100) USING display_name::VARCHAR(100),
    ALTER COLUMN api_key_prefix TYPE VARCHAR(20) USING api_key_prefix::VARCHAR(20),
    ALTER COLUMN settings SET DEFAULT '{}'::jsonb,
    ALTER COLUMN is_default DROP NOT NULL,
    ALTER COLUMN is_enabled DROP NOT NULL,
    ALTER COLUMN created_at DROP NOT NULL,
    ALTER COLUMN updated_at DROP NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
          FROM pg_constraint
         WHERE conrelid = 'user_llm_configs'::regclass
           AND conname = 'user_llm_configs_user_id_fkey'
    ) THEN
        ALTER TABLE user_llm_configs
            ADD CONSTRAINT user_llm_configs_user_id_fkey
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_user_llm_configs_user
    ON user_llm_configs (user_id);

CREATE INDEX IF NOT EXISTS idx_user_llm_configs_default
    ON user_llm_configs (user_id)
    WHERE is_default = TRUE;

CREATE UNIQUE INDEX IF NOT EXISTS uq_user_llm_provider_model
    ON user_llm_configs (user_id, provider, model)
    WHERE model IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uq_user_llm_provider_no_model
    ON user_llm_configs (user_id, provider)
    WHERE model IS NULL;
