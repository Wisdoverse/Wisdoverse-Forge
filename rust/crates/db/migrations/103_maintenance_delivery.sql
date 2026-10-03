CREATE TABLE IF NOT EXISTS maintenance_verification_reports (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    task_id UUID NOT NULL,
    run_id UUID,
    -- Preserve the recorded identity when an agent/run is explicitly removed.
    -- Only the live lookup link is nullable; the report snapshot is retained.
    live_run_id UUID,
    request_key UUID NOT NULL,
    author_id UUID NOT NULL,
    task_version BIGINT NOT NULL CHECK (task_version >= 0),
    revision TEXT NOT NULL CHECK (revision ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'),
    starting_revision TEXT NOT NULL CHECK (starting_revision ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'),
    criteria TEXT NOT NULL CHECK (octet_length(criteria) BETWEEN 1 AND 4000),
    comparison_key TEXT CHECK (comparison_key ~ '^[a-z0-9][a-z0-9._:-]{0,63}$'),
    input JSONB NOT NULL,
    snapshot JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, task_id, id),
    UNIQUE (organization_id, task_id, request_key),
    CHECK (live_run_id IS NULL OR (run_id IS NOT NULL AND live_run_id = run_id)),
    FOREIGN KEY (organization_id, task_id) REFERENCES orchestration_tasks (organization_id, id) ON DELETE CASCADE,
    FOREIGN KEY (organization_id, task_id, live_run_id)
        REFERENCES task_runs (organization_id, orchestration_task_id, id) ON DELETE SET NULL (live_run_id)
);
CREATE INDEX IF NOT EXISTS idx_maintenance_reports_task_time
    ON maintenance_verification_reports (organization_id, task_id, created_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS maintenance_review_decisions (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    task_id UUID NOT NULL,
    report_id UUID NOT NULL,
    request_key UUID NOT NULL,
    reviewer_id UUID NOT NULL,
    verdict TEXT NOT NULL CHECK (verdict IN ('accepted', 'rejected', 'rework', 'reopened')),
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 4000),
    setup_minutes INTEGER CHECK (setup_minutes BETWEEN 0 AND 100000),
    handling_minutes INTEGER CHECK (handling_minutes BETWEEN 0 AND 100000),
    review_minutes INTEGER CHECK (review_minutes BETWEEN 0 AND 100000),
    recovery_minutes INTEGER CHECK (recovery_minutes BETWEEN 0 AND 100000),
    rework_minutes INTEGER CHECK (rework_minutes BETWEEN 0 AND 100000),
    operation_minutes INTEGER CHECK (operation_minutes BETWEEN 0 AND 100000),
    baseline_minutes INTEGER CHECK (baseline_minutes BETWEEN 0 AND 600000),
    github_observation JSONB,
    input JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, task_id, request_key),
    FOREIGN KEY (organization_id, task_id, report_id)
        REFERENCES maintenance_verification_reports (organization_id, task_id, id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_maintenance_decisions_task_time
    ON maintenance_review_decisions (organization_id, task_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_maintenance_decisions_org_time
    ON maintenance_review_decisions (organization_id, created_at, task_id);
CREATE INDEX IF NOT EXISTS idx_maintenance_requests_org_time
    ON maintenance_requests (organization_id, created_at, task_id);

CREATE TABLE IF NOT EXISTS maintenance_handoffs (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    task_id UUID NOT NULL,
    request_key UUID NOT NULL,
    author_id UUID NOT NULL,
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 4000),
    next_step TEXT NOT NULL CHECK (octet_length(next_step) BETWEEN 1 AND 4000),
    input JSONB NOT NULL,
    snapshot JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, task_id, request_key),
    FOREIGN KEY (organization_id, task_id) REFERENCES orchestration_tasks (organization_id, id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_maintenance_handoffs_task_time
    ON maintenance_handoffs (organization_id, task_id, created_at DESC, id DESC);
