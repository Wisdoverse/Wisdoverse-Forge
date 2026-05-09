-- no-transaction
-- R&D Orchestrator initial schema

BEGIN;

CREATE TABLE IF NOT EXISTS participants (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    type            TEXT NOT NULL CHECK (type IN ('human', 'agent')),
    display_name    TEXT NOT NULL,
    casdoor_user_id TEXT,
    agent_session_id TEXT,
    agent_provider  TEXT CHECK (agent_provider IN ('claude', 'gemini', 'codex', 'opencode')),
    org_id          TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT participant_type_check CHECK (
        (type = 'human' AND casdoor_user_id IS NOT NULL AND agent_session_id IS NULL)
        OR
        (type = 'agent' AND agent_session_id IS NOT NULL AND casdoor_user_id IS NULL)
    )
);

CREATE INDEX idx_participants_org ON participants(org_id);
CREATE INDEX idx_participants_casdoor ON participants(casdoor_user_id) WHERE casdoor_user_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS tasks (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workflow_id UUID,
    title       TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    state       TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'assigned', 'working', 'review', 'completed', 'failed')),
    priority    TEXT NOT NULL DEFAULT 'normal' CHECK (priority IN ('low', 'normal', 'high', 'urgent')),
    assigned_to UUID REFERENCES participants(id),
    review_id   UUID,
    created_by  UUID NOT NULL REFERENCES participants(id),
    org_id      TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_tasks_org_state ON tasks(org_id, state);
CREATE INDEX idx_tasks_assigned ON tasks(assigned_to) WHERE assigned_to IS NOT NULL;
CREATE INDEX idx_tasks_workflow ON tasks(workflow_id) WHERE workflow_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS task_dependencies (
    task_id    UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    depends_on UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, depends_on),
    CONSTRAINT no_self_dependency CHECK (task_id != depends_on)
);

CREATE TABLE IF NOT EXISTS workflows (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'running', 'completed', 'failed')),
    org_id      TEXT NOT NULL,
    created_by  UUID NOT NULL REFERENCES participants(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_workflows_org ON workflows(org_id);

CREATE TABLE IF NOT EXISTS workflow_nodes (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workflow_id UUID NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    type        TEXT NOT NULL CHECK (type IN ('agent_task', 'human_review', 'gate')),
    config      JSONB NOT NULL DEFAULT '{}',
    position    INT NOT NULL DEFAULT 0
);

CREATE INDEX idx_workflow_nodes_workflow ON workflow_nodes(workflow_id);

CREATE TABLE IF NOT EXISTS workflow_node_dependencies (
    node_id    UUID NOT NULL REFERENCES workflow_nodes(id) ON DELETE CASCADE,
    depends_on UUID NOT NULL REFERENCES workflow_nodes(id) ON DELETE CASCADE,
    PRIMARY KEY (node_id, depends_on),
    CONSTRAINT no_self_node_dependency CHECK (node_id != depends_on)
);

ALTER TABLE tasks ADD CONSTRAINT fk_tasks_workflow FOREIGN KEY (workflow_id) REFERENCES workflows(id);

CREATE TABLE IF NOT EXISTS code_reviews (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id     UUID NOT NULL REFERENCES tasks(id),
    session_id  TEXT NOT NULL,
    diff_ref    TEXT NOT NULL,
    state       TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'in_review', 'approved', 'changes_requested', 'rejected')),
    assigned_to UUID REFERENCES participants(id),
    org_id      TEXT NOT NULL,
    created_by  UUID NOT NULL REFERENCES participants(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE tasks ADD CONSTRAINT fk_tasks_review FOREIGN KEY (review_id) REFERENCES code_reviews(id);

CREATE INDEX idx_reviews_task ON code_reviews(task_id);
CREATE INDEX idx_reviews_org_state ON code_reviews(org_id, state);

CREATE TABLE IF NOT EXISTS review_comments (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    review_id  UUID NOT NULL REFERENCES code_reviews(id) ON DELETE CASCADE,
    author_id  UUID NOT NULL REFERENCES participants(id),
    body       TEXT NOT NULL,
    file_path  TEXT,
    line       INT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_review_comments_review ON review_comments(review_id);

CREATE TABLE IF NOT EXISTS knowledge_entries (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    type       TEXT NOT NULL CHECK (type IN ('session', 'document', 'snippet')),
    title      TEXT NOT NULL,
    content    TEXT NOT NULL,
    source_id  TEXT,
    tags       TEXT[] NOT NULL DEFAULT '{}',
    org_id     TEXT NOT NULL,
    created_by UUID NOT NULL REFERENCES participants(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_knowledge_org ON knowledge_entries(org_id);
CREATE INDEX idx_knowledge_tags ON knowledge_entries USING gin(tags);

CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tasks_updated_at BEFORE UPDATE ON tasks FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER workflows_updated_at BEFORE UPDATE ON workflows FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER code_reviews_updated_at BEFORE UPDATE ON code_reviews FOR EACH ROW EXECUTE FUNCTION update_updated_at();

COMMIT;
