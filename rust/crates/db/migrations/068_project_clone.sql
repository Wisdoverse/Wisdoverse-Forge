-- 068: project git-clone foundation (M0 — schema only, no business logic).
--
-- A project may be created with an optional git repository that the platform
-- clones into the project's workspace directory. This migration lands only the
-- database foundation:
--   * projects.workspace_dir_name — the on-disk directory name under the
--     workspace projects root (backfilled from the existing slug).
--   * projects.clone_status — coarse lifecycle marker for the project's clone.
--   * project_clone_attempts — one row per clone attempt (retry-aware), holding
--     the runtime/worker/lease/error detail a later worker + reconciler own.
--   * job_queue unique-key index — the queue's `enqueue` ON CONFLICT path
--     already assumes a UNIQUE index on unique_key; it did not exist yet.
--
-- Idempotent throughout (IF NOT EXISTS / guarded DO-blocks) so it tolerates
-- pre-existing production drift and re-runs cleanly.

-- ---------------------------------------------------------------------------
-- projects: workspace_dir_name + clone_status
-- ---------------------------------------------------------------------------

ALTER TABLE projects ADD COLUMN IF NOT EXISTS workspace_dir_name TEXT;

-- Backfill from the canonical slug (NOT NULL since migration 026) so the
-- SET NOT NULL below cannot fail on existing rows.
UPDATE projects SET workspace_dir_name = slug WHERE workspace_dir_name IS NULL;

ALTER TABLE projects ALTER COLUMN workspace_dir_name SET NOT NULL;

ALTER TABLE projects ADD COLUMN IF NOT EXISTS clone_status TEXT NOT NULL DEFAULT 'none';

-- Enum CHECK on clone_status, guarded so re-running this migration is safe
-- (ADD CONSTRAINT has no IF NOT EXISTS in supported Postgres versions).
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE table_schema = 'public'
          AND table_name = 'projects'
          AND constraint_name = 'projects_clone_status_check'
    ) THEN
        ALTER TABLE projects
            ADD CONSTRAINT projects_clone_status_check
            CHECK (clone_status IN ('none', 'queued', 'cloning', 'ready', 'failed'));
    END IF;
END $$;

-- One live (non-deleted) directory name per workspace. Partial so soft-deleted
-- projects do not block reuse of a freed directory name.
CREATE UNIQUE INDEX IF NOT EXISTS uq_projects_workspace_dir
    ON projects(workspace_id, workspace_dir_name)
    WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- project_clone_attempts: one row per clone attempt
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS project_clone_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    project_id UUID NOT NULL REFERENCES projects(id),
    attempt INT NOT NULL,
    repository_url TEXT NOT NULL,
    provider TEXT,
    credential_id UUID,
    status TEXT NOT NULL CHECK (status IN ('queued', 'cloning', 'ready', 'failed', 'cancelled')),
    resolved_branch TEXT,
    head_sha TEXT,
    container_id TEXT,
    worker_id TEXT,
    job_id UUID,
    lease_expires_at TIMESTAMPTZ,
    error_class TEXT,
    error_message TEXT,
    bytes_cloned BIGINT,
    duration_ms BIGINT,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One attempt number per project (retry increments `attempt`).
CREATE UNIQUE INDEX IF NOT EXISTS uq_project_clone_attempt
    ON project_clone_attempts(project_id, attempt);

-- Drives the worker's claim/lease-recovery scans.
CREATE INDEX IF NOT EXISTS idx_project_clone_status
    ON project_clone_attempts(status, lease_expires_at);

-- updated_at maintenance, matching the convention in 002_credentials.sql.
DROP TRIGGER IF EXISTS project_clone_attempts_updated_at ON project_clone_attempts;
CREATE TRIGGER project_clone_attempts_updated_at
    BEFORE UPDATE ON project_clone_attempts
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ---------------------------------------------------------------------------
-- job_queue: the unique-key index the queue's ON CONFLICT path assumes
-- ---------------------------------------------------------------------------

CREATE UNIQUE INDEX IF NOT EXISTS idx_job_queue_unique_key
    ON job_queue(unique_key)
    WHERE unique_key IS NOT NULL;
