-- Migration 015: Align foreign-key ON DELETE policies for agents(id).
--
-- Context: DELETE /admin/agents/:id has been returning 500 because the FKs
-- pointing at agents(id) default to NO ACTION, which aborts the DELETE as
-- soon as any child row exists. Every active agent has events, so the
-- endpoint is 100% broken for any non-trivial agent.
--
-- This migration adds explicit ON DELETE actions sized to each relationship:
--
--   events.agent_id              -> CASCADE  (tightly-owned child rows;
--                                             pre-launch product has no audit
--                                             contract that requires keeping
--                                             events for deleted agents)
--   participants.agent_id        -> CASCADE  (participant identity has no
--                                             meaning without its agent)
--   agent_collaborators.agent_id -> CASCADE  (permission grants have no
--                                             meaning without the agent)
--   orchestration_tasks.assigned_agent_id -> SET NULL (tasks are independent
--                                             audit units; keep them)
--   attachments.agent_id         -> SET NULL (attachments can outlive the
--                                             agent; column already nullable)
--
-- agent_plugins.agent_id already has ON DELETE CASCADE from migration 014;
-- no change.
--
-- Rollback: see docs/superpowers/plans/2026-04-18-agent-delete-fk-minimal.md
-- ("Rollback Procedure" section).
--
-- Forward-only: sqlx records a checksum on first apply. Any correction to
-- this migration must land as a new migration (016+); editing this file
-- after it has applied on staging/prod will cause sqlx checksum mismatch.
--
-- Operational note: ADD CONSTRAINT ... NOT VALID (below) takes a brief
-- ACCESS EXCLUSIVE lock for metadata only. The subsequent VALIDATE
-- CONSTRAINT runs outside the transaction under SHARE UPDATE EXCLUSIVE
-- and allows concurrent reads and writes during the table scan.

BEGIN;

ALTER TABLE events
    DROP CONSTRAINT events_agent_id_fkey,
    ADD  CONSTRAINT events_agent_id_fkey
        FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE NOT VALID;

ALTER TABLE participants
    DROP CONSTRAINT participants_agent_id_fkey,
    ADD  CONSTRAINT participants_agent_id_fkey
        FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE NOT VALID;

ALTER TABLE agent_collaborators
    DROP CONSTRAINT agent_collaborators_agent_id_fkey,
    ADD  CONSTRAINT agent_collaborators_agent_id_fkey
        FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE NOT VALID;

ALTER TABLE orchestration_tasks
    DROP CONSTRAINT orchestration_tasks_assigned_agent_id_fkey,
    ADD  CONSTRAINT orchestration_tasks_assigned_agent_id_fkey
        FOREIGN KEY (assigned_agent_id) REFERENCES agents(id) ON DELETE SET NULL NOT VALID;

ALTER TABLE attachments
    DROP CONSTRAINT attachments_agent_id_fkey,
    ADD  CONSTRAINT attachments_agent_id_fkey
        FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE SET NULL NOT VALID;

COMMIT;

-- Validate outside the transaction so each VALIDATE takes only SHARE UPDATE
-- EXCLUSIVE (reads + writes allowed) instead of ACCESS EXCLUSIVE (blocks all).
ALTER TABLE events              VALIDATE CONSTRAINT events_agent_id_fkey;
ALTER TABLE participants        VALIDATE CONSTRAINT participants_agent_id_fkey;
ALTER TABLE agent_collaborators VALIDATE CONSTRAINT agent_collaborators_agent_id_fkey;
ALTER TABLE orchestration_tasks VALIDATE CONSTRAINT orchestration_tasks_assigned_agent_id_fkey;
ALTER TABLE attachments         VALIDATE CONSTRAINT attachments_agent_id_fkey;

-- Assert the post-migration pg_constraint state matches intent. Catches the
-- case where a future edit weakens the DROP to IF EXISTS, or where Postgres
-- records a different ON DELETE action than requested, or where the multi-
-- schema lookup 'agents'::regclass resolved unexpectedly.
DO $$
DECLARE
    expected_count INT := 5;
    actual_correct INT;
BEGIN
    SELECT COUNT(*) INTO actual_correct
    FROM pg_constraint c
    JOIN pg_class t ON c.conrelid = t.oid
    WHERE c.contype = 'f'
      AND c.confrelid = 'agents'::regclass
      AND (
        (t.relname = 'events'              AND c.confdeltype = 'c') OR
        (t.relname = 'participants'        AND c.confdeltype = 'c') OR
        (t.relname = 'agent_collaborators' AND c.confdeltype = 'c') OR
        (t.relname = 'orchestration_tasks' AND c.confdeltype = 'n') OR
        (t.relname = 'attachments'         AND c.confdeltype = 'n')
      );
    IF actual_correct <> expected_count THEN
        RAISE EXCEPTION
            'migration 015: expected % correct FK policies, found %',
            expected_count, actual_correct;
    END IF;
END $$;
