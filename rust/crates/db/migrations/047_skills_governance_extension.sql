-- Unit 2.2: governed skills extension.
--
-- Keep the legacy skills table and API shape intact while adding the
-- governance axes required by the context platform. Existing organization
-- skills become org-scoped active skills. Legacy global rows
-- (organization_id IS NULL) remain readable and nullable on governance fields.

ALTER TABLE skills
    ADD COLUMN IF NOT EXISTS workspace_id UUID,
    ADD COLUMN IF NOT EXISTS scope_kind TEXT,
    ADD COLUMN IF NOT EXISTS scope_id UUID,
    ADD COLUMN IF NOT EXISTS state TEXT NOT NULL DEFAULT 'active',
    ADD COLUMN IF NOT EXISTS version INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS owner_user_id UUID,
    ADD COLUMN IF NOT EXISTS ttl_expires_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS sensitivity TEXT NOT NULL DEFAULT 'internal',
    ADD COLUMN IF NOT EXISTS provenance JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS negative_trigger TEXT,
    ADD COLUMN IF NOT EXISTS required_inputs JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS tools JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS examples JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS success_evidence JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ;

ALTER TABLE skills
    ALTER COLUMN state SET DEFAULT 'active',
    ALTER COLUMN version SET DEFAULT 1,
    ALTER COLUMN sensitivity SET DEFAULT 'internal',
    ALTER COLUMN provenance SET DEFAULT '{}'::jsonb,
    ALTER COLUMN required_inputs SET DEFAULT '[]'::jsonb,
    ALTER COLUMN tools SET DEFAULT '[]'::jsonb,
    ALTER COLUMN examples SET DEFAULT '[]'::jsonb,
    ALTER COLUMN success_evidence SET DEFAULT '[]'::jsonb;

INSERT INTO workspaces (organization_id, name)
SELECT DISTINCT s.organization_id, 'Default Workspace'
  FROM skills s
 WHERE s.organization_id IS NOT NULL
   AND NOT EXISTS (
       SELECT 1
         FROM workspaces w
        WHERE w.organization_id = s.organization_id
          AND w.deleted_at IS NULL
   );

WITH workspace_fallback AS (
    SELECT DISTINCT ON (organization_id)
           organization_id,
           id
      FROM workspaces
     WHERE deleted_at IS NULL
     ORDER BY organization_id, created_at ASC, id ASC
)
UPDATE skills s
   SET workspace_id = workspace_fallback.id
  FROM workspace_fallback
WHERE s.organization_id = workspace_fallback.organization_id
   AND (
       s.workspace_id IS NULL
       OR NOT EXISTS (
           SELECT 1
             FROM workspaces w
            WHERE w.id = s.workspace_id
              AND w.organization_id = s.organization_id
              AND w.deleted_at IS NULL
       )
   );

UPDATE skills
   SET scope_kind = 'org',
       scope_id = organization_id
 WHERE organization_id IS NOT NULL
   AND scope_kind IS NULL;

WITH owner_fallback AS (
    SELECT DISTINCT ON (organization_id)
           organization_id,
           user_id
      FROM organization_members
     ORDER BY organization_id,
              CASE role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END,
              created_at ASC
)
UPDATE skills s
   SET owner_user_id = owner_fallback.user_id
  FROM owner_fallback
WHERE s.organization_id = owner_fallback.organization_id
   AND s.owner_user_id IS NULL;

UPDATE skills
   SET state = CASE
           WHEN state IN ('candidate', 'active', 'deprecated', 'revoked') THEN state
           ELSE 'active'
       END,
       version = CASE WHEN version IS NULL OR version < 1 THEN 1 ELSE version END,
       sensitivity = CASE
           WHEN sensitivity IN ('public', 'internal', 'confidential', 'secret_detected') THEN sensitivity
           ELSE 'internal'
       END,
       provenance = CASE
           WHEN COALESCE(jsonb_typeof(provenance), '') = 'object' THEN provenance
           ELSE '{}'::jsonb
       END,
       required_inputs = CASE
           WHEN COALESCE(jsonb_typeof(required_inputs), '') = 'array' THEN required_inputs
           ELSE '[]'::jsonb
       END,
       tools = CASE
           WHEN COALESCE(jsonb_typeof(tools), '') = 'array' THEN tools
           ELSE '[]'::jsonb
       END,
       examples = CASE
           WHEN COALESCE(jsonb_typeof(examples), '') = 'array' THEN examples
           ELSE '[]'::jsonb
       END,
       success_evidence = CASE
           WHEN COALESCE(jsonb_typeof(success_evidence), '') = 'array' THEN success_evidence
           ELSE '[]'::jsonb
       END;

