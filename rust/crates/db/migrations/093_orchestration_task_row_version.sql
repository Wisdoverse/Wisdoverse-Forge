-- Monotonic optimistic-concurrency token for orchestration task mutations.
-- Status alone cannot distinguish an ABA transition such as
-- canceled -> retried -> canceled; transaction timestamps are not a logical
-- version. The database owns this counter so every writer participates.

ALTER TABLE orchestration_tasks
    ADD COLUMN row_version BIGINT NOT NULL DEFAULT 0;

CREATE OR REPLACE FUNCTION bump_orchestration_task_row_version()
RETURNS trigger AS $$
BEGIN
    NEW.row_version := OLD.row_version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER orchestration_tasks_row_version
    BEFORE UPDATE ON orchestration_tasks
    FOR EACH ROW EXECUTE FUNCTION bump_orchestration_task_row_version();
