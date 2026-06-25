-- no-transaction
-- F049: validate the enrollment_idempotency.user_id FK added NOT VALID in 075.
ALTER TABLE enrollment_idempotency VALIDATE CONSTRAINT enrollment_idempotency_user_id_fkey;