UPDATE skills
   SET state = 'deprecated'
 WHERE enabled = FALSE
   AND state = 'active'
   AND revoked_at IS NULL;

UPDATE skills
   SET state = 'revoked'
 WHERE revoked_at IS NOT NULL
   AND state <> 'revoked';

UPDATE skills
   SET enabled = FALSE
 WHERE state = 'revoked';

UPDATE skills
   SET revoked_at = now()
 WHERE state = 'revoked'
   AND revoked_at IS NULL;

ALTER TABLE skills
    ALTER COLUMN state SET NOT NULL,
    ALTER COLUMN version SET NOT NULL,
    ALTER COLUMN sensitivity SET NOT NULL,
    ALTER COLUMN provenance SET NOT NULL,
    ALTER COLUMN required_inputs SET NOT NULL,
    ALTER COLUMN tools SET NOT NULL,
    ALTER COLUMN examples SET NOT NULL,
    ALTER COLUMN success_evidence SET NOT NULL;

ALTER TABLE skills
    DROP CONSTRAINT IF EXISTS skills_scope_kind_check,
    DROP CONSTRAINT IF EXISTS skills_state_check,
    DROP CONSTRAINT IF EXISTS skills_version_check,
    DROP CONSTRAINT IF EXISTS skills_sensitivity_check,
    DROP CONSTRAINT IF EXISTS skills_provenance_object_check,
    DROP CONSTRAINT IF EXISTS skills_required_inputs_array_check,
    DROP CONSTRAINT IF EXISTS skills_tools_array_check,
    DROP CONSTRAINT IF EXISTS skills_examples_array_check,
    DROP CONSTRAINT IF EXISTS skills_success_evidence_array_check,
    DROP CONSTRAINT IF EXISTS skills_governed_scope_check,
    DROP CONSTRAINT IF EXISTS skills_revoked_state_check;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skills_owner_user_id_fkey'
          AND conrelid = 'skills'::regclass
    ) THEN
        ALTER TABLE skills
            ADD CONSTRAINT skills_owner_user_id_fkey
            FOREIGN KEY (owner_user_id) REFERENCES users(id) ON DELETE SET NULL NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skills_workspace_id_fkey'
          AND conrelid = 'skills'::regclass
    ) THEN
        ALTER TABLE skills
            ADD CONSTRAINT skills_workspace_id_fkey
            FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skills_scope_kind_check'
          AND conrelid = 'skills'::regclass
    ) THEN
        ALTER TABLE skills
            ADD CONSTRAINT skills_scope_kind_check
            CHECK (scope_kind IS NULL OR scope_kind IN ('org', 'user', 'team', 'project')) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skills_state_check'
          AND conrelid = 'skills'::regclass
    ) THEN
        ALTER TABLE skills
            ADD CONSTRAINT skills_state_check
            CHECK (state IN ('candidate', 'active', 'deprecated', 'revoked')) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skills_version_check'
          AND conrelid = 'skills'::regclass
    ) THEN
        ALTER TABLE skills
            ADD CONSTRAINT skills_version_check
            CHECK (version >= 1) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skills_sensitivity_check'
          AND conrelid = 'skills'::regclass
    ) THEN
        ALTER TABLE skills
            ADD CONSTRAINT skills_sensitivity_check
            CHECK (sensitivity IN ('public', 'internal', 'confidential', 'secret_detected')) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skills_provenance_object_check'
          AND conrelid = 'skills'::regclass
    ) THEN
        ALTER TABLE skills
            ADD CONSTRAINT skills_provenance_object_check
            CHECK (jsonb_typeof(provenance) = 'object') NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skills_required_inputs_array_check'
          AND conrelid = 'skills'::regclass
    ) THEN
        ALTER TABLE skills
            ADD CONSTRAINT skills_required_inputs_array_check
            CHECK (jsonb_typeof(required_inputs) = 'array') NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skills_tools_array_check'
          AND conrelid = 'skills'::regclass
    ) THEN
        ALTER TABLE skills
            ADD CONSTRAINT skills_tools_array_check
            CHECK (jsonb_typeof(tools) = 'array') NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skills_examples_array_check'
          AND conrelid = 'skills'::regclass
    ) THEN
        ALTER TABLE skills
            ADD CONSTRAINT skills_examples_array_check
            CHECK (jsonb_typeof(examples) = 'array') NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skills_success_evidence_array_check'
          AND conrelid = 'skills'::regclass
    ) THEN
        ALTER TABLE skills
            ADD CONSTRAINT skills_success_evidence_array_check
            CHECK (jsonb_typeof(success_evidence) = 'array') NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skills_governed_scope_check'
          AND conrelid = 'skills'::regclass
    ) THEN
        ALTER TABLE skills
            ADD CONSTRAINT skills_governed_scope_check
            CHECK (
                organization_id IS NULL
                OR (
                    workspace_id IS NOT NULL
                    AND scope_kind IS NOT NULL
                    AND scope_id IS NOT NULL
                    AND (
                        (scope_kind = 'org' AND scope_id = organization_id)
                        OR scope_kind IN ('user', 'team', 'project')
                    )
                )
            ) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skills_revoked_state_check'
          AND conrelid = 'skills'::regclass
    ) THEN
        ALTER TABLE skills
            ADD CONSTRAINT skills_revoked_state_check
            CHECK (
                (state = 'revoked' AND revoked_at IS NOT NULL AND enabled = FALSE)
                OR (state <> 'revoked' AND revoked_at IS NULL)
            ) NOT VALID;
    END IF;
