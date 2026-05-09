-- Backfill Rust tables from the preserved legacy schema.
-- The source rows stay in `legacy.*` for auditability and rollback debugging.

DO $$
BEGIN
    IF to_regclass('legacy.org_members') IS NOT NULL THEN
        INSERT INTO organization_members (organization_id, user_id, role, created_at)
        SELECT
            om.org_id,
            om.user_id,
            om.role,
            COALESCE(om.joined_at, now())
        FROM legacy.org_members om
        ON CONFLICT (organization_id, user_id) DO UPDATE
        SET role = EXCLUDED.role,
            created_at = LEAST(organization_members.created_at, EXCLUDED.created_at);
    END IF;
END;
$$;

CREATE TEMP VIEW legacy_org_default_users AS
SELECT DISTINCT ON (om.organization_id)
    om.organization_id,
    om.user_id
FROM organization_members om
ORDER BY
    om.organization_id,
    CASE om.role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END,
    om.created_at,
    om.user_id;

CREATE TEMP VIEW legacy_user_org_memberships AS
SELECT DISTINCT
    om.organization_id,
    om.user_id
FROM organization_members om;

INSERT INTO workspaces (id, organization_id, name, created_at, updated_at, deleted_at)
SELECT
    o.id,
    o.id,
    'Default Workspace',
    o.created_at,
    o.updated_at,
    o.deleted_at
FROM organizations o
ON CONFLICT (id) DO UPDATE
SET organization_id = EXCLUDED.organization_id,
    name = EXCLUDED.name,
    created_at = LEAST(workspaces.created_at, EXCLUDED.created_at),
    updated_at = GREATEST(workspaces.updated_at, EXCLUDED.updated_at),
    deleted_at = EXCLUDED.deleted_at;

-- organizations.settings + users.preferences exist on the legacy schema only;
-- 001_init.sql creates these tables without those columns. Guard each
-- backfill via information_schema column-existence checks so a fresh DB
-- (no legacy data) skips them cleanly. Without these guards the parser
-- rejects `o.settings` / `u.preferences` and migration 009 fails with
-- `column ... does not exist`, blocking every #[sqlx::test] in the repo.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'organizations' AND column_name = 'settings'
    ) THEN
        EXECUTE $sql$
            INSERT INTO settings (id, organization_id, user_id, key, value, created_at, updated_at)
            SELECT
                legacy_stable_uuid('setting:org:' || o.id::text || ':organization_settings'),
                o.id,
                NULL,
                'organization_settings',
                o.settings,
                o.created_at,
                o.updated_at
            FROM organizations o
            WHERE COALESCE(o.settings, '{}'::jsonb) <> '{}'::jsonb
              AND NOT EXISTS (
                  SELECT 1
                  FROM settings s
                  WHERE s.organization_id = o.id
                    AND s.user_id IS NULL
                    AND s.key = 'organization_settings'
              )
        $sql$;
    END IF;
