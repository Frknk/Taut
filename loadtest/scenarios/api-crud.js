// api-crud: the panel under load. Reads copy what an open monitor detail polls (list, detail,
// checks, metrics, incidents) and writes create, update, pause and delete a monitor, spread over
// the seeded panel tenants. Open model: a fixed rate of iterations per second, so a slow api
// shows up as latency and dropped iterations instead of as less load.
//
//   just loadtest api-crud
//   just loadtest api-crud -e READ_RATE=40
//
// Every tenant has its own budget of 600 requests per minute; the script refuses to start when
// the rates would take a tenant over 80 % of it, because a 429 there measures the limiter.

import { check } from 'k6';
import exec from 'k6/execution';
import { fresh, login } from '../lib/auth.js';
import { api, listMonitors, tempMonitor } from '../lib/http.js';
import { steadyThresholds, weighted } from '../lib/checks.js';
import {
  ECHO_URL,
  PANEL_BUDGET_PER_MINUTE,
  PANEL_TENANTS,
  TEMP_MONITOR_PREFIX,
  intVar,
  panelSlug,
} from '../lib/config.js';

const READ_RATE = intVar('READ_RATE', 25);
const WRITE_RATE = intVar('WRITE_RATE', 1);
const DURATION = __ENV.DURATION || '5m';

// Requests per iteration: 5 reads plus the full incident list every 4th; 4 writes.
const READ_REQUESTS = 5.25;
const WRITE_REQUESTS = 4;
const perTenantPerMinute =
  ((READ_RATE * READ_REQUESTS + WRITE_RATE * WRITE_REQUESTS) * 60) / PANEL_TENANTS;
if (perTenantPerMinute > PANEL_BUDGET_PER_MINUTE * 0.8) {
  throw new Error(
    `${perTenantPerMinute.toFixed(0)} requests/min per tenant is over 80 % of the ${PANEL_BUDGET_PER_MINUTE} budget: ` +
      'lower READ_RATE or WRITE_RATE, or seed more tenants (LT_PANEL_TENANTS)',
  );
}

const ROUTES = [
  'GET /v1/monitors',
  'GET /v1/monitors/:id',
  'GET /v1/monitors/:id/checks',
  'GET /v1/monitors/:id/metrics',
  'GET /v1/incidents',
  'POST /v1/monitors',
  'PUT /v1/monitors/:id',
  'POST /v1/monitors/:id/pause',
  'DELETE /v1/monitors/:id',
];

export const options = {
  setupTimeout: '3m',
  scenarios: {
    warmup: {
      executor: 'constant-arrival-rate',
      exec: 'reads',
      rate: 5,
      timeUnit: '1s',
      duration: '1m',
      preAllocatedVUs: 10,
      maxVUs: 50,
      tags: { phase: 'warmup' },
    },
    reads: {
      executor: 'constant-arrival-rate',
      exec: 'reads',
      rate: READ_RATE,
      timeUnit: '1s',
      duration: DURATION,
      startTime: '1m',
      preAllocatedVUs: 50,
      maxVUs: 200,
      tags: { phase: 'steady' },
    },
    writes: {
      executor: 'constant-arrival-rate',
      exec: 'writes',
      rate: WRITE_RATE,
      timeUnit: '1s',
      duration: DURATION,
      startTime: '1m',
      preAllocatedVUs: 5,
      maxVUs: 20,
      tags: { phase: 'steady' },
    },
  },
  thresholds: { ...steadyThresholds(ROUTES), rate_limited: ['count==0'] },
};

export function setup() {
  const tenants = [];
  for (let n = 1; n <= PANEL_TENANTS; n++) {
    const session = login(panelSlug(n));
    const monitors = listMonitors(session)
      .json()
      .filter((m) => !m.name.startsWith(TEMP_MONITOR_PREFIX))
      .map((m) => m.id);
    tenants.push({ session, monitors });
  }
  return { tenants };
}

function tenantFor(data, offset) {
  return data.tenants[(exec.scenario.iterationInTest + offset) % data.tenants.length];
}

export function reads(data) {
  const tenant = tenantFor(data, 0);
  const session = fresh(tenant.session);
  const id = tenant.monitors[Math.floor(Math.random() * tenant.monitors.length)];
  const range = weighted([
    ['24h', 0.7],
    ['7d', 0.2],
    ['30d', 0.1],
  ]);

  check(listMonitors(session), { 'list: 200': (r) => r.status === 200 });
  check(api(session, 'GET', `/v1/monitors/${id}`, 'GET /v1/monitors/:id'), {
    'detail: 200': (r) => r.status === 200,
  });
  check(api(session, 'GET', `/v1/monitors/${id}/checks?limit=50`, 'GET /v1/monitors/:id/checks'), {
    'checks: 200': (r) => r.status === 200,
  });
  check(
    api(
      session,
      'GET',
      `/v1/monitors/${id}/metrics?range=${range}`,
      'GET /v1/monitors/:id/metrics',
    ),
    {
      'metrics: 200': (r) => r.status === 200,
    },
  );
  check(api(session, 'GET', '/v1/incidents?status=open', 'GET /v1/incidents'), {
    'open incidents: 200': (r) => r.status === 200,
  });
  if (exec.scenario.iterationInTest % 4 === 0) {
    check(api(session, 'GET', '/v1/incidents?status=all', 'GET /v1/incidents'), {
      'all incidents: 200': (r) => r.status === 200,
    });
  }
}

export function writes(data) {
  // Offset so writes do not always land on the tenant a read is using at the same instant.
  const tenant = tenantFor(data, Math.floor(data.tenants.length / 2));
  const session = fresh(tenant.session);
  const name = `${TEMP_MONITOR_PREFIX}${exec.vu.idInTest}-${exec.scenario.iterationInTest}`;

  const created = api(
    session,
    'POST',
    '/v1/monitors',
    'POST /v1/monitors',
    tempMonitor(name, ECHO_URL),
  );
  if (!check(created, { 'create: 201': (r) => r.status === 201 })) return;
  const id = created.json().id;

  check(
    api(session, 'PUT', `/v1/monitors/${id}`, 'PUT /v1/monitors/:id', {
      ...tempMonitor(name, ECHO_URL),
      interval_s: 300,
    }),
    {
      'update: 200': (r) => r.status === 200,
    },
  );
  check(api(session, 'POST', `/v1/monitors/${id}/pause`, 'POST /v1/monitors/:id/pause'), {
    'pause: 200': (r) => r.status === 200,
  });
  check(api(session, 'DELETE', `/v1/monitors/${id}`, 'DELETE /v1/monitors/:id'), {
    'delete: 204': (r) => r.status === 204,
  });
}

// Removes the monitors an interrupted run left behind, so the next one has room under the quota.
export function teardown(data) {
  for (const tenant of data.tenants) {
    const session = fresh(tenant.session);
    for (const monitor of listMonitors(session).json()) {
      if (monitor.name.startsWith(TEMP_MONITOR_PREFIX)) {
        api(session, 'DELETE', `/v1/monitors/${monitor.id}`, 'DELETE /v1/monitors/:id');
      }
    }
  }
}
