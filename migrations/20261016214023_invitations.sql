-- Invitations (identity PR 3): an owner invites an address to their tenant with a role; the
-- link in the email carries a secret whose SHA-256 is stored here. A registered user whose
-- email matches accepts it and becomes a member. Business table: it belongs to the tenant and
-- goes with it.

CREATE TABLE invitations (
    id          uuid PRIMARY KEY,
    tenant_id   uuid NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- Stored trimmed and lowercased, like users.email, so the match on accept is exact.
    email       text NOT NULL
                CHECK (email = lower(email) AND char_length(email) BETWEEN 3 AND 254),
    role        text NOT NULL CHECK (role IN ('owner', 'member')),
    token_hash  text NOT NULL UNIQUE,
    invited_by  uuid REFERENCES users (id) ON DELETE SET NULL,
    expires_at  timestamptz NOT NULL,
    accepted_at timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now()
);

-- One pending invitation per address and tenant: inviting again replaces it.
CREATE UNIQUE INDEX invitations_one_pending_per_email
    ON invitations (tenant_id, email) WHERE accepted_at IS NULL;
