-- no-transaction
-- F049: validate the enrollment_idempotency.org_id FK added NOT VALID in 075.
ALTER TABLE enrollment_idempotency VALIDATE CONSTRAINT enrollment_idempotency_org_id_fkey;
