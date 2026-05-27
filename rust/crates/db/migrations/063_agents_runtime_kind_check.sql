-- 063: add enum CHECK and joint-invariant CHECK to agents.runtime_kind.
--
-- Two-phase per FAANG-scale Postgres convention:
--   NOT VALID lands the constraint instantly under a brief AccessExclusive
--     metadata lock — no full-table scan.
--   VALIDATE then scans without blocking writes (ShareUpdateExclusive only).
--
-- This migration must land ONLY AFTER every running API instance is on the
-- new release that writes `runtime_kind` on every INSERT. See
-- docs/runbooks/migration-062-runtime-kind.md.

SET lock_timeout      = '3s';
SET statement_timeout = '30s';

ALTER TABLE agents
    ADD CONSTRAINT agents_runtime_kind_check
    CHECK (runtime_kind IN ('container', 'cli', 'api')) NOT VALID;
ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_check;

ALTER TABLE agents
    ADD CONSTRAINT agents_runtime_kind_invariants
    CHECK (
        (runtime_kind = 'container' AND cli_tool IS NOT NULL)
        OR (runtime_kind = 'cli'    AND cli_tool IS NOT NULL AND container_id IS NULL)
        OR (runtime_kind = 'api'    AND cli_tool IS NULL    AND container_id IS NULL)
    ) NOT VALID;
ALTER TABLE agents VALIDATE CONSTRAINT agents_runtime_kind_invariants;
