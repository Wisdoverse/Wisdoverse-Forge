-- Immutable image evidence for the currently attached Agent container.
-- Task-run writers snapshot this JSONB value at first dispatch.

ALTER TABLE agents
    ADD COLUMN container_image_identity JSONB;

ALTER TABLE agents
    ADD COLUMN interactive_lease_expires_at TIMESTAMPTZ;

-- Durable, non-secret owner epoch for interactive Container CLI work. The
-- current-generation sidecar renews only this exact hook session; terminal/MCP
-- bridge leases intentionally leave it NULL until a signed Working hook claims
-- the epoch.
ALTER TABLE agents
    ADD COLUMN interactive_owner_session_id TEXT;

ALTER TABLE agents
    ADD CONSTRAINT agents_container_image_identity_shape CHECK (
        container_image_identity IS NULL
        OR (
            container_id IS NOT NULL
            AND jsonb_typeof(container_image_identity) = 'object'
            AND container_image_identity ?& ARRAY['source', 'imageId', 'versionSource', 'trust']
            AND jsonb_typeof(container_image_identity -> 'source') = 'string'
            AND jsonb_typeof(container_image_identity -> 'imageId') = 'string'
            AND jsonb_typeof(container_image_identity -> 'versionSource') = 'string'
            AND jsonb_typeof(container_image_identity -> 'trust') = 'string'
            AND btrim(container_image_identity ->> 'source') <> ''
            AND container_image_identity ->> 'imageId' ~ '^sha256:[0-9a-fA-F]{64}$'
            AND container_image_identity ->> 'versionSource' IN ('docker-label', 'not-reported')
            AND container_image_identity ->> 'trust' IN ('verified-signature', 'host-local')
            AND (
                NOT container_image_identity ? 'version'
                OR (
                    jsonb_typeof(container_image_identity -> 'version') = 'string'
                    AND btrim(container_image_identity ->> 'version') <> ''
                )
            )
            AND (
                (
                    container_image_identity ->> 'trust' = 'host-local'
                    AND NOT container_image_identity ? 'manifestDigest'
                )
                OR (
                    container_image_identity ->> 'trust' = 'verified-signature'
                    AND container_image_identity ? 'manifestDigest'
                    AND jsonb_typeof(container_image_identity -> 'manifestDigest') = 'string'
                    AND container_image_identity ->> 'manifestDigest' ~ '^sha256:[0-9a-fA-F]{64}$'
                    AND container_image_identity ->> 'source'
                        LIKE '%@' || (container_image_identity ->> 'manifestDigest')
                )
            )
        )
    );

-- `agent_id` is historical evidence, not ownership of a live Agent row. Keep
-- the immutable UUID snapshot when an operator deletes the Agent; future run
-- inserts still prove the Agent exists through their INSERT ... SELECT gate.
ALTER TABLE task_runs
    DROP CONSTRAINT IF EXISTS task_runs_agent_id_fkey;
