-- Personal credentials belong to the authenticated clone requester. Legacy
-- attempts have no attributable requester and must remain anonymous.
ALTER TABLE project_clone_attempts
    ADD COLUMN requested_by UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN requested_at TIMESTAMPTZ;
