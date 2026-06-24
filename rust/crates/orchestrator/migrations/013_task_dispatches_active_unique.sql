-- Active-dispatch uniqueness for assign idempotency (#892, finding F040).
--
-- Enforce at most one active (queued/starting/started) task_dispatches row per
-- task, so a retried assign cannot spawn a second agent session for the same
-- task. Resolve any pre-existing duplicate active dispatches first (keep the
-- newest per task, fail the rest) so the unique index can be created on drifted
-- production data. Idempotent: safe on fresh databases and on re-run.
WITH ranked AS (
    SELECT id,
           ROW_NUMBER() OVER (PARTITION BY task_id ORDER BY created_at DESC, id DESC) AS rn
    FROM task_dispatches
    WHERE status IN ('queued', 'starting', 'started')
)
UPDATE task_dispatches d
SET status = 'failed',
    last_error = 'superseded_by_active_dispatch_dedup',
    updated_at = NOW()
FROM ranked r
WHERE d.id = r.id AND r.rn > 1;

CREATE UNIQUE INDEX IF NOT EXISTS idx_task_dispatches_active_unique
    ON task_dispatches (task_id)
    WHERE status IN ('queued', 'starting', 'started');
