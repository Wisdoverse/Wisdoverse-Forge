-- Migration 011: enforce "same email domain ⇒ same org" policy.
--
-- Adds:
-- 1. `organizations.email_domain` (unique) — marks the canonical org for that domain.
-- 2. `public_email_domains` table — domains we DO NOT auto-group on (gmail, qq, etc).
--
-- Backfills:
-- For every non-public domain present in `users`, picks one canonical org and
-- ensures every user with that domain is a member. The canonical org is the
-- one already containing the most members of that domain (tie-broken by oldest
-- created_at). Existing data in non-canonical orgs is left in place — users can
-- still reach it via switch-context — so this migration is non-destructive.

ALTER TABLE organizations
    ADD COLUMN IF NOT EXISTS email_domain TEXT;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'organizations_email_domain_key') THEN
        CREATE UNIQUE INDEX organizations_email_domain_key ON organizations(email_domain) WHERE email_domain IS NOT NULL;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS public_email_domains (
    domain TEXT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO public_email_domains (domain) VALUES
    ('gmail.com'),
    ('googlemail.com'),
    ('yahoo.com'),
    ('hotmail.com'),
    ('outlook.com'),
    ('live.com'),
    ('msn.com'),
    ('icloud.com'),
    ('me.com'),
    ('aol.com'),
    ('mail.com'),
    ('protonmail.com'),
    ('proton.me'),
    ('qq.com'),
    ('163.com'),
    ('126.com'),
    ('sina.com'),
    ('sina.cn'),
    ('sohu.com'),
    ('foxmail.com'),
    ('yeah.net'),
    ('139.com'),
    ('189.cn')
ON CONFLICT (domain) DO NOTHING;

-- Backfill canonical orgs per non-public domain. Wrapped in a function so the
-- per-domain loop is readable and the migration stays idempotent (re-running
-- this migration is a no-op once email_domain is set).
DO $$
DECLARE
    rec RECORD;
    canonical_org UUID;
BEGIN
    FOR rec IN
        SELECT DISTINCT lower(split_part(email, '@', 2)) AS domain
        FROM users
        WHERE email LIKE '%@%' AND deleted_at IS NULL
    LOOP
        -- Skip public providers and any domain already mapped.
        CONTINUE WHEN EXISTS (SELECT 1 FROM public_email_domains WHERE domain = rec.domain);
        CONTINUE WHEN EXISTS (SELECT 1 FROM organizations WHERE email_domain = rec.domain);

        -- Pick the org containing the most users of this domain. Tiebreak by
        -- created_at ASC so re-running deterministically picks the same org.
        SELECT om.organization_id INTO canonical_org
        FROM organization_members om
        JOIN users u ON u.id = om.user_id
        JOIN organizations o ON o.id = om.organization_id
        WHERE lower(split_part(u.email, '@', 2)) = rec.domain
          AND o.deleted_at IS NULL
        GROUP BY om.organization_id, o.created_at
        ORDER BY COUNT(*) DESC, o.created_at ASC
        LIMIT 1;

        CONTINUE WHEN canonical_org IS NULL;

        -- Mark it canonical for this domain.
        UPDATE organizations SET email_domain = rec.domain WHERE id = canonical_org;

        -- Add every user of this domain as a member of the canonical org.
        -- Existing members keep their role; new ones land as 'member'.
        INSERT INTO organization_members (organization_id, user_id, role)
        SELECT canonical_org, u.id, 'member'
        FROM users u
        WHERE lower(split_part(u.email, '@', 2)) = rec.domain
          AND u.deleted_at IS NULL
        ON CONFLICT (organization_id, user_id) DO NOTHING;
    END LOOP;
END $$;
