-- Raw check results (F-01..F-04, F-27): one row per check, append-only, in a TimescaleDB
-- hypertable partitioned by day and kept for 90 days (docs/adr/0002).
-- No foreign key to monitors: the API deletes a monitor's results with it, and an insert
-- for a monitor deleted a moment earlier is skipped by the writer.

CREATE TABLE check_results (
    monitor_id uuid NOT NULL,
    tenant_id  uuid NOT NULL,
    -- When the check started, or when the heartbeat ping arrived.
    at         timestamptz NOT NULL,
    region     text NOT NULL,
    ok         boolean NOT NULL,
    -- HTTP status code, when there was a response.
    status     smallint,
    latency_ms integer,
    -- Readable cause of a failure; NULL when ok.
    error      text,
    trace_id   text,
    -- Identifies a result, so recording a redelivered event is a no-op.
    PRIMARY KEY (monitor_id, at, region)
);

SELECT create_hypertable('check_results', by_range('at', INTERVAL '1 day'));
SELECT add_retention_policy('check_results', INTERVAL '90 days');
