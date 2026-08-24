-- Review checks: a lightweight, per-task human review checklist.
-- Each task can carry a set of named checks (e.g. "result_matches_brief") that
-- reviewers tick off; the rows become review evidence after the fact and feed
-- the "reviewed" signal in the product north star.
CREATE TABLE IF NOT EXISTS task_review_checks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    task_id UUID NOT NULL REFERENCES orchestration_tasks(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id),
    check_key TEXT NOT NULL,
    done BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT task_review_checks_key_length CHECK (char_length(check_key) BETWEEN 1 AND 64),
    CONSTRAINT task_review_checks_unique UNIQUE (task_id, user_id, check_key)
);

CREATE INDEX IF NOT EXISTS idx_task_review_checks_task ON task_review_checks(task_id);
CREATE INDEX IF NOT EXISTS idx_task_review_checks_org ON task_review_checks(organization_id);