END;
$$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'preferences'
    ) THEN
        EXECUTE $sql$
            INSERT INTO settings (id, organization_id, user_id, key, value, created_at, updated_at)
            SELECT
                legacy_stable_uuid('setting:user:' || u.id::text || ':' || om.organization_id::text || ':preferences'),
                om.organization_id,
                u.id,
                'preferences',
                u.preferences,
                u.created_at,
                u.updated_at
            FROM users u
            JOIN legacy_user_org_memberships om ON om.user_id = u.id
            WHERE COALESCE(u.preferences, '{}'::jsonb) <> '{}'::jsonb
            ON CONFLICT (organization_id, user_id, key) DO UPDATE
            SET value = EXCLUDED.value,
                updated_at = EXCLUDED.updated_at
        $sql$;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.teams') IS NOT NULL THEN
        INSERT INTO teams (id, organization_id, name, created_at, updated_at, deleted_at)
        SELECT
            t.id,
            t.org_id,
            t.name,
            t.created_at,
            t.updated_at,
            NULL
        FROM legacy.teams t
        ON CONFLICT (id) DO UPDATE
        SET organization_id = EXCLUDED.organization_id,
            name = EXCLUDED.name,
            updated_at = EXCLUDED.updated_at,
            deleted_at = EXCLUDED.deleted_at;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.projects') IS NOT NULL
       AND to_regclass('legacy.teams') IS NOT NULL THEN
        INSERT INTO projects (id, organization_id, workspace_id, name, repository_url, created_at, updated_at, deleted_at)
        SELECT
            p.id,
            t.org_id,
            t.org_id,
            p.name,
            NULL,
            p.created_at,
            p.updated_at,
            NULL
        FROM legacy.projects p
        JOIN legacy.teams t ON t.id = p.team_id
        ON CONFLICT (id) DO UPDATE
        SET organization_id = EXCLUDED.organization_id,
            workspace_id = EXCLUDED.workspace_id,
            name = EXCLUDED.name,
            repository_url = EXCLUDED.repository_url,
            updated_at = EXCLUDED.updated_at,
            deleted_at = EXCLUDED.deleted_at;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.groups') IS NOT NULL THEN
        INSERT INTO groups (id, organization_id, name, description, created_by, created_at, updated_at, deleted_at)
        SELECT
            g.id,
            g.org_id,
            g.name,
            g.description,
            COALESCE(g.created_by, g.manager_id, od.user_id),
            g.created_at,
            g.created_at,
            NULL
        FROM legacy.groups g
        LEFT JOIN legacy_org_default_users od ON od.organization_id = g.org_id
        WHERE COALESCE(g.created_by, g.manager_id, od.user_id) IS NOT NULL
        ON CONFLICT (id) DO UPDATE
        SET organization_id = EXCLUDED.organization_id,
            name = EXCLUDED.name,
            description = EXCLUDED.description,
            created_by = EXCLUDED.created_by,
            updated_at = EXCLUDED.updated_at,
            deleted_at = EXCLUDED.deleted_at;
    END IF;
END;
$$;

INSERT INTO group_members (group_id, user_id, role, created_at)
SELECT
    g.id,
    g.created_by,
    'owner',
    g.created_at
FROM groups g
ON CONFLICT (group_id, user_id) DO UPDATE
SET role = EXCLUDED.role,
    created_at = LEAST(group_members.created_at, EXCLUDED.created_at);

