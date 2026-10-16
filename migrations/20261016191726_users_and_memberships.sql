-- Identity (weeks 13–14): users, their tenants and the one-time tokens of email flows.
-- users and user_tokens are identity tables, not business tables of a tenant, so they carry no
-- tenant_id (docs/adr/0008). Memberships link a user to any number of tenants with a role.

CREATE TABLE users (
    id                uuid PRIMARY KEY,
    -- Stored trimmed and lowercased; the auth service normalizes before writing.
    email             text NOT NULL
                      CHECK (email = lower(email) AND char_length(email) BETWEEN 3 AND 254),
    name              text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    -- PHC string: $argon2id$v=19$...
    password_hash     text NOT NULL,
    email_verified_at timestamptz,
    -- Bumped when the password changes: every session that started before dies.
    token_version     integer NOT NULL DEFAULT 1,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX users_email_key ON users (email);

CREATE TABLE memberships (
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    tenant_id  uuid NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    role       text NOT NULL CHECK (role IN ('owner', 'member')),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, tenant_id)
);

CREATE INDEX memberships_tenant_id_idx ON memberships (tenant_id);

-- Email verification and password reset links. Only the SHA-256 of the secret is stored.
CREATE TABLE user_tokens (
    id         uuid PRIMARY KEY,
    kind       text NOT NULL CHECK (kind IN ('verify_email', 'reset_password')),
    token_hash text NOT NULL UNIQUE,
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    used_at    timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX user_tokens_user_id_kind_idx ON user_tokens (user_id, kind);
