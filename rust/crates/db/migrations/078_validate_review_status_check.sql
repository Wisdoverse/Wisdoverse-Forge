-- no-transaction
-- F048: validate the review_status CHECK added NOT VALID in 075. Single
-- statement + `-- no-transaction` so it runs without a wrapping transaction and
-- the brief lock from 075's ADD is already released (online validation).
ALTER TABLE orchestration_tasks VALIDATE CONSTRAINT orchestration_tasks_review_status_check;