DO $$
BEGIN
    IF to_regclass('legacy.groups') IS NOT NULL THEN
        INSERT INTO group_members (group_id, user_id, role, created_at)
        SELECT
            g.id,
            g.manager_id,
            'manager',
            g.created_at
        FROM legacy.groups g
        JOIN users u ON u.id = g.manager_id
        WHERE g.manager_id IS NOT NULL
        ON CONFLICT (group_id, user_id) DO UPDATE
        SET role = EXCLUDED.role,
            created_at = LEAST(group_members.created_at, EXCLUDED.created_at);
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.agents') IS NOT NULL THEN
        INSERT INTO agents (
            id,
            organization_id,
            project_id,
            user_id,
            name,
            status,
            container_id,
            cli_session_id,
            model,
            provider,
            started_at,
            ended_at,
            created_at,
            updated_at
        )
        SELECT
            a.id,
            a.org_id,
            a.project_id,
            COALESCE(a.user_id, od.user_id),
            NULLIF(a.name, ''),
            CASE lower(a.status)
                WHEN 'working' THEN 'working'::agent_status
                WHEN 'busy' THEN 'working'::agent_status
                WHEN 'running' THEN 'working'::agent_status
                WHEN 'idle' THEN 'idle'::agent_status
                WHEN 'online' THEN 'idle'::agent_status
                WHEN 'active' THEN 'idle'::agent_status
                ELSE 'offline'::agent_status
            END,
            a.container_id,
            a.cli_session_id,
            NULL,
            a.cli_tool,
            a.created_at,
            CASE
                WHEN lower(a.status) = 'offline' THEN COALESCE(a.last_activity, a.created_at)
                ELSE NULL
            END,
            a.created_at,
            COALESCE(a.last_activity, a.created_at)
        FROM legacy.agents a
        LEFT JOIN legacy_org_default_users od ON od.organization_id = a.org_id
        WHERE COALESCE(a.user_id, od.user_id) IS NOT NULL
        ON CONFLICT (id) DO UPDATE
        SET organization_id = EXCLUDED.organization_id,
            project_id = EXCLUDED.project_id,
            user_id = EXCLUDED.user_id,
            name = EXCLUDED.name,
            status = EXCLUDED.status,
            container_id = EXCLUDED.container_id,
            cli_session_id = EXCLUDED.cli_session_id,
            provider = EXCLUDED.provider,
            started_at = EXCLUDED.started_at,
            ended_at = EXCLUDED.ended_at,
            updated_at = EXCLUDED.updated_at;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.events') IS NOT NULL THEN
        INSERT INTO agents (
            id,
            organization_id,
            project_id,
            user_id,
            name,
            status,
            container_id,
            cli_session_id,
            model,
            provider,
            started_at,
            ended_at,
            created_at,
            updated_at
        )
        SELECT
            legacy_stable_uuid('recovered-agent:' || e.org_id::text || ':' || e.cli_session_id),
            e.org_id,
            NULL,
            od.user_id,
            'Recovered Session ' || left(e.cli_session_id, 8),
            'offline'::agent_status,
            NULL,
            e.cli_session_id,
            NULL,
            max(NULLIF(e.cli_tool, '')),
            min(COALESCE(e.created_at, e.timestamp, now())),
            max(COALESCE(e.created_at, e.timestamp, now())),
            min(COALESCE(e.created_at, e.timestamp, now())),
            max(COALESCE(e.created_at, e.timestamp, now()))
        FROM legacy.events e
        JOIN legacy_org_default_users od ON od.organization_id = e.org_id
        LEFT JOIN agents existing
          ON existing.organization_id = e.org_id
         AND existing.cli_session_id = e.cli_session_id
        WHERE existing.id IS NULL
          AND e.cli_session_id IS NOT NULL
        GROUP BY e.org_id, e.cli_session_id, od.user_id
        ON CONFLICT (id) DO UPDATE
        SET user_id = EXCLUDED.user_id,
            name = EXCLUDED.name,
            provider = EXCLUDED.provider,
            started_at = EXCLUDED.started_at,
            ended_at = EXCLUDED.ended_at,
            updated_at = EXCLUDED.updated_at;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.events') IS NOT NULL THEN
        INSERT INTO events (id, organization_id, agent_id, event_type, payload, session_id, created_at)
        SELECT
            CASE
                WHEN legacy_is_uuid(e.event_uuid) THEN e.event_uuid::uuid
                ELSE legacy_stable_uuid('event:' || e.id::text)
            END,
            matched_agent.organization_id,
            matched_agent.id,
            e.type,
            jsonb_strip_nulls(
                jsonb_build_object(
                    'legacy_event_id', e.id,
                    'tool', e.tool,
                    'tool_use_id', e.tool_use_id,
                    'input', e.input,
                    'output', e.output,
                    'success', e.success,
                    'duration_ms', e.duration_ms,
                    'timestamp', e.timestamp,
                    'cli_tool', e.cli_tool
                )
            ),
            e.cli_session_id,
            COALESCE(e.created_at, e.timestamp, now())
        FROM legacy.events e
        JOIN LATERAL (
            SELECT
                a.id,
                a.organization_id
            FROM agents a
            WHERE (
                    e.agent_id IS NOT NULL
                AND a.id = e.agent_id
            ) OR (
                    e.agent_id IS NULL
                AND e.cli_session_id IS NOT NULL
                AND a.organization_id = e.org_id
                AND a.cli_session_id = e.cli_session_id
            )
            ORDER BY a.created_at ASC
            LIMIT 1
        ) matched_agent ON TRUE
        ON CONFLICT (id) DO UPDATE
        SET organization_id = EXCLUDED.organization_id,
            agent_id = EXCLUDED.agent_id,
            event_type = EXCLUDED.event_type,
            payload = EXCLUDED.payload,
            session_id = EXCLUDED.session_id,
            created_at = EXCLUDED.created_at;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.api_keys') IS NOT NULL THEN
        INSERT INTO api_keys (
            id,
            organization_id,
            user_id,
            name,
            key_hash,
            key_prefix,
            scopes,
            expires_at,
            last_used_at,
            created_at,
            revoked_at
        )
        SELECT
            k.id,
            k.org_id,
            k.user_id,
            k.name,
            k.key_hash,
            k.key_prefix,
            COALESCE(k.scopes, ARRAY[]::text[]),
            k.expires_at,
            k.last_used_at,
            COALESCE(k.created_at, now()),
            k.revoked_at
        FROM legacy.api_keys k
        ON CONFLICT (id) DO UPDATE
        SET organization_id = EXCLUDED.organization_id,
            user_id = EXCLUDED.user_id,
            name = EXCLUDED.name,
            key_hash = EXCLUDED.key_hash,
            key_prefix = EXCLUDED.key_prefix,
            scopes = EXCLUDED.scopes,
            expires_at = EXCLUDED.expires_at,
            last_used_at = EXCLUDED.last_used_at,
            revoked_at = EXCLUDED.revoked_at;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.user_ssh_keys') IS NOT NULL THEN
        INSERT INTO ssh_keys (
            id,
            organization_id,
            user_id,
            name,
            public_key,
            fingerprint,
            key_type,
            created_at
        )
        SELECT
            legacy_stable_uuid('ssh-key:' || s.id::text || ':' || om.organization_id::text),
            om.organization_id,
            s.user_id,
            s.label,
            s.public_key,
            s.fingerprint,
            s.key_type,
            COALESCE(s.created_at, now())
        FROM legacy.user_ssh_keys s
        JOIN legacy_user_org_memberships om ON om.user_id = s.user_id
        ON CONFLICT (id) DO UPDATE
        SET name = EXCLUDED.name,
            public_key = EXCLUDED.public_key,
            fingerprint = EXCLUDED.fingerprint,
            key_type = EXCLUDED.key_type;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.git_credentials') IS NOT NULL THEN
        INSERT INTO git_credentials (
            id,
            organization_id,
            user_id,
            name,
            provider,
            credential_type,
            token_encrypted,
            token_nonce,
            remote_url,
            created_at,
            updated_at
        )
        SELECT
            legacy_stable_uuid('git-credential:' || g.id::text || ':' || om.organization_id::text),
            om.organization_id,
            g.user_id,
            COALESCE(NULLIF(g.host, ''), g.provider),
            g.provider,
            'token',
            convert_to(g.encrypted_token, 'UTF8'),
            NULL,
            g.host,
            COALESCE(g.created_at, now()),
            COALESCE(g.updated_at, g.created_at, now())
        FROM legacy.git_credentials g
        JOIN legacy_user_org_memberships om ON om.user_id = g.user_id
        ON CONFLICT (id) DO UPDATE
        SET name = EXCLUDED.name,
            provider = EXCLUDED.provider,
            credential_type = EXCLUDED.credential_type,
            token_encrypted = EXCLUDED.token_encrypted,
            token_nonce = EXCLUDED.token_nonce,
            remote_url = EXCLUDED.remote_url,
            updated_at = EXCLUDED.updated_at;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.llm_provider_configs') IS NOT NULL THEN
        INSERT INTO llm_provider_configs (
            id,
            organization_id,
            provider,
            model,
            api_key_encrypted,
            api_key_nonce,
            base_url,
            is_default,
            created_at,
            updated_at
        )
        SELECT
            c.id,
            c.org_id,
            c.provider,
            c.model,
            convert_to(c.encrypted_api_key, 'UTF8'),
            NULL,
            c.base_url,
            COALESCE(c.is_default, false),
            COALESCE(c.created_at, now()),
            COALESCE(c.updated_at, c.created_at, now())
        FROM legacy.llm_provider_configs c
        ON CONFLICT (id) DO UPDATE
        SET organization_id = EXCLUDED.organization_id,
            provider = EXCLUDED.provider,
            model = EXCLUDED.model,
            api_key_encrypted = EXCLUDED.api_key_encrypted,
            api_key_nonce = EXCLUDED.api_key_nonce,
            base_url = EXCLUDED.base_url,
            is_default = EXCLUDED.is_default,
            updated_at = EXCLUDED.updated_at;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.feature_flags') IS NOT NULL THEN
        INSERT INTO feature_flags (id, organization_id, name, enabled, metadata, created_at, updated_at)
        SELECT
            f.id,
            f.scope_id,
            f.key,
            COALESCE(f.enabled, false),
            jsonb_build_object('legacy_scope_type', f.scope_type),
            COALESCE(f.updated_at, now()),
            COALESCE(f.updated_at, now())
        FROM legacy.feature_flags f
        WHERE f.scope_type IN ('org', 'organization')
          AND f.scope_id IS NOT NULL
        ON CONFLICT (organization_id, name) DO UPDATE
        SET enabled = EXCLUDED.enabled,
            metadata = EXCLUDED.metadata,
            updated_at = EXCLUDED.updated_at;

        INSERT INTO feature_flags (id, organization_id, name, enabled, metadata, created_at, updated_at)
        SELECT
            f.id,
            NULL,
            f.key,
            COALESCE(f.enabled, false),
            jsonb_build_object('legacy_scope_type', f.scope_type),
            COALESCE(f.updated_at, now()),
            COALESCE(f.updated_at, now())
        FROM legacy.feature_flags f
        WHERE (f.scope_type NOT IN ('org', 'organization') OR f.scope_id IS NULL)
          AND NOT EXISTS (
              SELECT 1
              FROM feature_flags ff
              WHERE ff.organization_id IS NULL
                AND ff.name = f.key
          );
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.favorites') IS NOT NULL THEN
        INSERT INTO favorites (id, organization_id, user_id, target_type, target_id, created_at)
        SELECT
            CASE
                WHEN legacy_is_uuid(f.id) THEN f.id::uuid
                ELSE legacy_stable_uuid('favorite:' || f.id)
            END,
            f.org_id,
            f.user_id,
            f.type,
            CASE
                WHEN legacy_is_uuid(f.agent_id) THEN f.agent_id::uuid
                WHEN legacy_is_uuid(f.event_id) THEN f.event_id::uuid
                WHEN f.event_id ~ '^[0-9]+$' THEN legacy_stable_uuid('event:' || f.event_id)
                ELSE NULL
            END,
            COALESCE(f.created_at, now())
        FROM legacy.favorites f
        WHERE f.org_id IS NOT NULL
          AND f.user_id IS NOT NULL
          AND (
              legacy_is_uuid(f.agent_id)
              OR legacy_is_uuid(f.event_id)
              OR f.event_id ~ '^[0-9]+$'
          )
        ON CONFLICT (user_id, target_type, target_id) DO NOTHING;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.agent_collaborators') IS NOT NULL THEN
        INSERT INTO agent_collaborators (agent_id, user_id, permission, created_at)
        SELECT
            ac.agent_id,
            ac.user_id,
            CASE lower(ac.permission)
                WHEN 'manage' THEN 'admin'
                WHEN 'prompt' THEN 'edit'
                ELSE lower(ac.permission)
            END,
            COALESCE(ac.granted_at, now())
        FROM legacy.agent_collaborators ac
        ON CONFLICT (agent_id, user_id) DO UPDATE
        SET permission = EXCLUDED.permission,
            created_at = LEAST(agent_collaborators.created_at, EXCLUDED.created_at);
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.resource_profiles') IS NOT NULL THEN
        INSERT INTO resource_profiles (
            id,
            organization_id,
            name,
            cpu_millicores,
            memory_mb,
            storage_mb,
            max_pids,
            created_at,
            updated_at
        )
        SELECT
            rp.id,
            rp.org_id,
            rp.name,
            GREATEST(1, round(rp.cpu * 1000)::int),
            rp.memory_mb,
            1024,
            rp.pids_limit,
            rp.created_at,
            rp.updated_at
        FROM legacy.resource_profiles rp
        ON CONFLICT (id) DO UPDATE
        SET organization_id = EXCLUDED.organization_id,
            name = EXCLUDED.name,
            cpu_millicores = EXCLUDED.cpu_millicores,
            memory_mb = EXCLUDED.memory_mb,
            storage_mb = EXCLUDED.storage_mb,
            max_pids = EXCLUDED.max_pids,
            updated_at = EXCLUDED.updated_at;
    END IF;
