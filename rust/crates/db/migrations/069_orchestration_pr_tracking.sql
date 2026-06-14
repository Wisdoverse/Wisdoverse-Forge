-- 068: self-fix PR tracking columns on orchestration_tasks.
-- Additive + idempotent. base_commit_sha is the origin/main SHA pinned at dispatch
-- (the base the PR Bridge rebuilds onto); pr_* are GitHub opaque values; self_fix marks
-- a code-fix task against this repo; review_status mirrors the orchestrator ReviewState
-- vocabulary but is driven API-side on the task.

ALTER TABLE orchestration_tasks
    ADD COLUMN IF NOT EXISTS self_fix BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS base_commit_sha TEXT,
    ADD COLUMN IF NOT EXISTS pr_number INT,
    ADD COLUMN IF NOT EXISTS pr_url TEXT,
    ADD COLUMN IF NOT EXISTS pr_head_sha TEXT,
    ADD COLUMN IF NOT EXISTS review_status TEXT;
