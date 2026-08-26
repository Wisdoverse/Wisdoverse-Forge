-- Human task updates: comments and blocker signals for an orchestration
-- task. First-class records, independent of execution attempts and task
-- lifecycle state, so a person can leave notes, ask a question, or flag a
-- blocker without touching the execution data model.
CREATE TABLE IF NOT EXISTS task_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    task_id UUID NOT NULL REFERENCES orchestration_tasks(id) ON DELETE CASCADE,
    author_user_id UUID NOT NULL REFERENCES users(id),
    kind TEXT NOT NULL DEFAULT 'comment',
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT task_comments_kind_check CHECK (kind IN ('comment', 'blocker', 'unblock'))
);

CREATE INDEX IF NOT EXISTS idx_task_comments_task ON task_comments(task_id, created_at);
CREATE INDEX IF NOT EXISTS idx_task_comments_org ON task_comments(organization_id);