END;
$$;

-- organizations.plan exists only on the legacy schema. 001_init.sql creates
-- the canonical organizations without it; canonical billing comes from the
-- subscriptions table directly. Skip on a fresh DB so #[sqlx::test] runs.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'organizations' AND column_name = 'plan'
    ) THEN
        EXECUTE $sql$
            INSERT INTO billing_plans (id, name, max_agents, max_events_per_day, max_storage_mb, features, created_at, updated_at)
            SELECT
                legacy_stable_uuid('billing-plan:' || lower(o.plan)),
                o.plan,
                CASE lower(o.plan)
                    WHEN 'free' THEN 3
                    WHEN 'pro' THEN 10
                    WHEN 'team' THEN 25
                    WHEN 'enterprise' THEN 100
                    ELSE 5
                END,
                CASE lower(o.plan)
                    WHEN 'free' THEN 1000
                    WHEN 'pro' THEN 10000
                    WHEN 'team' THEN 50000
                    WHEN 'enterprise' THEN 250000
                    ELSE 10000
                END,
                CASE lower(o.plan)
                    WHEN 'free' THEN 256
                    WHEN 'pro' THEN 1024
                    WHEN 'team' THEN 5120
                    WHEN 'enterprise' THEN 20480
                    ELSE 1024
                END,
                jsonb_build_object('legacy_import', true),
                now(),
                now()
            FROM (
                SELECT DISTINCT plan
                FROM organizations
                WHERE plan IS NOT NULL
            ) o
            ON CONFLICT (name) DO NOTHING
        $sql$;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.subscriptions') IS NOT NULL THEN
        INSERT INTO billing_plans (id, name, max_agents, max_events_per_day, max_storage_mb, features, created_at, updated_at)
        SELECT
            legacy_stable_uuid('billing-plan:' || lower(s.plan)),
            s.plan,
            CASE lower(s.plan)
                WHEN 'free' THEN 3
                WHEN 'pro' THEN 10
                WHEN 'team' THEN 25
                WHEN 'enterprise' THEN 100
                ELSE 5
            END,
            CASE lower(s.plan)
                WHEN 'free' THEN 1000
                WHEN 'pro' THEN 10000
                WHEN 'team' THEN 50000
                WHEN 'enterprise' THEN 250000
                ELSE 10000
            END,
            CASE lower(s.plan)
                WHEN 'free' THEN 256
                WHEN 'pro' THEN 1024
                WHEN 'team' THEN 5120
                WHEN 'enterprise' THEN 20480
                ELSE 1024
            END,
            jsonb_build_object('legacy_import', true),
            now(),
            now()
        FROM (
            SELECT DISTINCT plan
            FROM legacy.subscriptions
            WHERE plan IS NOT NULL
        ) s
        ON CONFLICT (name) DO NOTHING;

        INSERT INTO subscriptions (
            id,
            organization_id,
            plan_id,
            stripe_subscription_id,
            stripe_customer_id,
            status,
            current_period_start,
            current_period_end,
            canceled_at,
            created_at,
            updated_at
        )
        SELECT
            s.id,
            s.org_id,
            bp.id,
            s.stripe_subscription_id,
            o.stripe_customer_id,
            lower(s.status),
            s.current_period_start,
            s.current_period_end,
            s.cancel_at,
            COALESCE(s.created_at, now()),
            COALESCE(s.updated_at, s.created_at, now())
        FROM legacy.subscriptions s
        JOIN organizations o ON o.id = s.org_id
        JOIN billing_plans bp ON bp.name = s.plan
        ON CONFLICT (id) DO UPDATE
        SET organization_id = EXCLUDED.organization_id,
            plan_id = EXCLUDED.plan_id,
            stripe_subscription_id = EXCLUDED.stripe_subscription_id,
            stripe_customer_id = EXCLUDED.stripe_customer_id,
            status = EXCLUDED.status,
            current_period_start = EXCLUDED.current_period_start,
            current_period_end = EXCLUDED.current_period_end,
            canceled_at = EXCLUDED.canceled_at,
            updated_at = EXCLUDED.updated_at;
    END IF;
