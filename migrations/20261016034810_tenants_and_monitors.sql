-- Tenants and monitors: the control plane of prototype 1 (docs/prototipo-1.md).
-- Enumerations are text with CHECK constraints instead of PostgreSQL enums, so adding a
-- value later (the DNS monitor of week 12) is a plain constraint swap inside a transaction.

CREATE TABLE tenants (
    id         uuid PRIMARY KEY,
    -- Public URL of the status page; lowercase letters, digits and inner hyphens.
    slug       text NOT NULL UNIQUE
               CHECK (slug ~ '^[a-z0-9][a-z0-9-]{1,38}[a-z0-9]$'),
    name       text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    plan       text NOT NULL DEFAULT 'free' CHECK (plan IN ('free')),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE monitors (
    id                    uuid PRIMARY KEY,
    tenant_id             uuid NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    name                  text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    kind                  text NOT NULL CHECK (kind IN ('http', 'keyword', 'tcp', 'heartbeat')),
    -- URL for http/keyword, host:port for tcp, NULL for heartbeat.
    target                text CHECK (char_length(target) <= 2048),
    interval_s            integer NOT NULL DEFAULT 60 CHECK (interval_s IN (30, 60, 180, 300)),
    timeout_s             integer NOT NULL DEFAULT 10
                          CHECK (timeout_s BETWEEN 1 AND 60 AND timeout_s < interval_s),
    -- NULL means any 2xx.
    expected_status       smallint CHECK (expected_status BETWEEN 100 AND 599),
    keyword               text CHECK (char_length(keyword) BETWEEN 1 AND 200),
    confirmation_period_s integer NOT NULL DEFAULT 60
                          CHECK (confirmation_period_s BETWEEN 0 AND 3600),
    recovery_period_s     integer NOT NULL DEFAULT 120
                          CHECK (recovery_period_s BETWEEN 0 AND 3600),
    -- Heartbeats only: secret path segment of the ping URL, extra silence tolerated after
    -- the interval, and the last ping received.
    heartbeat_token       text UNIQUE,
    grace_period_s        integer NOT NULL DEFAULT 60 CHECK (grace_period_s BETWEEN 0 AND 3600),
    last_ping_at          timestamptz,
    active                boolean NOT NULL DEFAULT true,
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT monitors_target_matches_kind CHECK ((kind = 'heartbeat') = (target IS NULL)),
    CONSTRAINT monitors_token_matches_kind CHECK ((kind = 'heartbeat') = (heartbeat_token IS NOT NULL)),
    CONSTRAINT monitors_keyword_matches_kind CHECK ((kind = 'keyword') = (keyword IS NOT NULL))
);

CREATE INDEX monitors_tenant_id_created_at_idx ON monitors (tenant_id, created_at);
