// Shapes of the taut-api JSON responses. Field names follow the API (snake_case).

export type MonitorKind = 'http' | 'keyword' | 'tcp' | 'heartbeat';
export type MonitorStatus = 'pending' | 'up' | 'down' | 'paused';

export interface Tenant {
  id: string;
  slug: string;
  name: string;
  plan: string;
  created_at: string;
}

export interface Monitor {
  id: string;
  tenant_id: string;
  name: string;
  type: MonitorKind;
  target: string | null;
  interval_s: number;
  timeout_s: number;
  expected_status: number | null;
  keyword: string | null;
  confirmation_period_s: number;
  recovery_period_s: number;
  grace_period_s: number;
  heartbeat_token: string | null;
  last_ping_at: string | null;
  active: boolean;
  status: MonitorStatus;
  last_check_at: string | null;
  last_check_ok: boolean | null;
  last_latency_ms: number | null;
  created_at: string;
  updated_at: string;
}

/** One stored check result (`GET /v1/monitors/{id}/checks`). */
export interface CheckResult {
  at: string;
  region: string;
  ok: boolean;
  status: number | null;
  latency_ms: number | null;
  error: string | null;
}

/** Body of `POST /v1/monitors` and `PUT /v1/monitors/{id}`. */
export interface MonitorInput {
  name: string;
  type: MonitorKind;
  target?: string;
  interval_s: number;
  timeout_s: number;
  expected_status?: number;
  keyword?: string;
  confirmation_period_s: number;
  recovery_period_s: number;
  grace_period_s: number;
}

export type ChannelKind = 'email' | 'webhook' | 'discord' | 'slack';

export interface AlertChannel {
  id: string;
  tenant_id: string;
  name: string;
  type: ChannelKind;
  config: { to?: string; url?: string };
  active: boolean;
  last_delivery_at: string | null;
  last_delivery_error: string | null;
  created_at: string;
}

/** Body of `POST /v1/alert-channels` and `PUT /v1/alert-channels/{id}`. */
export interface ChannelInput {
  name: string;
  type: ChannelKind;
  config: { to: string } | { url: string };
  active: boolean;
}

export interface Incident {
  id: string;
  monitor_id: string;
  monitor_name: string;
  started_at: string;
  resolved_at: string | null;
  cause: string;
  duration_s: number;
}

export interface Delivery {
  channel_id: string;
  channel_name: string;
  channel_type: ChannelKind;
  event: 'opened' | 'resolved';
  status: 'sent' | 'failed';
  attempts: number;
  last_error: string | null;
  updated_at: string;
}

export interface IncidentDetail extends Incident {
  deliveries: Delivery[];
}

export type MetricsRange = '24h' | '7d' | '30d';

export interface MetricsPoint {
  t: string;
  checks: number;
  failures: number;
  avg_ms: number | null;
  p95_ms: number | null;
}

/** Body of `GET /v1/monitors/{id}/metrics`. */
export interface Metrics {
  range: MetricsRange;
  bucket_s: number;
  checks: number;
  failures: number;
  uptime: Record<MetricsRange, number | null>;
  latency: { p50_ms: number | null; p95_ms: number | null; avg_ms: number | null };
  series: MetricsPoint[];
}

export interface DayBar {
  day: string;
  checks: number;
  failures: number;
  uptime: number | null;
}

/** Body of `GET /s/{slug}` on the status BFF. */
export interface StatusPage {
  tenant: { name: string; slug: string };
  status: 'operational' | 'partial_outage' | 'major_outage';
  monitors: { name: string; status: MonitorStatus; uptime_90d: number | null; days: DayBar[] }[];
  incidents: {
    monitor_name: string;
    started_at: string;
    resolved_at: string | null;
    duration_s: number;
  }[];
  generated_at: string;
}