END;
$$;

-- organizations.plan + organizations.stripe_customer_id are legacy-only
-- columns. Same guard as the billing_plans block above — skip on fresh DB.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'organizations' AND column_name = 'plan'
    ) AND EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'organizations' AND column_name = 'stripe_customer_id'
    ) THEN
        EXECUTE $sql$
            INSERT INTO subscriptions (
                id,
                organization_id,
                plan_id,
                stripe_subscription_id,
                stripe_customer_id,
                status,
                current_period_start,
                current_period_end,
                canceled_at,
                created_at,
                updated_at
            )
            SELECT
                legacy_stable_uuid('subscription:' || o.id::text),
                o.id,
                bp.id,
                NULL,
                o.stripe_customer_id,
                'active',
                o.created_at,
                NULL,
                NULL,
                o.created_at,
                o.updated_at
            FROM organizations o
            JOIN billing_plans bp ON bp.name = o.plan
            WHERE NOT EXISTS (
                SELECT 1
                FROM subscriptions s
                WHERE s.organization_id = o.id
            )
        $sql$;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.licenses') IS NOT NULL THEN
        INSERT INTO licenses (
            id,
            organization_id,
            license_key,
            plan_name,
            max_agents,
            max_users,
            valid_from,
            valid_until,
            is_active,
            created_at,
            updated_at
        )
        SELECT
            l.id,
            l.org_id,
            COALESCE(l.metadata ->> 'license_key', 'legacy-' || l.id::text),
            l.plan_type,
            COALESCE(
                NULLIF(l.limits ->> 'max_agents', '')::int,
                NULLIF(l.limits ->> 'maxAgents', '')::int,
                5
            ),
            COALESCE(
                NULLIF(l.limits ->> 'max_users', '')::int,
                NULLIF(l.limits ->> 'maxUsers', '')::int,
                10
            ),
            l.issued_at,
            l.expires_at,
            l.revoked_at IS NULL
                AND lower(l.status) IN ('active', 'valid')
                AND (l.expires_at IS NULL OR l.expires_at > now()),
            l.created_at,
            l.updated_at
        FROM legacy.licenses l
        ON CONFLICT (license_key) DO UPDATE
        SET organization_id = EXCLUDED.organization_id,
            plan_name = EXCLUDED.plan_name,
            max_agents = EXCLUDED.max_agents,
            max_users = EXCLUDED.max_users,
            valid_from = EXCLUDED.valid_from,
            valid_until = EXCLUDED.valid_until,
            is_active = EXCLUDED.is_active,
            updated_at = EXCLUDED.updated_at;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.voice_provider_configs') IS NOT NULL THEN
        INSERT INTO voice_providers (
            id,
            organization_id,
            name,
            provider_type,
            config,
            is_default,
            created_at,
            updated_at
        )
        SELECT
            v.id,
            v.org_id,
            COALESCE(v.display_name, v.provider),
            v.provider,
            COALESCE(v.config_json, '{}'::jsonb)
                || jsonb_strip_nulls(
                    jsonb_build_object(
                        'legacy_model', v.model,
                        'legacy_language', v.language,
                        'legacy_api_key_enc', v.api_key_enc,
                        'legacy_enabled', v.is_enabled
                    )
                ),
            COALESCE(v.is_default, false),
            COALESCE(v.created_at, now()),
            COALESCE(v.updated_at, v.created_at, now())
        FROM legacy.voice_provider_configs v
        ON CONFLICT (id) DO UPDATE
        SET organization_id = EXCLUDED.organization_id,
            name = EXCLUDED.name,
            provider_type = EXCLUDED.provider_type,
            config = EXCLUDED.config,
            is_default = EXCLUDED.is_default,
            updated_at = EXCLUDED.updated_at;
    END IF;
