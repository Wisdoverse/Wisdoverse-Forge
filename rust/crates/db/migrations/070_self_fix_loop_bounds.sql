ALTER TABLE orchestration_tasks ADD COLUMN IF NOT EXISTS merge_attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE orchestration_tasks ADD COLUMN IF NOT EXISTS review_opened_at TIMESTAMPTZ;
