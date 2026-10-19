-- Load-test data for checker-soak.js: many monitors checked every 30 s and nothing else. Run it
-- with `just loadtest-seed soak`, which removes the previous load-test data first (clean.sql),
-- so the base data set and this one never mix: the soak measures the checking pipeline alone.
--
-- Tenants respect the free-plan quota of 10 monitors (services/api/src/tenant.rs), so 2 000
-- monitors need 200 tenants. No users and no history: the scheduler only reads `monitors`, and
-- picks the new rows up at its next resync (TAUT_SCHEDULER__RESYNC_SECS, 30 s by default),
-- spread over the interval by monitor id.
--
-- Variables: soak_tenants, soak_monitors, soak_interval_s, echo_url.

\set ON_ERROR_STOP on
SET synchronous_commit = off;

BEGIN;

CREATE TEMP TABLE lt_soak ON COMMIT DROP AS
SELECT gen_random_uuid() AS id, format('lt-k-%s', lpad(n::text, 4, '0')) AS slug
FROM generate_series(1, :soak_tenants) AS n;

INSERT INTO tenants (id, slug, name)
SELECT id, slug, format('Soak %s', slug) FROM lt_soak;

INSERT INTO monitors (id, tenant_id, name, kind, target, interval_s, timeout_s)
SELECT gen_random_uuid(), t.id, format('Soak %s', m), 'http', :'echo_url', :soak_interval_s, 10
FROM lt_soak AS t
CROSS JOIN generate_series(1, :soak_monitors) AS m;

COMMIT;

SELECT count(*) AS soak_tenants,
       (SELECT count(*) FROM monitors m JOIN tenants t ON t.id = m.tenant_id
        WHERE t.slug LIKE 'lt-k-%') AS soak_monitors
FROM tenants WHERE slug LIKE 'lt-k-%';
