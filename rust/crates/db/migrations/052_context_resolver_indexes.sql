-- Unit 3.2: deterministic context resolver ranking support.

CREATE INDEX IF NOT EXISTS idx_memory_items_resolver_scope_rank
    ON memory_items (
        organization_id,
        workspace_id,
        scope_kind,
        scope_id,
        revoked_at,
        last_verified_at DESC,
        confidence DESC,
        id ASC
    )
    INCLUDE (title)
    WHERE state = 'active';
