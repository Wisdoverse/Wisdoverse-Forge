-- Unit 2.3: append-only skill version snapshots for deterministic rollback.

CREATE TABLE IF NOT EXISTS skill_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    skill_id UUID NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    snapshot JSONB NOT NULL,
    author_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skill_versions_version_check'
          AND conrelid = 'skill_versions'::regclass
    ) THEN
        ALTER TABLE skill_versions
            ADD CONSTRAINT skill_versions_version_check
            CHECK (version >= 1) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'skill_versions_snapshot_object_check'
          AND conrelid = 'skill_versions'::regclass
    ) THEN
        ALTER TABLE skill_versions
            ADD CONSTRAINT skill_versions_snapshot_object_check
            CHECK (jsonb_typeof(snapshot) = 'object') NOT VALID;
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_skill_versions_skill_version
    ON skill_versions(skill_id, version);

CREATE INDEX IF NOT EXISTS idx_skill_versions_skill_created_at
    ON skill_versions(skill_id, created_at DESC);

ALTER TABLE skill_versions VALIDATE CONSTRAINT skill_versions_version_check;
ALTER TABLE skill_versions VALIDATE CONSTRAINT skill_versions_snapshot_object_check;
