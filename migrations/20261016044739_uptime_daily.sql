-- Daily uptime per monitor (F-26, F-46): a TimescaleDB continuous aggregate over
-- check_results instead of a table maintained by hand (docs/adr/0002). It is a projection:
-- dropping it and refreshing rebuilds it from the raw results.
--
-- materialized_only = false turns on real-time aggregation: the days not refreshed yet are
-- computed from the raw rows at query time, so today's bar is always current. WITH NO DATA
-- keeps the creation inside the migration's transaction; the policy fills it in the
-- background.

CREATE MATERIALIZED VIEW uptime_daily
WITH (timescaledb.continuous, timescaledb.materialized_only = false) AS
SELECT monitor_id,
       tenant_id,
       time_bucket(INTERVAL '1 day', at) AS day,
       count(*) AS checks,
       count(*) FILTER (WHERE NOT ok) AS failures,
       avg(latency_ms) FILTER (WHERE ok) AS avg_latency_ms
FROM check_results
GROUP BY monitor_id, tenant_id, time_bucket(INTERVAL '1 day', at)
WITH NO DATA;

-- Refresh the last three days every 30 minutes; older days do not change. The raw results
-- expire after 90 days, which is exactly the window the status page shows.
SELECT add_continuous_aggregate_policy(
    'uptime_daily',
    start_offset => INTERVAL '3 days',
    end_offset => INTERVAL '1 hour',
    schedule_interval => INTERVAL '30 minutes'
);
