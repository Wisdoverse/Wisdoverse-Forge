-- no-transaction
-- Validation uses the lighter SHARE UPDATE EXCLUSIVE lock and is separated from
-- column creation so existing installations do not hold an ACCESS EXCLUSIVE
-- lock while scanning historical events.
ALTER TABLE agents VALIDATE CONSTRAINT agents_lifecycle_sequence_valid;
ALTER TABLE events VALIDATE CONSTRAINT events_ingest_receipt_complete;
