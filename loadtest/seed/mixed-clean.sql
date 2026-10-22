-- Removes the data of mixed.sql (tenants `lt-m-`, their users and check results) and nothing
-- else, in the order of clean.sql: tenants first so the checker stops recording, results after.

\set ON_ERROR_STOP on

CREATE TEMP TABLE lt_mixed_clean AS SELECT id FROM tenants WHERE slug LIKE 'lt-m-%';

BEGIN;
DELETE FROM tenants WHERE id IN (SELECT id FROM lt_mixed_clean);
DELETE FROM users WHERE email LIKE 'lt-m-%@loadtest.taut.test';
COMMIT;

DELETE FROM check_results WHERE tenant_id IN (SELECT id FROM lt_mixed_clean);
DROP TABLE lt_mixed_clean;
