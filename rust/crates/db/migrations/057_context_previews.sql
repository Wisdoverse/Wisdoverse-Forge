-- Unit 4.2: short-lived immutable context previews used as a publish guard.

CREATE TABLE IF NOT EXISTS context_previews (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    task_id UUID NOT NULL REFERENCES orchestration_tasks(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    created_by_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    task_draft_hash TEXT NOT NULL,
    preview_hash TEXT NOT NULL,
    selected_items JSONB NOT NULL,
    removed_item_ids UUID[] NOT NULL DEFAULT '{}',
    pinned_item_ids UUID[] NOT NULL DEFAULT '{}',
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT context_previews_hash_check CHECK (
        char_length(task_draft_hash) = 64 AND char_length(preview_hash) = 64
    ),
    CONSTRAINT context_previews_selected_items_array_check CHECK (jsonb_typeof(selected_items) = 'array')
);

CREATE INDEX IF NOT EXISTS idx_context_previews_task_agent_live
    ON context_previews(organization_id, workspace_id, task_id, agent_id, expires_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_context_previews_creator_live
    ON context_previews(organization_id, created_by_user_id, expires_at DESC, id DESC);
