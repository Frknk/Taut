-- Load-test data for mixed.js: panel tenants whose monitors are also the background checking
-- load. Unlike base.sql it is additive: it only replaces its own `lt-m-` tenants, so it can run
-- on a server that already holds other load-test data (the VPS of the demo). Remove it with
-- mixed-clean.sql; until then the monitors keep being checked.
--
-- Each tenant has an owner who can log in (the password of base.sql) and `mixed_monitors` HTTP
-- monitors at 60 s against `echo_url`, with `mixed_days` days of history at 5 min so the panel
-- metrics have something to aggregate. The free-plan quota is 10 monitors per tenant.
--
-- Variables: mixed_tenants, mixed_monitors, mixed_days, echo_url.

\set ON_ERROR_STOP on
SET synchronous_commit = off;

-- The caller runs mixed-clean.sql right before (psql reads both from stdin, so no \ir).

BEGIN;

CREATE TEMP TABLE lt_mixed ON COMMIT DROP AS
SELECT gen_random_uuid() AS id, format('lt-m-%s', lpad(n::text, 2, '0')) AS slug
FROM generate_series(1, :mixed_tenants) AS n;

INSERT INTO tenants (id, slug, name)
SELECT id, slug, format('Mixta %s', slug) FROM lt_mixed;

-- Same Argon2id hash as base.sql ("loadtest-only-password").
INSERT INTO users (id, email, name, password_hash, email_verified_at)
SELECT gen_random_uuid(), slug || '@loadtest.taut.test', format('Mixta %s', slug),
       '$argon2id$v=19$m=19456,t=2,p=1$/CujHpmee/171q3coWAPQg$MsamTTfdUcXImAgLzqlzhFk56fSgnpb6VaOwlMdkW58',
       now()
FROM lt_mixed;

INSERT INTO memberships (user_id, tenant_id, role)
SELECT u.id, t.id, 'owner'
FROM lt_mixed AS t
JOIN users AS u ON u.email = t.slug || '@loadtest.taut.test';

CREATE TEMP TABLE lt_mixed_monitors ON COMMIT DROP AS
SELECT gen_random_uuid() AS id, t.id AS tenant_id, m AS n
FROM lt_mixed AS t
CROSS JOIN generate_series(1, :mixed_monitors) AS m;

INSERT INTO monitors (id, tenant_id, name, kind, target, interval_s, timeout_s, created_at, updated_at)
SELECT id, tenant_id, format('Servicio %s', n), 'http', :'echo_url', 60, 10,
       now() - make_interval(days => :mixed_days) + n * INTERVAL '1 second', now()
FROM lt_mixed_monitors;

INSERT INTO monitor_status (monitor_id, tenant_id, status, consecutive_successes, last_checked_at)
SELECT id, tenant_id, 'up', 1, now() FROM lt_mixed_monitors;

-- History like base.sql: about 0.3 % failures, latencies mostly 40-120 ms with a slow tail.
INSERT INTO check_results (monitor_id, tenant_id, at, region, ok, status, latency_ms, error)
SELECT monitor_id, tenant_id, at, 'local', ok,
       CASE WHEN ok THEN 200 ELSE 503 END,
       CASE WHEN ok THEN (40 + r * 80 + CASE WHEN r > 0.98 THEN r * 600 ELSE 0 END)::int ELSE 30 END,
       CASE WHEN ok THEN NULL ELSE 'unexpected status 503' END
FROM (
    SELECT m.id AS monitor_id, m.tenant_id, s.at, random() > 0.003 AS ok, random() AS r
    FROM lt_mixed_monitors AS m
    CROSS JOIN LATERAL generate_series(
        date_trunc('minute', now()) - make_interval(days => :mixed_days) + (m.n % 60) * INTERVAL '1 second',
        date_trunc('minute', now()) - INTERVAL '5 minutes',
        INTERVAL '5 minutes'
    ) AS s(at)
) AS rows;

-- Two resolved incidents per monitor, for the panel's incident lists.
INSERT INTO incidents (id, tenant_id, monitor_id, started_at, resolved_at, cause)
SELECT gen_random_uuid(), m.tenant_id, m.id, started, started + (2 + random() * 28) * INTERVAL '1 minute',
       'unexpected status 503'
FROM lt_mixed_monitors AS m
CROSS JOIN LATERAL (VALUES
    (now() - (1 + random() * 2) * INTERVAL '1 day'),
    (now() - (3 + random() * 3) * INTERVAL '1 day')
) AS i(started);

COMMIT;

SELECT count(*) AS mixed_tenants,
       (SELECT count(*) FROM monitors m JOIN tenants t ON t.id = m.tenant_id
        WHERE t.slug LIKE 'lt-m-%') AS mixed_monitors
FROM tenants WHERE slug LIKE 'lt-m-%';
