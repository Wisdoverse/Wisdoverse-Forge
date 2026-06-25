-- 075: schema-hardening constraints (F048 review_status CHECK, F049 tenant FKs).
--
-- Corrective, idempotent, and guarded. Constraints are added `NOT VALID` (no
-- full-table scan under an ACCESS EXCLUSIVE lock) and then `VALIDATE`d
-- separately (SHARE UPDATE EXCLUSIVE, concurrent-write-safe), mirroring the
-- project's existing corrective-constraint pattern.

-- F048: orchestration_tasks.review_status had no CHECK, breaking the schema's
-- otherwise-uniform enum-CHECK discipline. Pin it to the canonical self-fix
-- vocabulary (the `domain::self_fix::review_status` constants) so a typo'd state
-- ('aproved') cannot silently strand a self-fix PR's merge eligibility. NULL is
-- the resting state for non-self-fix tasks.
--
-- The vocabulary is the UNION of the two writers: the orchestrator `ReviewState`
-- (`pending`, `in_review`, `approved`, `changes_requested`, `rejected` — the
-- `#[serde(rename_all = "snake_case")]` enum the column mirrors) PLUS the
-- API-side self-fix extras `merged` (post-merge) and `sensitive_blocked`
-- (CODEOWNERS-routed). All seven are written by `set_review_status` /
-- `set_pr_metadata` callers or existing tests, so the CHECK must admit every one
-- (and any already-stored value passes VALIDATE).
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'orchestration_tasks_review_status_check'
    ) THEN
        ALTER TABLE orchestration_tasks
            ADD CONSTRAINT orchestration_tasks_review_status_check
            CHECK (
                review_status IS NULL
                OR review_status IN (
                    'pending', 'in_review', 'approved', 'changes_requested', 'rejected', 'merged', 'sensitive_blocked'
                )
            ) NOT VALID;
    END IF;
END $$;

-- F049: enrollment_idempotency and agent_join_codes carry tenant-boundary
-- columns with no FK to organizations(id)/users(id), so a row can outlive the
-- org/user it names. These tables are ephemeral (TTL-expiring), so orphaned rows
-- (referencing an already-deleted org/user) are dead garbage — delete them first
-- so the subsequent VALIDATE cannot fail on pre-existing drift, then add the FKs
-- ON DELETE CASCADE so future org/user deletion cleans them automatically.
DO $$
DECLARE
    orphans bigint;
BEGIN
    DELETE FROM enrollment_idempotency e
        WHERE NOT EXISTS (SELECT 1 FROM organizations o WHERE o.id = e.org_id)
           OR NOT EXISTS (SELECT 1 FROM users u WHERE u.id = e.user_id);
    GET DIAGNOSTICS orphans = ROW_COUNT;
    RAISE NOTICE 'F049: removed % orphaned enrollment_idempotency row(s)', orphans;

    DELETE FROM agent_join_codes a
        WHERE NOT EXISTS (SELECT 1 FROM organizations o WHERE o.id = a.organization_id);
    GET DIAGNOSTICS orphans = ROW_COUNT;
    RAISE NOTICE 'F049: removed % orphaned agent_join_codes row(s)', orphans;

    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'enrollment_idempotency_org_id_fkey') THEN
        ALTER TABLE enrollment_idempotency
            ADD CONSTRAINT enrollment_idempotency_org_id_fkey
            FOREIGN KEY (org_id) REFERENCES organizations(id) ON DELETE CASCADE NOT VALID;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'enrollment_idempotency_user_id_fkey') THEN
        ALTER TABLE enrollment_idempotency
            ADD CONSTRAINT enrollment_idempotency_user_id_fkey
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE NOT VALID;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'agent_join_codes_organization_id_fkey') THEN
        ALTER TABLE agent_join_codes
            ADD CONSTRAINT agent_join_codes_organization_id_fkey
            FOREIGN KEY (organization_id) REFERENCES organizations(id) ON DELETE CASCADE NOT VALID;
    END IF;
END $$;

-- VALIDATE of every constraint added above is deferred to separate single-
-- statement `-- no-transaction` migrations (078-081), so the brief
-- ACCESS EXCLUSIVE lock from each `ADD CONSTRAINT ... NOT VALID` here is released
-- at this migration's commit instead of being held through the validation table
-- scans (which would turn online validation into a blocking migration). This
-- mirrors the repo's 016 (add NOT VALID) -> 018/019 (VALIDATE) pattern.
