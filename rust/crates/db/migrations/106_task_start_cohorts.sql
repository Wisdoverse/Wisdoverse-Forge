-- Keep the first start across retries, run retention, and task deletion.
-- No task content or result payload is copied into this table.
CREATE TABLE task_starts (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    task_id UUID NOT NULL,
    first_started_at TIMESTAMPTZ,
    PRIMARY KEY (organization_id, task_id)
);

CREATE INDEX idx_task_starts_org_started ON task_starts (organization_id, first_started_at);

CREATE FUNCTION record_task_first_start() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.status = 'working' OR NEW.started_at IS NOT NULL THEN
        INSERT INTO task_starts (organization_id, task_id, first_started_at)
        VALUES (NEW.organization_id, NEW.id, COALESCE(NEW.started_at, NOW()))
        ON CONFLICT (organization_id, task_id) DO NOTHING;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER orchestration_task_first_start
AFTER INSERT OR UPDATE OF status, started_at ON orchestration_tasks
FOR EACH ROW EXECUTE FUNCTION record_task_first_start();

-- Surviving history is useful, but cannot prove coverage before installation.
INSERT INTO task_starts (organization_id, task_id, first_started_at)
SELECT organization_id, task_id, MIN(started_at)
FROM (
    SELECT organization_id, id AS task_id, started_at
    FROM orchestration_tasks
    WHERE started_at IS NOT NULL OR attempt > 0 OR status = 'working'
    UNION ALL
    SELECT r.organization_id, r.orchestration_task_id, r.started_at
    FROM task_runs r
    JOIN orchestration_tasks t ON t.id = r.orchestration_task_id AND t.organization_id = r.organization_id
    UNION ALL
    SELECT organization_id, aggregate_id, created_at
    FROM orchestration_outbox
    WHERE aggregate_type = 'orchestration_task' AND event_type = 'assignment'
) starts
GROUP BY organization_id, task_id
ON CONFLICT (organization_id, task_id) DO NOTHING;

-- Use wall-clock time after trigger installation, rather than transaction start.
CREATE TABLE task_start_measurement (
    singleton BOOLEAN PRIMARY KEY CHECK (singleton),
    coverage_since TIMESTAMPTZ NOT NULL
);
INSERT INTO task_start_measurement VALUES (TRUE, clock_timestamp());
