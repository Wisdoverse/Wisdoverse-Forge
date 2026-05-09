-- Prepare the legacy Go/TypeScript schema so Rust can reuse the same database.
-- Compatible legacy tables are moved into the `legacy` schema and preserved there.

CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE SCHEMA IF NOT EXISTS legacy;

-- Deterministic UUIDs let us safely remap legacy bigint/text identifiers.
CREATE OR REPLACE FUNCTION legacy_stable_uuid(input TEXT) RETURNS UUID AS $$
SELECT (
    substr(md5(input), 1, 8) || '-' ||
    substr(md5(input), 9, 4) || '-' ||
    '4' || substr(md5(input), 13, 3) || '-' ||
    substr('89ab', (get_byte(decode(substr(md5(input), 17, 2), 'hex'), 0) % 4) + 1, 1) ||
    substr(md5(input), 18, 3) || '-' ||
    substr(md5(input), 21, 12)
)::uuid;
$$ LANGUAGE sql IMMUTABLE STRICT;

CREATE OR REPLACE FUNCTION legacy_is_uuid(input TEXT) RETURNS BOOLEAN AS $$
SELECT input IS NOT NULL
   AND input ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$';
$$ LANGUAGE sql IMMUTABLE;

DO $$
DECLARE
    tbl TEXT;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'org_members',
        'teams',
        'team_members',
        'projects',
        'agents',
        'events',
        'api_keys',
        'git_credentials',
        'llm_provider_configs',
        'feature_flags',
        'favorites',
        'groups',
        'agent_collaborators',
        'participants',
        'resource_profiles',
        'subscriptions',
        'licenses',
        'voice_provider_configs',
        'user_ssh_keys'
    ]
    LOOP
        IF to_regclass(format('public.%I', tbl)) IS NOT NULL
           AND to_regclass(format('legacy.%I', tbl)) IS NULL THEN
            EXECUTE format('ALTER TABLE public.%I SET SCHEMA legacy', tbl);
        END IF;
    END LOOP;
END;
$$;

-- Keep the legacy tenant and identity tables in-place, but extend them to the
-- Rust shape so existing rows continue to deserialize correctly.
--
-- Guarded by to_regclass so migration 000 is a no-op on a fresh DB (where
-- organizations / users don't exist yet — 001_init.sql creates them, and
-- the ADD COLUMN IF NOT EXISTS clauses there already make those columns
-- canonical). Without the guard, `cargo sqlx migrate run` or `sqlx::test`
-- against a fresh DB panics with `relation "organizations" does not exist`,
-- blocking every #[sqlx::test] in the repo.
DO $$
BEGIN
    IF to_regclass('public.organizations') IS NOT NULL THEN
        ALTER TABLE organizations
            ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

        UPDATE organizations
        SET created_at = now()
        WHERE created_at IS NULL;

        UPDATE organizations
        SET updated_at = COALESCE(updated_at, created_at, now())
        WHERE updated_at IS NULL;

        ALTER TABLE organizations
            ALTER COLUMN created_at SET DEFAULT now(),
            ALTER COLUMN updated_at SET DEFAULT now(),
            ALTER COLUMN created_at SET NOT NULL,
            ALTER COLUMN updated_at SET NOT NULL;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('public.users') IS NOT NULL THEN
        ALTER TABLE users
            ADD COLUMN IF NOT EXISTS display_name TEXT,
            ADD COLUMN IF NOT EXISTS is_admin BOOLEAN NOT NULL DEFAULT false,
            ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS last_login_at TIMESTAMPTZ;

        -- Only run the data-massage UPDATEs if legacy columns exist — a
        -- fresh DB doesn't have them so their NULLIF / COALESCE references
        -- would fail to parse.
        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'name'
        ) THEN
            UPDATE users
            SET display_name = COALESCE(
                    display_name,
                    NULLIF(name, ''),
                    NULLIF(system_username, ''),
                    split_part(email, '@', 1)
                )
            WHERE display_name IS NULL;
        END IF;

        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'role'
        ) THEN
            UPDATE users
            SET is_admin = (role = 'admin')
            WHERE role IS NOT NULL;
        END IF;

        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'last_login'
        ) THEN
            UPDATE users
            SET last_login_at = COALESCE(last_login_at, last_login)
            WHERE last_login_at IS NULL
              AND last_login IS NOT NULL;
        END IF;

        UPDATE users
        SET created_at = now()
        WHERE created_at IS NULL;

        UPDATE users
        SET updated_at = COALESCE(updated_at, created_at, now())
        WHERE updated_at IS NULL;

        ALTER TABLE users
            ALTER COLUMN email SET NOT NULL,
            ALTER COLUMN created_at SET DEFAULT now(),
            ALTER COLUMN updated_at SET DEFAULT now(),
            ALTER COLUMN created_at SET NOT NULL,
            ALTER COLUMN updated_at SET NOT NULL,
            ALTER COLUMN is_admin SET DEFAULT false,
            ALTER COLUMN is_admin SET NOT NULL;
    END IF;
END;
$$;
