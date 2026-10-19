-- Load-test data (loadtest/README.md). Run it with `just loadtest-seed`, which removes the
-- previous data first (clean.sql), passes the sizes as psql variables and refreshes the
-- aggregates afterwards (refresh.sql). Everything it creates is easy to find and delete:
-- slugs start with "lt-" and emails end in "@loadtest.taut.test".
--
-- It runs as the owner role `taut`, which bypasses row-level security (no FORCE, see
-- migrations/20261018235444_row_level_security.sql), and goes around the api on purpose:
-- registering needs a verified email and auth allows 10 registrations per IP and hour.
--
-- Variables: status_tenants, status_monitors, status_days, status_step_min, panel_tenants,
-- panel_monitors, panel_days, panel_step_min, echo_url. A step is also the interval of the
-- monitors, so it must be 1 or 5 minutes (intervals of 60 or 300 s).

\set ON_ERROR_STOP on
SET synchronous_commit = off;

BEGIN;

-- Status-page tenants (lt-s-NN) show many days; panel tenants (lt-p-NN) get finer history.
CREATE TEMP TABLE lt_tenants ON COMMIT DROP AS
SELECT gen_random_uuid() AS id, format('lt-s-%s', lpad(n::text, 2, '0')) AS slug, 'status' AS role,
       :status_monitors AS monitors, :status_days AS days, :status_step_min AS step_min
FROM generate_series(1, :status_tenants) AS n
UNION ALL
SELECT gen_random_uuid(), format('lt-p-%s', lpad(n::text, 2, '0')), 'panel',
       :panel_monitors, :panel_days, :panel_step_min
FROM generate_series(1, :panel_tenants) AS n;

INSERT INTO tenants (id, slug, name)
SELECT id, slug, format('Carga %s', slug) FROM lt_tenants;

-- Argon2id (m=19456, t=2, p=1: the parameters of Argon2::default() in services/auth) of the
-- dev-only password "loadtest-only-password" (LT_PASSWORD in loadtest/lib/config.js). It was
-- generated once with the argon2 0.6 crate; any PHC string with valid parameters verifies.
INSERT INTO users (id, email, name, password_hash, email_verified_at)
SELECT gen_random_uuid(), slug || '@loadtest.taut.test', format('Carga %s', slug),
       '$argon2id$v=19$m=19456,t=2,p=1$/CujHpmee/171q3coWAPQg$MsamTTfdUcXImAgLzqlzhFk56fSgnpb6VaOwlMdkW58',
       now()
FROM lt_tenants;

INSERT INTO memberships (user_id, tenant_id, role)
SELECT u.id, t.id, 'owner'
FROM lt_tenants AS t
JOIN users AS u ON u.email = t.slug || '@loadtest.taut.test';

-- HTTP monitors against the echo server, created as if 90 days ago so the history fits.
CREATE TEMP TABLE lt_monitors ON COMMIT DROP AS
SELECT gen_random_uuid() AS id, t.id AS tenant_id, m AS n, t.days, t.step_min
FROM lt_tenants AS t
CROSS JOIN LATERAL generate_series(1, t.monitors) AS m;

INSERT INTO monitors (id, tenant_id, name, kind, target, interval_s, timeout_s, created_at, updated_at)
SELECT id, tenant_id, format('Servicio %s', n), 'http', :'echo_url', step_min * 60, 10,
       now() - INTERVAL '90 days' + n * INTERVAL '1 second', now()
FROM lt_monitors;

INSERT INTO monitor_status (monitor_id, tenant_id, status, consecutive_successes, last_checked_at)
SELECT id, tenant_id, 'up', 1, now() FROM lt_monitors;

-- History: one result per step, up to the previous minute; the offset per monitor keeps them
-- from sharing timestamps. About 0.3 % fail; latencies are mostly 40-120 ms with a slow tail.
-- Nothing is older than 89 days, so the 90-day retention never drops rows mid-run.
INSERT INTO check_results (monitor_id, tenant_id, at, region, ok, status, latency_ms, error)
SELECT monitor_id, tenant_id, at, 'local', ok,
       CASE WHEN ok THEN 200 ELSE 503 END,
       CASE WHEN ok THEN (40 + r * 80 + CASE WHEN r > 0.98 THEN r * 600 ELSE 0 END)::int ELSE 30 END,
       CASE WHEN ok THEN NULL ELSE 'unexpected status 503' END
FROM (
    SELECT m.id AS monitor_id, m.tenant_id, s.at, random() > 0.003 AS ok, random() AS r
    FROM lt_monitors AS m
    CROSS JOIN LATERAL generate_series(
        date_trunc('minute', now()) - make_interval(days => least(m.days, 89)) + (m.n % 60) * INTERVAL '1 second',
        date_trunc('minute', now()) - INTERVAL '1 minute',
        make_interval(mins => m.step_min)
    ) AS s(at)
) AS rows;

-- Three resolved incidents per monitor: one in the last week (the status page lists those) and
-- two older ones (the panel's incident list).
INSERT INTO incidents (id, tenant_id, monitor_id, started_at, resolved_at, cause)
SELECT gen_random_uuid(), m.tenant_id, m.id, started, started + (2 + random() * 28) * INTERVAL '1 minute',
       'unexpected status 503'
FROM lt_monitors AS m
CROSS JOIN LATERAL (VALUES
    (now() - (1 + random() * 6) * INTERVAL '1 day'),
    (now() - (8 + random() * 10) * INTERVAL '1 day'),
    (now() - (20 + random() * 9) * INTERVAL '1 day')
) AS i(started);

COMMIT;

SELECT role, count(*) AS tenants, sum(monitors) AS monitors FROM (
    SELECT CASE WHEN slug LIKE 'lt-s-%' THEN 'status' ELSE 'panel' END AS role,
           (SELECT count(*) FROM monitors WHERE tenant_id = t.id) AS monitors
    FROM tenants AS t WHERE slug LIKE 'lt-%'
) AS seeded GROUP BY role ORDER BY role;
