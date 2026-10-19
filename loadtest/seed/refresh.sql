-- After seeding or cleaning: materialize the daily uptime of the whole history (the policy only
-- refreshes the last three days) and update the planner statistics, so the first run is not
-- measured with stale estimates. Neither statement can run inside a transaction.

\set ON_ERROR_STOP on

CALL refresh_continuous_aggregate('uptime_daily', NULL, NULL);
VACUUM ANALYZE check_results;
ANALYZE monitors;
ANALYZE monitor_status;
ANALYZE incidents;
ANALYZE tenants;

SELECT count(*) AS check_results FROM check_results
WHERE tenant_id IN (SELECT id FROM tenants WHERE slug LIKE 'lt-%');
