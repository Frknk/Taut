-- Tenant API keys (identity PR 4): `Authorization: Bearer taut_live_…` for scripts and CI.
-- Only the SHA-256 of the key is stored; `prefix` keeps its first characters so the panel can
-- tell keys apart. A revoked key keeps its row (revoked_at set) for the audit trail.

CREATE TABLE api_keys (
    id           uuid PRIMARY KEY,
    tenant_id    uuid NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    name         text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    prefix       text NOT NULL,
    key_hash     text NOT NULL UNIQUE,
    scope        text NOT NULL CHECK (scope IN ('read', 'write')),
    -- The owner who created it; the key outlives their account.
    created_by   uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    -- Written at most once a minute per key, outside the request.
    last_used_at timestamptz,
    revoked_at   timestamptz
);

CREATE INDEX api_keys_tenant_id_created_at_idx ON api_keys (tenant_id, created_at);
