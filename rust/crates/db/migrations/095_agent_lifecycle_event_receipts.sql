-- Durable ordering and exactly-once database effects for signed Agent events.
-- Columns are additive; existing historical rows remain outside the ingest
-- receipt contract because they predate a stable event identity.

ALTER TABLE agents
    ADD COLUMN lifecycle_generation_fingerprint TEXT,
    ADD COLUMN last_lifecycle_sequence BIGINT NOT NULL DEFAULT 0;

ALTER TABLE events
    ADD COLUMN ingest_event_id TEXT,
    ADD COLUMN ingest_generation_fingerprint TEXT,
    ADD COLUMN lifecycle_sequence BIGINT,
    ADD COLUMN ingest_applied BOOLEAN;

ALTER TABLE agents
    ADD CONSTRAINT agents_lifecycle_sequence_valid CHECK (
        last_lifecycle_sequence >= 0
        AND (
            lifecycle_generation_fingerprint IS NULL
            OR lifecycle_generation_fingerprint ~ '^[0-9a-f]{64}$'
        )
    ) NOT VALID;

ALTER TABLE events
    ADD CONSTRAINT events_ingest_receipt_complete CHECK (
        (
            ingest_event_id IS NULL
            AND ingest_generation_fingerprint IS NULL
            AND lifecycle_sequence IS NULL
            AND ingest_applied IS NULL
        )
        OR (
            ingest_event_id IS NOT NULL
            AND btrim(ingest_event_id) <> ''
            AND length(ingest_event_id) <= 256
            AND ingest_generation_fingerprint ~ '^[0-9a-f]{64}$'
            AND (lifecycle_sequence IS NULL OR lifecycle_sequence > 0)
            AND ingest_applied IS NOT NULL
        )
    ) NOT VALID;
