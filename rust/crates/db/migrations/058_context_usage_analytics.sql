-- Unit 5.1: context usage analytics snapshot.
--
-- The dashboard reads this materialized view as a last-good snapshot. Runtime
-- refresh is owned by the API server's nightly worker and uses
-- REFRESH MATERIALIZED VIEW CONCURRENTLY guarded by a PostgreSQL advisory lock.

CREATE MATERIALIZED VIEW IF NOT EXISTS context_usage_analytics AS
WITH feedback AS (
    SELECT
        organization_id,
        workspace_id,
        run_id,
        item_id,
        item_kind,
        COUNT(*)::bigint AS feedback_total_count,
        COUNT(*) FILTER (WHERE label = 'useful')::bigint AS feedback_useful_count,
        COUNT(*) FILTER (WHERE label IN ('stale', 'wrong', 'too_sensitive', 'do_not_use_again'))::bigint
            AS feedback_negative_count,
        MAX(updated_at) AS last_feedback_at
    FROM context_feedback
    GROUP BY organization_id, workspace_id, run_id, item_id, item_kind
)
SELECT
    rci.organization_id,
    rci.workspace_id,
    tr.agent_id,
    COALESCE(a.name, rci.adapter) AS agent_name,
    rci.item_id,
    rci.item_kind,
    COALESCE(
        NULLIF(rci.applied_snapshot->>'title', ''),
        NULLIF(rci.applied_snapshot->>'name', ''),
        NULLIF(rci.applied_snapshot #>> '{source,title}', ''),
        rci.item_id::text
    ) AS item_title,
    COALESCE(mi.scope_kind, sk.scope_kind) AS scope_kind,
    COALESCE(mi.scope_id, sk.scope_id) AS scope_id,
    COALESCE(mi.state, sk.state) AS item_state,
    COALESCE(mi.sensitivity, sk.sensitivity) AS sensitivity,
    COALESCE(mi.last_verified_at, sk.updated_at) AS last_verified_at,
    COALESCE(
        NULLIF(tr.capability_profile->>'task_kind', ''),
        NULLIF(tr.capability_profile->>'taskKind', ''),
        NULLIF(task.params->>'task_kind', ''),
        NULLIF(task.params->>'taskKind', ''),
        NULLIF(task.params->>'kind', ''),
        'task'
    ) AS task_kind,
    COALESCE(
        NULLIF(rci.capability_profile->>'runtime_kind', ''),
        NULLIF(rci.capability_profile->>'runtimeKind', ''),
        NULLIF(tr.capability_profile #>> '{runtime_capability,runtime_kind}', ''),
        NULLIF(a.cli_tool, ''),
        NULLIF(rci.adapter, ''),
        'unknown'
    ) AS runtime,
    COUNT(DISTINCT rci.id)::bigint AS applied_count,
    COUNT(DISTINCT rci.id) FILTER (WHERE tr.status = 'completed')::bigint AS completed_count,
    CASE
        WHEN COUNT(DISTINCT rci.id) = 0 THEN 0::double precision
        ELSE COUNT(DISTINCT rci.id) FILTER (WHERE tr.status = 'completed')::double precision
            / COUNT(DISTINCT rci.id)::double precision
    END AS success_rate,
    COALESCE(SUM(feedback.feedback_total_count), 0)::bigint AS feedback_total_count,
    COALESCE(SUM(feedback.feedback_useful_count), 0)::bigint AS feedback_useful_count,
    COALESCE(SUM(feedback.feedback_negative_count), 0)::bigint AS feedback_negative_count,
    CASE
        WHEN COALESCE(SUM(feedback.feedback_total_count), 0) = 0 THEN 0::double precision
        ELSE COALESCE(SUM(feedback.feedback_negative_count), 0)::double precision
            / COALESCE(SUM(feedback.feedback_total_count), 0)::double precision
    END AS negative_feedback_rate,
    MAX(rci.applied_at) AS last_used_at,
    MAX(feedback.last_feedback_at) AS last_feedback_at
FROM run_context_injections rci
JOIN task_runs tr
  ON tr.id = rci.run_id
 AND tr.organization_id = rci.organization_id
 AND tr.workspace_id = rci.workspace_id
JOIN orchestration_tasks task
  ON task.id = tr.orchestration_task_id
 AND task.organization_id = tr.organization_id
LEFT JOIN agents a
  ON a.id = tr.agent_id
 AND a.organization_id = tr.organization_id
 AND a.workspace_id = tr.workspace_id
LEFT JOIN memory_items mi
  ON rci.item_kind = 'memory'
 AND mi.id = rci.item_id
 AND mi.organization_id = rci.organization_id
 AND mi.workspace_id = rci.workspace_id
LEFT JOIN skills sk
  ON rci.item_kind = 'skill'
 AND sk.id = rci.item_id
 AND sk.organization_id = rci.organization_id
 AND sk.workspace_id = rci.workspace_id
LEFT JOIN feedback
  ON feedback.organization_id = rci.organization_id
 AND feedback.workspace_id = rci.workspace_id
 AND feedback.run_id = rci.run_id
 AND feedback.item_id = rci.item_id
 AND feedback.item_kind = rci.item_kind
GROUP BY
    rci.organization_id,
    rci.workspace_id,
    tr.agent_id,
    COALESCE(a.name, rci.adapter),
    rci.item_id,
    rci.item_kind,
    COALESCE(
        NULLIF(rci.applied_snapshot->>'title', ''),
        NULLIF(rci.applied_snapshot->>'name', ''),
        NULLIF(rci.applied_snapshot #>> '{source,title}', ''),
        rci.item_id::text
    ),
    COALESCE(mi.scope_kind, sk.scope_kind),
    COALESCE(mi.scope_id, sk.scope_id),
    COALESCE(mi.state, sk.state),
    COALESCE(mi.sensitivity, sk.sensitivity),
    COALESCE(mi.last_verified_at, sk.updated_at),
    COALESCE(
        NULLIF(tr.capability_profile->>'task_kind', ''),
        NULLIF(tr.capability_profile->>'taskKind', ''),
        NULLIF(task.params->>'task_kind', ''),
        NULLIF(task.params->>'taskKind', ''),
        NULLIF(task.params->>'kind', ''),
        'task'
    ),
    COALESCE(
        NULLIF(rci.capability_profile->>'runtime_kind', ''),
        NULLIF(rci.capability_profile->>'runtimeKind', ''),
        NULLIF(tr.capability_profile #>> '{runtime_capability,runtime_kind}', ''),
        NULLIF(a.cli_tool, ''),
        NULLIF(rci.adapter, ''),
        'unknown'
    );

CREATE UNIQUE INDEX IF NOT EXISTS idx_context_usage_analytics_unique
    ON context_usage_analytics (
        organization_id,
        workspace_id,
        agent_id,
        item_id,
        item_kind,
        task_kind,
        runtime
    );

CREATE INDEX IF NOT EXISTS idx_context_usage_analytics_top_useful
    ON context_usage_analytics (
        organization_id,
        workspace_id,
        success_rate DESC,
        feedback_useful_count DESC,
        applied_count DESC
    );

CREATE INDEX IF NOT EXISTS idx_context_usage_analytics_needs_review
    ON context_usage_analytics (
        organization_id,
        workspace_id,
        negative_feedback_rate DESC,
        feedback_negative_count DESC,
        last_feedback_at DESC
    )
    WHERE feedback_negative_count > 0;

CREATE INDEX IF NOT EXISTS idx_context_usage_analytics_stale
    ON context_usage_analytics (
        organization_id,
        workspace_id,
        last_used_at ASC
    );

CREATE TABLE IF NOT EXISTS context_usage_analytics_refreshes (
    name TEXT PRIMARY KEY,
    last_refreshed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_refresh_started_at TIMESTAMPTZ,
    last_refresh_error TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO context_usage_analytics_refreshes (name, last_refreshed_at)
VALUES ('context_usage_analytics', now())
ON CONFLICT (name) DO NOTHING;
