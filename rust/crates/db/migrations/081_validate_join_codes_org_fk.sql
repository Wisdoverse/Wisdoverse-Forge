-- no-transaction
-- F049: validate the agent_join_codes.organization_id FK added NOT VALID in 075.
ALTER TABLE agent_join_codes VALIDATE CONSTRAINT agent_join_codes_organization_id_fkey;