END $$;

DROP INDEX IF EXISTS idx_skills_org_scope_state;
DROP INDEX IF EXISTS idx_skills_active_name;
DROP INDEX IF EXISTS idx_skills_trigger_pattern;

CREATE INDEX IF NOT EXISTS idx_skills_org_scope_state
    ON skills(organization_id, workspace_id, scope_kind, scope_id, state);

CREATE INDEX IF NOT EXISTS idx_skills_active_name
    ON skills(organization_id, workspace_id, name)
    WHERE enabled = TRUE AND state = 'active' AND revoked_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_skills_trigger_pattern
    ON skills(organization_id, workspace_id, trigger_pattern)
    WHERE trigger_pattern IS NOT NULL AND enabled = TRUE AND state = 'active' AND revoked_at IS NULL;

ALTER TABLE skills VALIDATE CONSTRAINT skills_owner_user_id_fkey;
ALTER TABLE skills VALIDATE CONSTRAINT skills_workspace_id_fkey;
ALTER TABLE skills VALIDATE CONSTRAINT skills_scope_kind_check;
ALTER TABLE skills VALIDATE CONSTRAINT skills_state_check;
ALTER TABLE skills VALIDATE CONSTRAINT skills_version_check;
ALTER TABLE skills VALIDATE CONSTRAINT skills_sensitivity_check;
ALTER TABLE skills VALIDATE CONSTRAINT skills_provenance_object_check;
ALTER TABLE skills VALIDATE CONSTRAINT skills_required_inputs_array_check;
ALTER TABLE skills VALIDATE CONSTRAINT skills_tools_array_check;
ALTER TABLE skills VALIDATE CONSTRAINT skills_examples_array_check;
ALTER TABLE skills VALIDATE CONSTRAINT skills_success_evidence_array_check;
ALTER TABLE skills VALIDATE CONSTRAINT skills_governed_scope_check;
ALTER TABLE skills VALIDATE CONSTRAINT skills_revoked_state_check;
