-- Removes every load-test row: tenants with an "lt-" slug (monitors, statuses, incidents,
-- memberships and API keys go with them by cascade), their users, and their check results, which
-- have no foreign key and are deleted by tenant. Deleting by tenant also catches the results of
-- monitors a scenario created and deleted. Run it with `just loadtest-clean`.
--
-- The order matters: the seeded monitors keep being checked while this runs, and the incident
-- service records a result only while its monitor exists. So the tenants (and with them the
-- monitors) go first, in their own transaction, and their results after that; deleting the
-- results first would leave behind the ones recorded in between.

\set ON_ERROR_STOP on

CREATE TEMP TABLE lt_clean AS SELECT id FROM tenants WHERE slug LIKE 'lt-%';

BEGIN;
DELETE FROM tenants WHERE id IN (SELECT id FROM lt_clean);
DELETE FROM users WHERE email LIKE '%@loadtest.taut.test';
COMMIT;

DELETE FROM check_results WHERE tenant_id IN (SELECT id FROM lt_clean);
DROP TABLE lt_clean;
