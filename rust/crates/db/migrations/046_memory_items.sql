-- Unit 2.1: governed memory items.
--
-- Memory items are API-owned governed context assets. Content is stored for
-- later scoped reads but is intentionally fetched through a separate endpoint
-- and never included in default entity serialization.

CREATE TABLE IF NOT EXISTS memory_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    scope_kind TEXT NOT NULL,
    scope_id UUID NOT NULL,
    source_task_id UUID REFERENCES orchestration_tasks(id) ON DELETE SET NULL,
    source_run_id UUID REFERENCES task_runs(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    content_redacted BOOLEAN NOT NULL DEFAULT FALSE,
    content_encrypted BOOLEAN NOT NULL DEFAULT FALSE,
    visibility TEXT NOT NULL DEFAULT 'shared',
    sensitivity TEXT NOT NULL DEFAULT 'internal',
    provenance JSONB NOT NULL DEFAULT '{}'::jsonb,
    ttl_expires_at TIMESTAMPTZ,
    confidence DOUBLE PRECISION,
    last_used_at TIMESTAMPTZ,
    last_verified_at TIMESTAMPTZ,
    state TEXT NOT NULL DEFAULT 'active',
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT memory_items_scope_kind_check CHECK (scope_kind IN ('user', 'team', 'project')),
    CONSTRAINT memory_items_visibility_check CHECK (visibility IN ('private', 'shared')),
    CONSTRAINT memory_items_sensitivity_check CHECK (sensitivity IN ('public', 'internal', 'confidential', 'secret_detected')),
    CONSTRAINT memory_items_state_check CHECK (state IN ('candidate', 'pending', 'active', 'needs_review', 'revoked')),
    CONSTRAINT memory_items_confidence_check CHECK (confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
    CONSTRAINT memory_items_revoked_state_check CHECK (
        (state = 'revoked' AND revoked_at IS NOT NULL)
        OR (state <> 'revoked' AND revoked_at IS NULL)
    )
);

ALTER TABLE memory_items
    ADD COLUMN IF NOT EXISTS organization_id UUID,
    ADD COLUMN IF NOT EXISTS workspace_id UUID,
    ADD COLUMN IF NOT EXISTS owner_user_id UUID,
    ADD COLUMN IF NOT EXISTS scope_kind TEXT,
    ADD COLUMN IF NOT EXISTS scope_id UUID,
    ADD COLUMN IF NOT EXISTS source_task_id UUID,
    ADD COLUMN IF NOT EXISTS source_run_id UUID,
    ADD COLUMN IF NOT EXISTS title TEXT,
    ADD COLUMN IF NOT EXISTS content TEXT,
    ADD COLUMN IF NOT EXISTS content_redacted BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS content_encrypted BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS visibility TEXT NOT NULL DEFAULT 'shared',
    ADD COLUMN IF NOT EXISTS sensitivity TEXT NOT NULL DEFAULT 'internal',
    ADD COLUMN IF NOT EXISTS provenance JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS ttl_expires_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS confidence DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS last_used_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_verified_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS state TEXT NOT NULL DEFAULT 'active',
    ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

ALTER TABLE memory_items
    ALTER COLUMN id SET DEFAULT gen_random_uuid(),
    ALTER COLUMN id SET NOT NULL,
    ALTER COLUMN organization_id SET NOT NULL,
    ALTER COLUMN workspace_id SET NOT NULL,
    ALTER COLUMN owner_user_id SET NOT NULL,
    ALTER COLUMN scope_kind SET NOT NULL,
    ALTER COLUMN scope_id SET NOT NULL,
    ALTER COLUMN title SET NOT NULL,
    ALTER COLUMN content SET NOT NULL,
    ALTER COLUMN content_redacted SET NOT NULL,
    ALTER COLUMN content_encrypted SET NOT NULL,
    ALTER COLUMN visibility SET NOT NULL,
    ALTER COLUMN sensitivity SET NOT NULL,
    ALTER COLUMN provenance SET NOT NULL,
    ALTER COLUMN state SET NOT NULL,
    ALTER COLUMN created_at SET NOT NULL,
    ALTER COLUMN updated_at SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_items_organization_id_fkey'
          AND conrelid = 'memory_items'::regclass
    ) THEN
        ALTER TABLE memory_items
            ADD CONSTRAINT memory_items_organization_id_fkey
            FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_items_workspace_id_fkey'
          AND conrelid = 'memory_items'::regclass
    ) THEN
        ALTER TABLE memory_items
            ADD CONSTRAINT memory_items_workspace_id_fkey
            FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_items_owner_user_id_fkey'
          AND conrelid = 'memory_items'::regclass
    ) THEN
        ALTER TABLE memory_items
            ADD CONSTRAINT memory_items_owner_user_id_fkey
            FOREIGN KEY (owner_user_id) REFERENCES users(id) ON DELETE RESTRICT NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_items_source_task_id_fkey'
          AND conrelid = 'memory_items'::regclass
    ) THEN
        ALTER TABLE memory_items
            ADD CONSTRAINT memory_items_source_task_id_fkey
            FOREIGN KEY (source_task_id) REFERENCES orchestration_tasks(id) ON DELETE SET NULL NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_items_source_run_id_fkey'
          AND conrelid = 'memory_items'::regclass
    ) THEN
        ALTER TABLE memory_items
            ADD CONSTRAINT memory_items_source_run_id_fkey
            FOREIGN KEY (source_run_id) REFERENCES task_runs(id) ON DELETE SET NULL NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_items_scope_kind_check'
          AND conrelid = 'memory_items'::regclass
    ) THEN
        ALTER TABLE memory_items
            ADD CONSTRAINT memory_items_scope_kind_check
            CHECK (scope_kind IN ('user', 'team', 'project')) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_items_visibility_check'
          AND conrelid = 'memory_items'::regclass
    ) THEN
        ALTER TABLE memory_items
            ADD CONSTRAINT memory_items_visibility_check
            CHECK (visibility IN ('private', 'shared')) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_items_sensitivity_check'
          AND conrelid = 'memory_items'::regclass
    ) THEN
        ALTER TABLE memory_items
            ADD CONSTRAINT memory_items_sensitivity_check
            CHECK (sensitivity IN ('public', 'internal', 'confidential', 'secret_detected')) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_items_state_check'
          AND conrelid = 'memory_items'::regclass
    ) THEN
        ALTER TABLE memory_items
            ADD CONSTRAINT memory_items_state_check
            CHECK (state IN ('candidate', 'pending', 'active', 'needs_review', 'revoked')) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_items_confidence_check'
          AND conrelid = 'memory_items'::regclass
    ) THEN
        ALTER TABLE memory_items
            ADD CONSTRAINT memory_items_confidence_check
            CHECK (confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'memory_items_revoked_state_check'
          AND conrelid = 'memory_items'::regclass
    ) THEN
        ALTER TABLE memory_items
            ADD CONSTRAINT memory_items_revoked_state_check
            CHECK (
                (state = 'revoked' AND revoked_at IS NOT NULL)
                OR (state <> 'revoked' AND revoked_at IS NULL)
            ) NOT VALID;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_memory_items_org_workspace_scope
    ON memory_items(organization_id, workspace_id, scope_kind, scope_id);

