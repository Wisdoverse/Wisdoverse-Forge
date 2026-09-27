-- no-transaction
-- Validate separately to avoid scanning event history under the stronger lock
-- used to replace the constraint.
ALTER TABLE events VALIDATE CONSTRAINT events_ingest_receipt_complete;
