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

export type Role = 'owner' | 'member';

/** A tenant as `GET /v1/tenants` returns it: with the caller's role. */
export interface TenantWithRole extends Tenant {
  role: Role;
}

/** The signed-in user (`GET /v1/auth/me`, `user` of a login). */
export interface User {
  id: string;
  email: string;
  name: string;
  email_verified_at: string | null;
  mfa_enabled: boolean;
  created_at: string;
}

/** Body of a login, refresh or password change. The refresh token travels in a cookie. */
export interface SessionResponse {
  access_token: string;
  token_type: 'Bearer';
  expires_in: number;
  user: User;
}

/** A login whose password was right but whose account asks for a second factor. */
export interface MfaRequired {
  status: 'mfa_required';
  /** Short-lived token for `POST /v1/auth/mfa`; not an access token. */
  mfa_token: string;
}

/** What `POST /v1/auth/login` answers: a session, or the request for the second step. */
export type LoginResponse = SessionResponse | MfaRequired;

/** Body of `POST /v1/auth/mfa/enroll`. */
export interface MfaEnrollment {
  /** Base32 secret, for apps that cannot scan the QR. */
  secret: string;
  /** `otpauth://totp/…`, what the QR encodes. */
  otpauth_uri: string;
}

/** Body of `POST /v1/auth/mfa/confirm` and `/backup-codes`: shown once. */
export interface BackupCodes {
  backup_codes: string[];
}

/** Body of `GET /v1/auth/mfa`. */
export interface MfaStatus {
  enabled: boolean;
  enabled_at: string | null;
  backup_codes_left: number;
}

/** One row of `GET /v1/members`. */
export interface Member {
  user_id: string;
  name: string;
  email: string;
  role: Role;
  joined_at: string;
}

export type ApiKeyScope = 'read' | 'write';

/** A tenant API key as `GET /v1/api-keys` lists it: never the key itself. */
export interface ApiKey {
  id: string;
  name: string;
  /** First characters of the key, to tell keys apart (`taut_live_Ab12cd34`). */
  prefix: string;
  scope: ApiKeyScope;
  created_at: string;
  last_used_at: string | null;
  revoked_at: string | null;
}

/** Body of `POST /v1/api-keys`: the key plus its full value, shown this one time. */
export interface CreatedApiKey extends ApiKey {
  key: string;
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