CREATE INDEX IF NOT EXISTS idx_memory_items_active
    ON memory_items(organization_id, workspace_id, updated_at DESC, id DESC)
    WHERE revoked_at IS NULL AND state = 'active';

CREATE INDEX IF NOT EXISTS idx_memory_items_ttl
    ON memory_items(ttl_expires_at)
    WHERE ttl_expires_at IS NOT NULL AND revoked_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_memory_items_fts
    ON memory_items USING GIN (to_tsvector('simple', coalesce(title, '') || ' ' || coalesce(content, '')));

DROP TRIGGER IF EXISTS memory_items_updated_at ON memory_items;
CREATE TRIGGER memory_items_updated_at
    BEFORE UPDATE ON memory_items
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

ALTER TABLE memory_items VALIDATE CONSTRAINT memory_items_organization_id_fkey;
ALTER TABLE memory_items VALIDATE CONSTRAINT memory_items_workspace_id_fkey;
ALTER TABLE memory_items VALIDATE CONSTRAINT memory_items_owner_user_id_fkey;
ALTER TABLE memory_items VALIDATE CONSTRAINT memory_items_source_task_id_fkey;
ALTER TABLE memory_items VALIDATE CONSTRAINT memory_items_source_run_id_fkey;
ALTER TABLE memory_items VALIDATE CONSTRAINT memory_items_scope_kind_check;
ALTER TABLE memory_items VALIDATE CONSTRAINT memory_items_visibility_check;
ALTER TABLE memory_items VALIDATE CONSTRAINT memory_items_sensitivity_check;
ALTER TABLE memory_items VALIDATE CONSTRAINT memory_items_state_check;
ALTER TABLE memory_items VALIDATE CONSTRAINT memory_items_confidence_check;
ALTER TABLE memory_items VALIDATE CONSTRAINT memory_items_revoked_state_check;
