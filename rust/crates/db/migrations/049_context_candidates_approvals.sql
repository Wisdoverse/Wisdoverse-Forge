-- Unit 2.4: context approval queue.
--
-- Candidates are workspace-scoped so approval queue reads remain tenant-safe
-- even when source_run_id is cleared by retention. Proposed content stays JSONB
-- but API responses expose only redacted previews.

CREATE TABLE IF NOT EXISTS context_candidates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source_run_id UUID REFERENCES task_runs(id) ON DELETE SET NULL,
    target_skill_id UUID REFERENCES skills(id) ON DELETE SET NULL,
    item_kind TEXT NOT NULL,
    proposed_content JSONB NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending',
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS context_approvals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    candidate_id UUID NOT NULL REFERENCES context_candidates(id) ON DELETE CASCADE,
    approver_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    decision TEXT NOT NULL,
    scope_kind TEXT,
    scope_id UUID,
    ttl_at TIMESTAMPTZ,
    sensitivity TEXT,
    reason TEXT,
    self_approval BOOLEAN NOT NULL DEFAULT FALSE,
    user_attest_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'context_candidates_item_kind_check'
          AND conrelid = 'context_candidates'::regclass
    ) THEN
        ALTER TABLE context_candidates
            ADD CONSTRAINT context_candidates_item_kind_check
            CHECK (item_kind IN ('memory', 'skill')) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'context_candidates_state_check'
          AND conrelid = 'context_candidates'::regclass
    ) THEN
        ALTER TABLE context_candidates
            ADD CONSTRAINT context_candidates_state_check
            CHECK (state IN ('pending', 'approved', 'rejected', 'superseded')) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'context_candidates_proposed_content_object_check'
          AND conrelid = 'context_candidates'::regclass
    ) THEN
        ALTER TABLE context_candidates
            ADD CONSTRAINT context_candidates_proposed_content_object_check
            CHECK (jsonb_typeof(proposed_content) = 'object') NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'context_approvals_decision_check'
          AND conrelid = 'context_approvals'::regclass
    ) THEN
        ALTER TABLE context_approvals
            ADD CONSTRAINT context_approvals_decision_check
            CHECK (decision IN ('approved', 'rejected')) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'context_approvals_scope_kind_check'
          AND conrelid = 'context_approvals'::regclass
    ) THEN
        ALTER TABLE context_approvals
            ADD CONSTRAINT context_approvals_scope_kind_check
            CHECK (scope_kind IS NULL OR scope_kind IN ('user', 'team', 'project')) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'context_approvals_sensitivity_check'
          AND conrelid = 'context_approvals'::regclass
    ) THEN
        ALTER TABLE context_approvals
            ADD CONSTRAINT context_approvals_sensitivity_check
            CHECK (sensitivity IS NULL OR sensitivity IN ('public', 'internal', 'confidential', 'secret_detected')) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'context_approvals_approved_scope_check'
          AND conrelid = 'context_approvals'::regclass
    ) THEN
        ALTER TABLE context_approvals
            ADD CONSTRAINT context_approvals_approved_scope_check
            CHECK (
                decision = 'rejected'
                OR (
                    scope_kind IS NOT NULL
                    AND scope_id IS NOT NULL
                    AND sensitivity IS NOT NULL
                )
            ) NOT VALID;
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_context_approvals_candidate_once
    ON context_approvals(candidate_id);

CREATE INDEX IF NOT EXISTS idx_context_candidates_pending_scope
    ON context_candidates(organization_id, workspace_id, created_at DESC, id DESC)
    WHERE state = 'pending';

CREATE INDEX IF NOT EXISTS idx_context_candidates_source_run
    ON context_candidates(source_run_id)
    WHERE source_run_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_context_candidates_target_skill
    ON context_candidates(target_skill_id)
    WHERE target_skill_id IS NOT NULL;

DROP TRIGGER IF EXISTS context_candidates_updated_at ON context_candidates;
CREATE TRIGGER context_candidates_updated_at
    BEFORE UPDATE ON context_candidates
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE OR REPLACE FUNCTION enforce_context_candidate_state_transition()
RETURNS trigger AS $$
BEGIN
    IF TG_OP <> 'UPDATE' OR OLD.state = NEW.state THEN
        RETURN NEW;
    END IF;

    IF OLD.state = 'pending' AND NEW.state IN ('approved', 'rejected', 'superseded') THEN
        RETURN NEW;
    END IF;

    RAISE EXCEPTION 'invalid context candidate state transition from % to %', OLD.state, NEW.state;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS context_candidates_state_transition ON context_candidates;
CREATE TRIGGER context_candidates_state_transition
    BEFORE UPDATE OF state ON context_candidates
    FOR EACH ROW EXECUTE FUNCTION enforce_context_candidate_state_transition();

CREATE OR REPLACE FUNCTION enforce_context_approval_self_approval()
RETURNS trigger AS $$
DECLARE
    candidate_owner UUID;
BEGIN
    IF NEW.decision <> 'approved' THEN
        RETURN NEW;
    END IF;

    SELECT owner_user_id INTO candidate_owner
      FROM context_candidates
     WHERE id = NEW.candidate_id;

    IF candidate_owner = NEW.approver_user_id AND NEW.scope_kind <> 'user' THEN
        RAISE EXCEPTION 'self approval is not allowed for wider context scopes';
    END IF;

    IF candidate_owner = NEW.approver_user_id
       AND NEW.scope_kind = 'user'
       AND NEW.scope_id IS DISTINCT FROM NEW.approver_user_id THEN
        RAISE EXCEPTION 'self approval is only allowed for the approver user scope';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS context_approvals_self_approval ON context_approvals;
CREATE TRIGGER context_approvals_self_approval
    BEFORE INSERT ON context_approvals
    FOR EACH ROW EXECUTE FUNCTION enforce_context_approval_self_approval();

ALTER TABLE context_candidates VALIDATE CONSTRAINT context_candidates_item_kind_check;
ALTER TABLE context_candidates VALIDATE CONSTRAINT context_candidates_state_check;
ALTER TABLE context_candidates VALIDATE CONSTRAINT context_candidates_proposed_content_object_check;
ALTER TABLE context_approvals VALIDATE CONSTRAINT context_approvals_decision_check;
ALTER TABLE context_approvals VALIDATE CONSTRAINT context_approvals_scope_kind_check;
ALTER TABLE context_approvals VALIDATE CONSTRAINT context_approvals_sensitivity_check;
ALTER TABLE context_approvals VALIDATE CONSTRAINT context_approvals_approved_scope_check;
