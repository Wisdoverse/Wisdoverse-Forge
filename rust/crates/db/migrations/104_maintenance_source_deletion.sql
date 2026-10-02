-- Preserve tenant ownership while allowing explicit task deletion to remove
-- its source record. Correct migration 101 without changing its checksum.
ALTER TABLE maintenance_requests
    DROP CONSTRAINT IF EXISTS maintenance_requests_organization_id_fkey,
    DROP CONSTRAINT IF EXISTS maintenance_requests_organization_id_task_id_fkey,
    ADD CONSTRAINT maintenance_requests_organization_id_fkey
        FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE,
    ADD CONSTRAINT maintenance_requests_organization_id_task_id_fkey
        FOREIGN KEY (organization_id, task_id)
        REFERENCES orchestration_tasks(organization_id, id) ON DELETE CASCADE;
