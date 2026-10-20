-- Daily uptime of some monitors of the transaction's tenant. Today is not materialized yet, so
-- uptime_daily aggregates it from check_results on every read. Filtering only by tenant made
-- that part read the whole day of every tenant (no index starts with tenant_id), and its cost
-- grew with the platform and through the UTC day. With the monitor ids it uses the primary key
-- of check_results (docs/loadtest/2026-10-20-status-bff-perfil.md).
--
-- uptime_daily_of_current_tenant stays for the binaries that still call it; a later migration
-- drops it.
CREATE FUNCTION uptime_daily_of_monitors(since timestamptz, monitors uuid[])
    RETURNS TABLE (monitor_id uuid, day timestamptz, checks bigint, failures bigint)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path = public, pg_temp
    AS $$
        SELECT u.monitor_id, u.day, u.checks, u.failures
        FROM uptime_daily u
        WHERE u.tenant_id = current_tenant_id()
          AND u.monitor_id = ANY (monitors)
          AND u.day >= since
    $$;
REVOKE ALL ON FUNCTION uptime_daily_of_monitors(timestamptz, uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION uptime_daily_of_monitors(timestamptz, uuid[]) TO taut_app;
