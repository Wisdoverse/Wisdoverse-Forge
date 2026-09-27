-- no-transaction
-- A WAL redelivery keeps the same event ID. Build the uniqueness guard without
-- blocking event writers on an existing installation.
CREATE UNIQUE INDEX CONCURRENTLY events_ingest_identity_unique
    ON events (agent_id, ingest_generation_fingerprint, ingest_event_id)
    WHERE ingest_event_id IS NOT NULL;
