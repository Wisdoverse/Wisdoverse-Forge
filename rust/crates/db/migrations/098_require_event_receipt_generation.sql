-- CHECK constraints also accept NULL results. Require the generation explicitly
-- so an event ID cannot be stored as a partial receipt. Keep legacy all-NULL
-- rows valid and preserve previously applied migration checksums.

ALTER TABLE events DROP CONSTRAINT events_ingest_receipt_complete;

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
            AND ingest_generation_fingerprint IS NOT NULL
            AND ingest_generation_fingerprint ~ '^[0-9a-f]{64}$'
            AND (lifecycle_sequence IS NULL OR lifecycle_sequence > 0)
            AND ingest_applied IS NOT NULL
        )
    ) NOT VALID;
