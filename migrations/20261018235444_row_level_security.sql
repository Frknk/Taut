-- Row-level security per tenant (ADR 0010): a second line of defense under the
-- `WHERE tenant_id = $1` every query already has, so a query that forgets it returns nothing
-- instead of another tenant's rows.
--
-- The services act as `taut_app`, a role without login and without BYPASSRLS, inside a
-- transaction that sets `app.tenant_id` (taut_db::begin_tenant). The api's pool connects as
-- that role from the start, so outside such a transaction it sees no tenant data at all. The
-- owner keeps bypassing RLS (no FORCE): the scheduler, auth, migrations and TimescaleDB's jobs
-- work across tenants by design.

-- Roles belong to the whole cluster, and the test databases migrate in parallel: a role that
-- already exists, or that another migration creates at the same moment, is not an error.
DO $$
BEGIN
    CREATE ROLE taut_app NOLOGIN NOSUPERUSER NOBYPASSRLS;
EXCEPTION WHEN duplicate_object OR unique_violation THEN
    NULL;
END
$$;

-- An owner that is not a superuser (a managed database) needs membership to switch to it.
DO $$
BEGIN
    IF NOT pg_has_role(current_user, 'taut_app', 'SET') THEN
        GRANT taut_app TO CURRENT_USER;
    END IF;
EXCEPTION WHEN unique_violation THEN
    NULL;
END
$$;

-- The tenant of the current transaction; NULL outside one. After a transaction that set it,
-- the setting reads '' rather than NULL, hence the nullif.
CREATE FUNCTION current_tenant_id() RETURNS uuid
    LANGUAGE sql STABLE PARALLEL SAFE
    AS $$ SELECT nullif(current_setting('app.tenant_id', true), '')::uuid $$;

-- Explicit grants, no default privileges: a new table stays closed to the services (and its
-- tests fail) until its migration grants it and gives it a policy.
GRANT SELECT, INSERT, UPDATE, DELETE
    ON tenants, memberships, invitations, api_keys, monitors, monitor_status, check_results,
       incidents, alert_channels, notification_deliveries
    TO taut_app;
-- Members and invitations show names and emails; hashes and second-factor secrets stay out.
GRANT SELECT (id, email, name) ON users TO taut_app;

ALTER TABLE tenants ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON tenants TO taut_app
    USING (id = current_tenant_id());

ALTER TABLE memberships ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON memberships TO taut_app
    USING (tenant_id = current_tenant_id());

ALTER TABLE invitations ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON invitations TO taut_app
    USING (tenant_id = current_tenant_id());

ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON api_keys TO taut_app
    USING (tenant_id = current_tenant_id());

ALTER TABLE monitors ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON monitors TO taut_app
    USING (tenant_id = current_tenant_id());

ALTER TABLE monitor_status ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON monitor_status TO taut_app
    USING (tenant_id = current_tenant_id());

-- A hypertable: the policy stays on the parent and applies to every chunk read or written
-- through it, including the chunks an insert creates.
ALTER TABLE check_results ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON check_results TO taut_app
    USING (tenant_id = current_tenant_id());

ALTER TABLE incidents ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON incidents TO taut_app
    USING (tenant_id = current_tenant_id());

ALTER TABLE alert_channels ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON alert_channels TO taut_app
    USING (tenant_id = current_tenant_id());

ALTER TABLE notification_deliveries ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON notification_deliveries TO taut_app
    USING (tenant_id = current_tenant_id());

-- TimescaleDB does not support RLS on continuous aggregates, and reading one needs SELECT on
-- it (its watermark check), which would expose every tenant. `uptime_daily` stays closed to
-- taut_app; this function reads it with the owner's rights, only for the current tenant.
-- Recreating the aggregate later needs RLS off on check_results within that migration.
CREATE FUNCTION uptime_daily_of_current_tenant(since timestamptz)
    RETURNS TABLE (monitor_id uuid, day timestamptz, checks bigint, failures bigint)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path = public, pg_temp
    AS $$
        SELECT u.monitor_id, u.day, u.checks, u.failures
        FROM uptime_daily u
        WHERE u.tenant_id = current_tenant_id() AND u.day >= since
    $$;
REVOKE ALL ON FUNCTION uptime_daily_of_current_tenant(timestamptz) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION uptime_daily_of_current_tenant(timestamptz) TO taut_app;
