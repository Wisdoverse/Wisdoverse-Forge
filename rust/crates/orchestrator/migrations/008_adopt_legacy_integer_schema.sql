-- no-transaction
-- Adopt legacy integer-key orchestrator tables by preserving them under
-- legacy_orchestrator and recreating the Rust-owned UUID schema.

BEGIN;

CREATE SCHEMA IF NOT EXISTS legacy_orchestrator;

DO $$
DECLARE
    has_legacy_integer_schema BOOLEAN;
BEGIN
    SELECT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name IN (
              'tasks',
              'task_dependencies',
              'workflows',
              'workflow_nodes',
              'workflow_node_dependencies',
              'code_reviews',
              'review_comments',
              'knowledge_entries',
              'audit_logs'
          )
          AND column_name IN ('id', 'task_id', 'workflow_id', 'node_id', 'depends_on', 'review_id')
          AND udt_name <> 'uuid'
    )
    INTO has_legacy_integer_schema;

    IF has_legacy_integer_schema THEN
        IF to_regclass('public.review_comments') IS NOT NULL
           AND to_regclass('legacy_orchestrator.review_comments_legacy_int') IS NULL THEN
            ALTER TABLE public.review_comments SET SCHEMA legacy_orchestrator;
            ALTER TABLE legacy_orchestrator.review_comments RENAME TO review_comments_legacy_int;
        END IF;

        IF to_regclass('public.code_reviews') IS NOT NULL
           AND to_regclass('legacy_orchestrator.code_reviews_legacy_int') IS NULL THEN
            ALTER TABLE public.code_reviews SET SCHEMA legacy_orchestrator;
            ALTER TABLE legacy_orchestrator.code_reviews RENAME TO code_reviews_legacy_int;
        END IF;

        IF to_regclass('public.task_dependencies') IS NOT NULL
           AND to_regclass('legacy_orchestrator.task_dependencies_legacy_int') IS NULL THEN
            ALTER TABLE public.task_dependencies SET SCHEMA legacy_orchestrator;
            ALTER TABLE legacy_orchestrator.task_dependencies RENAME TO task_dependencies_legacy_int;
        END IF;

        IF to_regclass('public.workflow_node_dependencies') IS NOT NULL
           AND to_regclass('legacy_orchestrator.workflow_node_dependencies_legacy_int') IS NULL THEN
            ALTER TABLE public.workflow_node_dependencies SET SCHEMA legacy_orchestrator;
            ALTER TABLE legacy_orchestrator.workflow_node_dependencies RENAME TO workflow_node_dependencies_legacy_int;
        END IF;

        IF to_regclass('public.workflow_nodes') IS NOT NULL
           AND to_regclass('legacy_orchestrator.workflow_nodes_legacy_int') IS NULL THEN
            ALTER TABLE public.workflow_nodes SET SCHEMA legacy_orchestrator;
            ALTER TABLE legacy_orchestrator.workflow_nodes RENAME TO workflow_nodes_legacy_int;
        END IF;

        IF to_regclass('public.tasks') IS NOT NULL
           AND to_regclass('legacy_orchestrator.tasks_legacy_int') IS NULL THEN
            ALTER TABLE public.tasks SET SCHEMA legacy_orchestrator;
            ALTER TABLE legacy_orchestrator.tasks RENAME TO tasks_legacy_int;
        END IF;

        IF to_regclass('public.workflows') IS NOT NULL
           AND to_regclass('legacy_orchestrator.workflows_legacy_int') IS NULL THEN
            ALTER TABLE public.workflows SET SCHEMA legacy_orchestrator;
            ALTER TABLE legacy_orchestrator.workflows RENAME TO workflows_legacy_int;
        END IF;

        IF to_regclass('public.knowledge_entries') IS NOT NULL
           AND to_regclass('legacy_orchestrator.knowledge_entries_legacy_int') IS NULL THEN
            ALTER TABLE public.knowledge_entries SET SCHEMA legacy_orchestrator;
            ALTER TABLE legacy_orchestrator.knowledge_entries RENAME TO knowledge_entries_legacy_int;
        END IF;

        IF to_regclass('public.audit_logs') IS NOT NULL
           AND to_regclass('legacy_orchestrator.audit_logs_legacy_int') IS NULL THEN
            ALTER TABLE public.audit_logs SET SCHEMA legacy_orchestrator;
            ALTER TABLE legacy_orchestrator.audit_logs RENAME TO audit_logs_legacy_int;
        END IF;
    END IF;
END;
$$;

CREATE TABLE IF NOT EXISTS workflows (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name                 TEXT NOT NULL,
    description          TEXT NOT NULL DEFAULT '',
    status               TEXT NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'running', 'completed', 'failed', 'cancelled', 'paused')),
    org_id               TEXT NOT NULL,
    created_by           UUID NOT NULL REFERENCES participants(id),
    temporal_workflow_id TEXT,
    temporal_run_id      TEXT,
    team_id              UUID REFERENCES teams(id),
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_workflows_org ON workflows(org_id);
CREATE INDEX IF NOT EXISTS idx_workflows_team ON workflows(team_id) WHERE team_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS workflow_nodes (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workflow_id  UUID NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    type         TEXT NOT NULL CHECK (type IN ('agent_task', 'human_review', 'gate')),
    config       JSONB NOT NULL DEFAULT '{}',
    position     INT NOT NULL DEFAULT 0,
    status       TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'completed', 'failed', 'skipped')),
    started_at   TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    error        TEXT,
    output       JSONB
);

CREATE INDEX IF NOT EXISTS idx_workflow_nodes_workflow ON workflow_nodes(workflow_id);