END;
$$;

DO $$
BEGIN
    IF to_regclass('legacy.participants') IS NOT NULL THEN
        INSERT INTO participants (
            id,
            organization_id,
            agent_id,
            name,
            capabilities,
            status,
            registered_at,
            last_heartbeat_at
        )
        SELECT
            p.id,
            a.organization_id,
            a.id,
            p.name,
            COALESCE(
                ARRAY(
                    SELECT jsonb_array_elements_text(COALESCE(p.skills, '[]'::jsonb))
                ),
                ARRAY[]::text[]
            ),
            CASE lower(COALESCE(p.status, 'offline'))
                WHEN 'available' THEN 'available'
                WHEN 'busy' THEN 'busy'
                WHEN 'online' THEN 'available'
                ELSE 'offline'
            END,
            COALESCE(p.created_at, now()),
            p.last_heartbeat
        FROM legacy.participants p
        JOIN agents a
          ON legacy_is_uuid(p.agent_id)
         AND a.id = p.agent_id::uuid
        ON CONFLICT (organization_id, agent_id) DO UPDATE
        SET name = EXCLUDED.name,
            capabilities = EXCLUDED.capabilities,
            status = EXCLUDED.status,
            last_heartbeat_at = EXCLUDED.last_heartbeat_at;
    END IF;
END;
$$;
