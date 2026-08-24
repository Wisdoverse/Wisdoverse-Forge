-- Team invites: let a team lead invite a person who does not have an account
-- yet. The invite link carries a one-time token; redeeming it (after register
-- or login with the matching email) adds the org + team memberships.
CREATE TABLE IF NOT EXISTS team_invites (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id uuid NOT NULL REFERENCES organizations(id),
    team_id uuid NOT NULL REFERENCES teams(id),
    email text NOT NULL,
    role text NOT NULL DEFAULT 'member',
    token_hash text NOT NULL,
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    accepted_at timestamptz,
    UNIQUE (team_id, email)
);

CREATE INDEX IF NOT EXISTS team_invites_token_hash_idx
    ON team_invites (token_hash);
