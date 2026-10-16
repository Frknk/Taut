-- Incidents and alerts (F-16, F-17, F-32, F-40, F-41): the health state of each monitor, the
-- incidents it opens and resolves, where tenants want alerts and what was delivered.

-- Health of each monitor, written only by the incident service. One row per monitor, locked
-- while a result is applied, so results of the same monitor are processed one at a time.
CREATE TABLE monitor_status (
    monitor_id            uuid PRIMARY KEY REFERENCES monitors (id) ON DELETE CASCADE,
    tenant_id             uuid NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    status                text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'up', 'down')),
    consecutive_failures  integer NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
    consecutive_successes integer NOT NULL DEFAULT 0 CHECK (consecutive_successes >= 0),
    -- First failure of the current streak: the start and the cause of the incident it opens.
    failing_since         timestamptz,
    failure_cause         text,
    -- Instant of the newest result applied; older results arriving late are recorded but
    -- do not move the state.
    last_checked_at       timestamptz,
    updated_at            timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE incidents (
    id          uuid PRIMARY KEY,
    tenant_id   uuid NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    monitor_id  uuid NOT NULL REFERENCES monitors (id) ON DELETE CASCADE,
    started_at  timestamptz NOT NULL,
    -- NULL while the incident is open.
    resolved_at timestamptz,
    cause       text NOT NULL,
    CONSTRAINT incidents_resolved_after_start CHECK (resolved_at IS NULL OR resolved_at >= started_at)
);

-- At most one open incident per monitor, whatever the number of incident service replicas.
CREATE UNIQUE INDEX incidents_one_open_per_monitor ON incidents (monitor_id) WHERE resolved_at IS NULL;
CREATE INDEX incidents_tenant_id_started_at_idx ON incidents (tenant_id, started_at DESC);
CREATE INDEX incidents_monitor_id_started_at_idx ON incidents (monitor_id, started_at DESC);

CREATE TABLE alert_channels (
    id         uuid PRIMARY KEY,
    tenant_id  uuid NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    name       text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    kind       text NOT NULL CHECK (kind IN ('email', 'webhook', 'discord', 'slack')),
    -- {"to": "..."} for email, {"url": "..."} for the webhook kinds.
    config     jsonb NOT NULL,
    active     boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    -- Outcome of the last message sent through the channel (alert or test), written by the
    -- notifier so the panel can show whether the channel works.
    last_delivery_at    timestamptz,
    last_delivery_error text
);

CREATE INDEX alert_channels_tenant_id_idx ON alert_channels (tenant_id);

-- One row per incident transition and channel: makes redelivered events idempotent (a sent
-- alert is never sent again) and is the delivery history shown in the panel.
CREATE TABLE notification_deliveries (
    incident_id   uuid NOT NULL REFERENCES incidents (id) ON DELETE CASCADE,
    channel_id    uuid NOT NULL REFERENCES alert_channels (id) ON DELETE CASCADE,
    event         text NOT NULL CHECK (event IN ('opened', 'resolved')),
    tenant_id     uuid NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    status        text NOT NULL CHECK (status IN ('sent', 'failed')),
    attempts      integer NOT NULL DEFAULT 1,
    last_error    text,
    updated_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (incident_id, channel_id, event)
);
