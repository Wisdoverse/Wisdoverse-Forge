CREATE TABLE IF NOT EXISTS maintenance_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    task_id UUID NOT NULL UNIQUE,
    repository TEXT NOT NULL CHECK (repository = lower(repository)),
    source_kind TEXT NOT NULL CHECK (source_kind IN ('request', 'pull_request')),
    source_reference TEXT NOT NULL CHECK (
        length(source_reference) BETWEEN 1 AND 128
        AND source_reference ~ '^[a-z0-9][a-z0-9._:-]*$'
    ),
    source_pr_number INTEGER,
    source_head_sha TEXT,
    default_branch TEXT NOT NULL CHECK (default_branch <> ''),
    starting_sha TEXT NOT NULL CHECK (starting_sha <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, repository, source_kind, source_reference),
    FOREIGN KEY (organization_id, task_id)
        REFERENCES orchestration_tasks (organization_id, id),
    CHECK (
        (source_kind = 'request' AND source_pr_number IS NULL AND source_head_sha IS NULL)
        OR (source_kind = 'pull_request' AND source_pr_number IS NOT NULL AND source_pr_number > 0
            AND source_reference = source_pr_number::text AND source_head_sha IS NOT NULL)
    )
);
