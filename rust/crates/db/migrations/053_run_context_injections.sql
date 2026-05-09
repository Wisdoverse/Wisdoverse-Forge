-- Unit 3.4: immutable context injection facts per run.
--
-- Source items intentionally do not have foreign keys here. Provenance must
-- survive memory/skill hard-deletes by relying on `applied_snapshot`.

CREATE TABLE IF NOT EXISTS run_context_injections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES task_runs(id) ON DELETE CASCADE,
    item_id UUID NOT NULL,
    item_kind TEXT NOT NULL,
    position INTEGER NOT NULL,
    adapter TEXT NOT NULL,
    envelope_version TEXT NOT NULL,
    capability_profile JSONB NOT NULL,
    applied_snapshot JSONB NOT NULL,
    degradation_reason TEXT,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT run_context_injections_item_kind_check CHECK (item_kind IN ('memory', 'skill')),
    CONSTRAINT run_context_injections_position_check CHECK (position >= 0),
    CONSTRAINT run_context_injections_adapter_check CHECK (char_length(adapter) BETWEEN 1 AND 64),
    CONSTRAINT run_context_injections_envelope_version_check CHECK (char_length(envelope_version) BETWEEN 1 AND 32),
    CONSTRAINT run_context_injections_snapshot_object_check CHECK (jsonb_typeof(applied_snapshot) = 'object'),
    CONSTRAINT run_context_injections_capability_object_check CHECK (jsonb_typeof(capability_profile) = 'object'),
    CONSTRAINT run_context_injections_degradation_length_check CHECK (
        degradation_reason IS NULL OR char_length(degradation_reason) <= 128
    ),
    CONSTRAINT run_context_injections_unique_item UNIQUE (run_id, item_id, item_kind),
    CONSTRAINT run_context_injections_unique_position UNIQUE (run_id, position)
);
