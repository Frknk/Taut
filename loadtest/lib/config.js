// Load-test configuration. Everything comes from environment variables (`-e NAME=value` or the
// k6 service in compose.yaml) with defaults for the local stack. BASE_URL fills the three URLs at
// once when a single host routes everything (staging behind an ingress).

const env = __ENV;

/** Reads a positive integer variable, failing fast on typos instead of running a wrong test. */
export function intVar(name, fallback) {
  const raw = env[name];
  if (raw === undefined || raw === '') return fallback;
  const value = Number(raw);
  if (!Number.isInteger(value) || value <= 0) {
    throw new Error(`${name} must be a positive integer, got "${raw}"`);
  }
  return value;
}

/** Reads a string variable restricted to a set of values. */
export function enumVar(name, allowed, fallback) {
  const value = env[name] || fallback;
  if (!allowed.includes(value)) {
    throw new Error(`${name} must be one of ${allowed.join(', ')}, got "${value}"`);
  }
  return value;
}

const base = env.BASE_URL;

export const API_URL = env.API_URL || base || 'http://localhost:8080';
export const AUTH_URL = env.AUTH_URL || base || 'http://localhost:8086';
export const STATUS_URL = env.STATUS_URL || base || 'http://localhost:8085';

/** Target of the monitors the tests create; it must answer 200 and be reachable by the checker. */
export const ECHO_URL = env.ECHO_URL || 'http://echo/';

/** `name=url` pairs whose `/ready` the smoke test checks. */
export const SERVICE_URLS = (env.SERVICE_URLS || '')
  .split(',')
  .map((pair) => pair.trim())
  .filter(Boolean)
  .map((pair) => {
    const [name, url] = pair.split('=');
    return { name, url };
  });

/** Password of every seeded user; the seed stores its Argon2id hash (loadtest/seed/base.sql). */
export const PASSWORD = env.LT_PASSWORD || 'loadtest-only-password';

/** Seed sizes; `just loadtest` passes the same values it gave `just loadtest-seed`. */
export const STATUS_TENANTS = intVar('LT_STATUS_TENANTS', 20);
export const STATUS_MONITORS = intVar('LT_STATUS_MONITORS', 10);
export const PANEL_TENANTS = intVar('LT_PANEL_TENANTS', 50);
export const PANEL_MONITORS = intVar('LT_PANEL_MONITORS', 5);

/** Days of history the status page renders (services/status-bff/src/page.rs). */
export const STATUS_DAYS = 90;

/** Free-plan budget of panel requests per tenant and minute (services/api/src/tenant.rs). */
export const PANEL_BUDGET_PER_MINUTE = 600;

/** Free-plan monitor quota per tenant (services/api/src/tenant.rs). */
export const MONITOR_QUOTA = 10;

const EMAIL_DOMAIN = 'loadtest.taut.test';

function pad(n) {
  return String(n).padStart(2, '0');
}

/** Slug of the n-th (1-based) status-page tenant: lt-s-01. */
export function statusSlug(n) {
  return `lt-s-${pad(n)}`;
}

/** Slug of the n-th (1-based) panel tenant: lt-p-01. */
export function panelSlug(n) {
  return `lt-p-${pad(n)}`;
}

/** Email of the owner of a seeded tenant. */
export function ownerEmail(slug) {
  return `${slug}@${EMAIL_DOMAIN}`;
}

/** Prometheus, for scenarios that check server-side counters (rate-limit, checker-soak). */
export const PROM_URL = env.PROM_URL || 'http://localhost:9090';

/** Tenants of the rate-limit scenario: A is flooded, B is the control (loadtest/seed/base.sql). */
export const RATE_LIMIT_SLUGS = { flooded: 'lt-rl-a', control: 'lt-rl-b' };

/** Attacker/victim pairs of the isolation scenario: lt-i-a1 tries to reach lt-i-b1, and so on. */
export const ISOLATION_PAIRS = [1, 2, 3, 4].map((n) => ({
  attacker: `lt-i-a${n}`,
  victim: `lt-i-b${n}`,
}));

/** Free-plan budget of API-key requests per tenant and minute (services/api/src/tenant.rs). */
export const API_KEY_BUDGET_PER_MINUTE = 60;

/** Prefix of the monitors the tests create and delete; teardown removes leftovers by it. */
export const TEMP_MONITOR_PREFIX = 'lt-tmp-';
