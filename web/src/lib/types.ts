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
