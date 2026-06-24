-- Durable user-facing Inbox notifications.
--
-- `orchestration_inbox` is a worker delivery dedupe table. This table backs
-- the browser Inbox for human owners, so missed WebSocket events survive page
-- refreshes and offline periods.

CREATE TABLE IF NOT EXISTS inbox_notifications (
    id TEXT PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    notification_type TEXT NOT NULL CHECK (
        notification_type IN (
            'blocked',
            'completed',
            'failed',
            'assigned',
            'mentioned',
            'credential_expired'
        )
    ),
    task_id UUID REFERENCES orchestration_tasks(id) ON DELETE CASCADE,
    task_title TEXT NOT NULL,
    message TEXT NOT NULL,
    task_href TEXT,
    read BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_inbox_notifications_owner
    ON inbox_notifications(organization_id, user_id, read, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_inbox_notifications_task
    ON inbox_notifications(organization_id, task_id)
    WHERE task_id IS NOT NULL;
