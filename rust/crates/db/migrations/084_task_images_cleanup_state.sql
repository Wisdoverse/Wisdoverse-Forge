-- 084: per-task state for the instruction-image cleanup sweeper.
--
-- `task_images_workspace_id` records the workspace the images were materialized
-- into at dispatch, so cleanup does NOT depend on the (possibly hard-deleted)
-- assigned agent row — after an agent delete the FK nulls `assigned_agent_id`
-- but the workspace projects dir still holds the images.
--
-- `task_images_cleaned_at` is the sweeper's PERMANENT done marker. The sweeper sets
-- it only AFTER removal is confirmed, inside the SAME transaction that holds a
-- `FOR UPDATE` lock on the task row, so the bounded SELECT makes progress instead of
-- re-returning already-removed rows under the row cap. Because removal + marking
-- live in one row-locked transaction, the sweeper and the dispatch path (which holds
-- the same row lock from `assign_agent_in_tx` through commit, before it materializes
-- any images) are mutually exclusive: a retried task is either skipped via
-- `SKIP LOCKED` / the age re-check, or its fresh images are materialized only after
-- the sweeper has finished. A crash mid-removal just rolls the transaction back, so
-- the row stays un-marked and is retried on the next tick.
--
-- `task_images_retry_after` is a backoff stamp for a directory the sweeper REFUSES
-- to fully remove (a symlinked component or a planted sub-directory). Without it,
-- such a row keeps its old `updated_at` and `cleaned_at IS NULL` and would re-fill
-- the oldest-first `LIMIT 500` scan every tick, starving newer eligible tasks; with
-- it, the row is skipped until the backoff expires so the scan pages past it.
--
-- All three reset to NULL when images are (re-)materialized so a retried task's new
-- images become eligible again.

ALTER TABLE orchestration_tasks
    ADD COLUMN IF NOT EXISTS task_images_workspace_id UUID,
    ADD COLUMN IF NOT EXISTS task_images_cleaned_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS task_images_retry_after TIMESTAMPTZ;

-- Recreate the `updated_at` trigger with a guard so writes that touch ONLY the
-- cleanup columns do NOT bump `updated_at`. Cleanup is internal bookkeeping: bumping
-- `updated_at` would surface a swept task as freshly "updated" in the API/UI even
-- though nothing the operator did changed, and would also reset the very age the
-- sweeper's TTL gate reads. These columns are written only by the sweeper and the
-- dispatch writer (never alongside a real field), so skipping the bump when any of
-- them changes is safe. A normal update (status, result, ...) leaves them unchanged,
-- so the guard is true and `updated_at` bumps exactly as before — including
-- `assign_agent_in_tx`, which keeps refreshing `updated_at` on re-dispatch so the
-- sweeper's race re-check still works.
DROP TRIGGER IF EXISTS orchestration_tasks_updated_at ON orchestration_tasks;
CREATE TRIGGER orchestration_tasks_updated_at
    BEFORE UPDATE ON orchestration_tasks
    FOR EACH ROW
    WHEN (OLD.task_images_workspace_id IS NOT DISTINCT FROM NEW.task_images_workspace_id
          AND OLD.task_images_cleaned_at IS NOT DISTINCT FROM NEW.task_images_cleaned_at
          AND OLD.task_images_retry_after IS NOT DISTINCT FROM NEW.task_images_retry_after)
    EXECUTE FUNCTION update_updated_at();

-- Partial index for the sweeper scan: only not-yet-done tasks that actually have a
-- materialized workspace dir, ordered by age.
CREATE INDEX IF NOT EXISTS idx_orch_tasks_images_cleanup
    ON orchestration_tasks (updated_at)
    WHERE task_images_workspace_id IS NOT NULL AND task_images_cleaned_at IS NULL;

-- Backfill the workspace for image tasks that were materialized BEFORE this
-- migration, so the sweeper can reclaim their pre-existing `.task-images/<id>`
-- directories instead of leaking them forever (the dispatch writer only stamps the
-- column for tasks dispatched after this deploy). The directory was materialized
-- into the assigned agent's workspace at dispatch and agents do not change
-- workspace, so the agent's current workspace is where the directory lives. Tasks of
-- ANY status are stamped (not just terminal ones): an image task still `working` or
-- `blocked` when this migration runs has its directory on disk, and the
-- completion/result paths never set this new column, so a terminal-only backfill
-- would leak it once it finishes; the sweeper itself only acts on terminal tasks, so
-- stamping a still-running task is harmless. Over-stamping a task with no directory is
-- harmless too — the sweeper no-ops (and marks done) on a missing directory. Tasks
-- whose agent was already hard-deleted cannot be located and are left for manual
-- cleanup (a vanishingly small set, since instruction image upload ships with this
-- migration). The `imageAttachmentIds` test uses a CASE (not a bare
-- `jsonb_array_length`) so a malformed non-array value is treated as "no images"
-- instead of raising and aborting the migration — PostgreSQL does not guarantee the
-- `jsonb_typeof` guard short-circuits first. Idempotent: only NULL columns are
-- filled. Runs AFTER the trigger is replaced, so this UPDATE (a cleanup-column write)
-- does not bump `updated_at` and pre-existing terminal directories stay eligible on
-- the first post-deploy tick rather than waiting out another TTL.
UPDATE orchestration_tasks t
   SET task_images_workspace_id = a.workspace_id
  FROM agents a
 WHERE t.assigned_agent_id = a.id
   AND a.workspace_id IS NOT NULL
   AND t.task_images_workspace_id IS NULL
   AND CASE
         WHEN jsonb_typeof(t.params -> 'imageAttachmentIds') = 'array'
           THEN jsonb_array_length(t.params -> 'imageAttachmentIds')
         ELSE 0
       END > 0;
