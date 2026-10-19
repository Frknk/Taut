// Smoke test: one user walks through the panel and the status page a few times. It proves the
// stack, the seed and the scripts work before a heavy run, and fails on any wrong answer.
//
//   just loadtest smoke

import http from 'k6/http';
import { check, fail } from 'k6';
import { login } from '../lib/auth.js';
import { api, listMonitors, tempMonitor } from '../lib/http.js';
import {
  ECHO_URL,
  PANEL_MONITORS,
  SERVICE_URLS,
  STATUS_DAYS,
  STATUS_MONITORS,
  STATUS_URL,
  TEMP_MONITOR_PREFIX,
  panelSlug,
  statusSlug,
} from '../lib/config.js';

export const options = {
  scenarios: {
    smoke: { executor: 'shared-iterations', vus: 1, iterations: 3, maxDuration: '2m' },
  },
  thresholds: {
    checks: ['rate==1'],
    http_req_failed: ['rate==0'],
  },
};

export function setup() {
  for (const { name, url } of SERVICE_URLS) {
    const res = http.get(`${url}/ready`, { tags: { name: 'GET /ready' } });
    if (!check(res, { [`${name} is ready`]: (r) => r.status === 200 })) {
      fail(`${name} is not ready (${res.status}): is the stack up? (just loadtest-up)`);
    }
  }
  return { session: login(panelSlug(1)) };
}

export default function ({ session }) {
  const list = listMonitors(session);
  check(list, {
    'list monitors: 200': (r) => r.status === 200,
    'list monitors: seeded ones are there': (r) =>
      r.json().filter((m) => !m.name.startsWith(TEMP_MONITOR_PREFIX)).length === PANEL_MONITORS,
  });
  const monitor = list.json()[0];

  check(api(session, 'GET', `/v1/monitors/${monitor.id}`, 'GET /v1/monitors/:id'), {
    'monitor detail: 200': (r) => r.status === 200,
  });
  check(
    api(
      session,
      'GET',
      `/v1/monitors/${monitor.id}/checks?limit=50`,
      'GET /v1/monitors/:id/checks',
    ),
    {
      'checks: 200 with history': (r) => r.status === 200 && r.json().length === 50,
    },
  );
  for (const range of ['24h', '7d', '30d']) {
    check(
      api(
        session,
        'GET',
        `/v1/monitors/${monitor.id}/metrics?range=${range}`,
        'GET /v1/monitors/:id/metrics',
      ),
      {
        [`metrics ${range}: 200 with checks`]: (r) => r.status === 200 && r.json().checks > 0,
      },
    );
  }
  check(api(session, 'GET', '/v1/incidents?status=all', 'GET /v1/incidents'), {
    'incidents: 200 with history': (r) => r.status === 200 && r.json().length > 0,
  });

  const created = api(
    session,
    'POST',
    '/v1/monitors',
    'POST /v1/monitors',
    tempMonitor(`${TEMP_MONITOR_PREFIX}smoke`, ECHO_URL),
  );
  if (check(created, { 'create monitor: 201': (r) => r.status === 201 })) {
    check(api(session, 'DELETE', `/v1/monitors/${created.json().id}`, 'DELETE /v1/monitors/:id'), {
      'delete monitor: 204': (r) => r.status === 204,
    });
  }

  const page = http.get(`${STATUS_URL}/s/${statusSlug(1)}`, { tags: { name: 'GET /s/:slug' } });
  check(page, {
    'status page: 200': (r) => r.status === 200,
    'status page: every monitor with 90 days': (r) => {
      const monitors = r.json().monitors;
      return (
        monitors.length === STATUS_MONITORS && monitors.every((m) => m.days.length === STATUS_DAYS)
      );
    },
    'status page: recent incidents': (r) => r.json().incidents.length > 0,
  });

  const missing = http.get(`${STATUS_URL}/s/lt-no-existe`, {
    tags: { name: 'GET /s/:slug' },
    responseCallback: http.expectedStatuses(404),
  });
  check(missing, { 'unknown status page: 404': (r) => r.status === 404 });
}
