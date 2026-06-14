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

-- Backfill from the canonical slug (NOT NULL since migration 026). Backfill
-- ALL rows — including soft-deleted ones — so the SET NOT NULL below holds
-- for every row, not just live ones. `WHERE workspace_dir_name IS NULL` keeps
-- this idempotent: a replay against already-backfilled rows is a no-op and
-- never overwrites a name the create-path service has since allocated.
UPDATE projects SET workspace_dir_name = slug WHERE workspace_dir_name IS NULL;

-- Collision-aware dedup for the partial unique index below.
--
-- The pre-existing slug uniqueness is only `(team_id, slug)` (migration 023),
-- and teams are org-scoped (no workspace_id), so two LIVE projects in the same
-- workspace but different teams can legitimately share a slug today. Backfilling
-- workspace_dir_name = slug therefore produces duplicate
-- `(workspace_id, workspace_dir_name)` pairs among live rows, which would make
-- the `WHERE deleted_at IS NULL` unique index build FAIL and abort the
-- migration on first run against real data.
--
-- Mirror migration 026's collision-aware approach: among live
-- (`deleted_at IS NULL`) rows that share `(workspace_id, workspace_dir_name)`,
-- keep the deterministically-oldest row (ORDER BY created_at, id) as-is and
-- suffix the rest with their full UUID. A full UUID suffix is collision-free by
-- construction:
--   * It cannot collide with another suffixed row — UUIDs are unique, so
--     `slug || '-' || id` differs for every id.
--   * It is astronomically unlikely to collide with an un-suffixed bare slug;
--     the WHILE loop below proves it to zero rather than assuming it.
--
-- Idempotent: on a re-run the rows are already distinct, so the window finds no
-- duplicates and the UPDATE touches zero rows.
DO $$
DECLARE
    collisions bigint;
    guard      int := 0;
BEGIN
    LOOP
        -- Suffix every live row that is not the kept representative of its
        -- (workspace_id, workspace_dir_name) group. Re-derive groups each pass
        -- so a suffix that (impossibly) reintroduced a clash is healed too.
        WITH ranked AS (
            SELECT id,
                   row_number() OVER (
                       PARTITION BY workspace_id, workspace_dir_name
                       ORDER BY created_at ASC, id ASC
                   ) AS rn
              FROM projects
             WHERE deleted_at IS NULL
        )
        UPDATE projects p
           SET workspace_dir_name = p.workspace_dir_name || '-' || p.id::text
          FROM ranked
         WHERE p.id = ranked.id
           AND ranked.rn > 1;

        GET DIAGNOSTICS collisions = ROW_COUNT;
        EXIT WHEN collisions = 0;

        guard := guard + 1;
        IF guard > 5 THEN
            RAISE EXCEPTION
                'migration 068: workspace_dir_name dedup did not converge after % passes', guard;
        END IF;
    END LOOP;
END $$;

ALTER TABLE projects ALTER COLUMN workspace_dir_name SET NOT NULL;

-- Make the NOT NULL invariant unbreakable for FUTURE inserts. The create-path
-- service (`ProjectRepository::create_with_clone`) always sets a derived,
-- collision-resolved `workspace_dir_name` explicitly, but any INSERT that omits
-- the column (a future code path, or a bare test seed) now gets a unique,
-- filesystem-safe value rather than a NULL that would violate the NOT NULL above.
--
-- The default is a GREPPABLE SENTINEL — `unallocated-<uuid>` — NOT a bare
-- `gen_random_uuid()`. A bare uuid is indistinguishable from a real allocated
-- name, so a create path that silently FORGOT to set the dir name would be
-- invisible. The `unallocated-` prefix makes that bug alarmable/greppable: any
-- live `workspace_dir_name LIKE 'unallocated-%'` is a defect (a write that did
-- not go through the product path), not an expected value. `unallocated-` + a
-- 36-char uuid is 48 chars (<= the 64-char `WorkspaceDirName` cap), all
-- `[a-z0-9-]`, no leading/trailing dash, so it still satisfies
-- `WorkspaceDirName::parse` / `is_safe_dir_name`, and the uuid keeps it unique by
-- construction so it can never collide on the `(workspace_id, …)` unique index.
--
-- This sentinel guards `workspace_dir_name` specifically. The `slug` column gets
-- NO parallel default (out of scope here); the create path supplies both columns
-- on every product write, and `slug`'s NOT NULL predates this migration.
--
-- Applied as a separate `SET DEFAULT` *after* the backfill so it only governs
-- new rows and keeps the idempotent backfill above untouched. This is a
-- fallback, not the product path: real projects carry the name-derived
-- directory the service allocates.
ALTER TABLE projects ALTER COLUMN workspace_dir_name SET DEFAULT ('unallocated-' || gen_random_uuid()::text);

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
-- projects do not block reuse of a freed directory name. The dedup above
-- guarantees this build succeeds on any pre-existing data.
CREATE UNIQUE INDEX IF NOT EXISTS uq_projects_workspace_dir
    ON projects(workspace_id, workspace_dir_name)
    WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- project_clone_attempts: one row per clone attempt
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS project_clone_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    workspace_id    UUID NOT NULL REFERENCES workspaces(id),
    -- RESTRICT by design: cancel an attempt before hard-deleting a project
    -- (projects are soft-deleted, so this also matches the soft-delete model).
    project_id      UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    attempt INT NOT NULL,
    repository_url TEXT NOT NULL,
    provider TEXT,
    credential_id UUID,
    status TEXT NOT NULL
        CONSTRAINT project_clone_attempts_status_check
        CHECK (status IN ('queued', 'cloning', 'ready', 'failed', 'cancelled')),
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
