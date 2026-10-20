-- After seeding or cleaning: materialize the daily uptime of the whole history (the policy only
-- refreshes the last three days) and update the planner statistics, so the first run is not
-- measured with stale estimates. Neither statement can run inside a transaction.
--
-- The window ends where the policy's does (end_offset of 1 hour), so today stays in the
-- real-time part of the aggregate, as in production. Ending it at NULL would materialize today
-- too and leave nothing for each page to aggregate from check_results: the runs of 19-10 were
-- measured that way (docs/loadtest/2026-10-20-status-bff-perfil.md).

\set ON_ERROR_STOP on

CALL refresh_continuous_aggregate('uptime_daily', NULL, now() - INTERVAL '1 hour');
VACUUM ANALYZE check_results;
ANALYZE monitors;
ANALYZE monitor_status;
ANALYZE incidents;
ANALYZE tenants;

SELECT count(*) AS check_results FROM check_results
WHERE tenant_id IN (SELECT id FROM tenants WHERE slug LIKE 'lt-%');
