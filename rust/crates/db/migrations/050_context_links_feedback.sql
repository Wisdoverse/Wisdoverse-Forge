-- Unit 2.5: context provenance links and per-run feedback.
--
-- `context_links` intentionally uses polymorphic item/ref pairs. PostgreSQL
-- cannot enforce cross-table FKs here, so write paths validate item/ref
-- existence in a transaction and the Unit 5.2 runbook owns orphan cleanup.

CREATE TABLE IF NOT EXISTS context_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    item_id UUID NOT NULL,
    item_kind TEXT NOT NULL,
    ref_id UUID NOT NULL,
    ref_kind TEXT NOT NULL,
    link_type TEXT NOT NULL,
    created_by_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT context_links_item_kind_check CHECK (item_kind IN ('memory', 'skill')),
    CONSTRAINT context_links_ref_kind_check CHECK (
        ref_kind IN ('task', 'run', 'agent', 'user', 'team', 'project', 'source_message')
    ),
    CONSTRAINT context_links_link_type_check CHECK (
        link_type IN ('applied', 'suggested', 'source', 'derived_from')
    ),
    CONSTRAINT context_links_unique_link UNIQUE (
        organization_id, workspace_id, item_id, item_kind, ref_id, ref_kind, link_type
    )
);

CREATE INDEX IF NOT EXISTS idx_context_links_item_ref_cover
    ON context_links(item_id, item_kind, ref_kind, ref_id)
    INCLUDE (link_type, created_at);

CREATE INDEX IF NOT EXISTS idx_context_links_ref
    ON context_links(ref_id, ref_kind)
    INCLUDE (item_kind, item_id, link_type, created_at);

CREATE INDEX IF NOT EXISTS idx_context_links_org_workspace_created
    ON context_links(organization_id, workspace_id, created_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS context_feedback (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES task_runs(id) ON DELETE CASCADE,
    item_id UUID NOT NULL,
    item_kind TEXT NOT NULL,
    label TEXT NOT NULL,
    note TEXT,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT context_feedback_item_kind_check CHECK (item_kind IN ('memory', 'skill')),
    CONSTRAINT context_feedback_label_check CHECK (
        label IN ('useful', 'stale', 'wrong', 'too_sensitive', 'do_not_use_again')
    ),
    CONSTRAINT context_feedback_note_length_check CHECK (note IS NULL OR char_length(note) <= 4000),
    CONSTRAINT context_feedback_unique_user_run_item UNIQUE (run_id, item_id, item_kind, user_id)
);

CREATE INDEX IF NOT EXISTS idx_context_feedback_item
    ON context_feedback(item_id, item_kind);

CREATE INDEX IF NOT EXISTS idx_context_feedback_run
    ON context_feedback(run_id);

CREATE INDEX IF NOT EXISTS idx_context_feedback_negative_window
    ON context_feedback(organization_id, workspace_id, item_kind, item_id, label, created_at DESC)
    WHERE label IN ('stale', 'wrong', 'too_sensitive');

DROP TRIGGER IF EXISTS context_feedback_updated_at ON context_feedback;
CREATE TRIGGER context_feedback_updated_at
    BEFORE UPDATE ON context_feedback
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();