CREATE TABLE IF NOT EXISTS workflow_node_dependencies (
    node_id    UUID NOT NULL REFERENCES workflow_nodes(id) ON DELETE CASCADE,
    depends_on UUID NOT NULL REFERENCES workflow_nodes(id) ON DELETE CASCADE,
    PRIMARY KEY (node_id, depends_on),
    CONSTRAINT no_self_node_dependency CHECK (node_id != depends_on)
);

CREATE TABLE IF NOT EXISTS tasks (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workflow_id           UUID REFERENCES workflows(id),
    title                 TEXT NOT NULL,
    description           TEXT NOT NULL DEFAULT '',
    state                 TEXT NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'assigned', 'working', 'review', 'completed', 'failed', 'changes_requested')),
    priority              TEXT NOT NULL DEFAULT 'normal'
        CHECK (priority IN ('low', 'normal', 'high', 'urgent')),
    assigned_to           UUID REFERENCES participants(id),
    review_id             UUID,
    agentforge_session_id TEXT,
    created_by            UUID NOT NULL REFERENCES participants(id),
    org_id                TEXT NOT NULL,
    team_id               UUID REFERENCES teams(id),
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_tasks_org_state ON tasks(org_id, state);
CREATE INDEX IF NOT EXISTS idx_tasks_assigned ON tasks(assigned_to) WHERE assigned_to IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tasks_workflow ON tasks(workflow_id) WHERE workflow_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tasks_team ON tasks(team_id) WHERE team_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS task_dependencies (
    task_id    UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    depends_on UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, depends_on),
    CONSTRAINT no_self_dependency CHECK (task_id != depends_on)
);

CREATE TABLE IF NOT EXISTS code_reviews (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id       UUID NOT NULL REFERENCES tasks(id),
    session_id    TEXT NOT NULL,
    diff_ref      TEXT NOT NULL,
    diff_snapshot JSONB,
    state         TEXT NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'in_review', 'approved', 'changes_requested', 'rejected')),
    assigned_to   UUID REFERENCES participants(id),
    org_id        TEXT NOT NULL,
    created_by    UUID NOT NULL REFERENCES participants(id),
    team_id       UUID REFERENCES teams(id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE tasks
    DROP CONSTRAINT IF EXISTS fk_tasks_review;

ALTER TABLE tasks
    ADD CONSTRAINT fk_tasks_review
    FOREIGN KEY (review_id) REFERENCES code_reviews(id);

CREATE INDEX IF NOT EXISTS idx_reviews_task ON code_reviews(task_id);
CREATE INDEX IF NOT EXISTS idx_reviews_org_state ON code_reviews(org_id, state);
CREATE INDEX IF NOT EXISTS idx_reviews_team ON code_reviews(team_id) WHERE team_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS review_comments (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    review_id  UUID NOT NULL REFERENCES code_reviews(id) ON DELETE CASCADE,
    author_id  UUID NOT NULL REFERENCES participants(id),
    body       TEXT NOT NULL,
    file_path  TEXT,
    line       INT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_review_comments_review ON review_comments(review_id);

CREATE TABLE IF NOT EXISTS knowledge_entries (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    type             TEXT NOT NULL
        CHECK (type IN ('session', 'document', 'snippet', 'session_summary', 'review_learnings', 'decision_record')),
    title            TEXT NOT NULL,
    content          TEXT NOT NULL,
    source_id        TEXT,
    source_type      TEXT NOT NULL DEFAULT '',
    source_ref       TEXT NOT NULL DEFAULT '',
    tags             TEXT[] NOT NULL DEFAULT '{}',
    org_id           TEXT NOT NULL,
    created_by       UUID NOT NULL REFERENCES participants(id),
    team_id          UUID REFERENCES teams(id),
    embedding_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (embedding_status IN ('pending', 'processing', 'completed', 'failed')),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_knowledge_org ON knowledge_entries(org_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_tags ON knowledge_entries USING gin(tags);
CREATE INDEX IF NOT EXISTS idx_knowledge_fts
    ON knowledge_entries USING gin(to_tsvector('english', title || ' ' || content));
CREATE INDEX IF NOT EXISTS idx_knowledge_team ON knowledge_entries(team_id) WHERE team_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS audit_logs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    action      TEXT NOT NULL,
    actor_id    TEXT NOT NULL,
    actor_type  TEXT NOT NULL CHECK (actor_type IN ('human', 'agent', 'system')),
    resource    TEXT NOT NULL,
    resource_id TEXT,
    org_id      TEXT NOT NULL,
    changes     JSONB,
    ip_address  INET,
    user_agent  TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_audit_org_time ON audit_logs(org_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_logs(actor_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_resource ON audit_logs(resource, resource_id);

DROP TRIGGER IF EXISTS tasks_updated_at ON tasks;
CREATE TRIGGER tasks_updated_at BEFORE UPDATE ON tasks FOR EACH ROW EXECUTE FUNCTION update_updated_at();

DROP TRIGGER IF EXISTS workflows_updated_at ON workflows;
CREATE TRIGGER workflows_updated_at BEFORE UPDATE ON workflows FOR EACH ROW EXECUTE FUNCTION update_updated_at();

DROP TRIGGER IF EXISTS code_reviews_updated_at ON code_reviews;
CREATE TRIGGER code_reviews_updated_at BEFORE UPDATE ON code_reviews FOR EACH ROW EXECUTE FUNCTION update_updated_at();

DROP TRIGGER IF EXISTS knowledge_entries_updated_at ON knowledge_entries;
CREATE TRIGGER knowledge_entries_updated_at BEFORE UPDATE ON knowledge_entries FOR EACH ROW EXECUTE FUNCTION update_updated_at();

COMMIT;
