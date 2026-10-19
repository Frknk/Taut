// isolation: no tenant reaches another tenant's data while the system is under load. Four
// attacker tenants (lt-i-aN) probe the monitors, incidents and status page of their victim
// (lt-i-bN) while the victims use the panel normally, so pooled connections keep switching
// tenant context (row-level security, ADR 0010) between requests.
//
//   just loadtest isolation
//   just loadtest isolation -e SELFTEST=1    must FAIL: proves the detector catches a leak
//
// A violation is any answer that gives the attacker something: a 2xx where 403/404 was due, a
// foreign id in a list, a foreign monitor on the status page, or a different answer for an
// existing tenant than for a made-up one (an oracle). The first violation aborts the run.

import http from 'k6/http';
import { check } from 'k6';
import { Counter } from 'k6/metrics';
import exec from 'k6/execution';
import { createApiKey, fresh, login, revokeApiKey } from '../lib/auth.js';
import { api, listMonitors } from '../lib/http.js';
import { API_URL, ECHO_URL, ISOLATION_PAIRS, STATUS_URL, intVar } from '../lib/config.js';

const ATTACK_RATE = intVar('ATTACK_RATE', 20);
const VICTIM_RATE = intVar('VICTIM_RATE', 8);
const DURATION = __ENV.DURATION || '3m';
/** The attacker probes its own tenant: every probe should then report a violation. */
const SELFTEST = __ENV.SELFTEST === '1';

const violations = new Counter('isolation_violations');
const unexpected = new Counter('unexpected_responses');

export const options = {
  setupTimeout: '2m',
  scenarios: {
    attack: {
      executor: 'constant-arrival-rate',
      exec: 'attack',
      rate: ATTACK_RATE,
      timeUnit: '1s',
      duration: DURATION,
      preAllocatedVUs: 20,
      maxVUs: 100,
    },
    victims: {
      executor: 'constant-arrival-rate',
      exec: 'victims',
      rate: VICTIM_RATE,
      timeUnit: '1s',
      duration: DURATION,
      preAllocatedVUs: 10,
      maxVUs: 50,
    },
  },
  thresholds: {
    isolation_violations: [{ threshold: 'count==0', abortOnFail: true }],
    unexpected_responses: ['count==0'],
    dropped_iterations: ['count==0'],
  },
};

// 403 and 404 are the answers the attacker should get: they are not failures here.
http.setResponseCallback(http.expectedStatuses({ min: 200, max: 299 }, 403, 404));

function ids(res) {
  return res.json().map((item) => item.id);
}

export function setup() {
  const pairs = ISOLATION_PAIRS.map(({ attacker, victim }) => {
    const victimSession = login(victim);
    const attackerSession = SELFTEST ? victimSession : login(attacker);
    const key = createApiKey(attackerSession, `lt-isolation-${Date.now()}`);
    const victimMonitors = ids(listMonitors(victimSession));
    const victimIncidents = ids(
      api(victimSession, 'GET', '/v1/incidents?status=all', 'GET /v1/incidents'),
    );
    return {
      attacker: attackerSession,
      victim: victimSession,
      key,
      victimMonitors,
      victimIncidents,
    };
  });
  return { pairs };
}

/** Records a probe: a leak is a violation; any other wrong answer is unexpected. */
function verdict(probe, res, leaked, expectedStatus) {
  if (leaked) {
    violations.add(1, { probe });
    console.error(`LEAK in ${probe}: ${res.status} ${res.url}`);
  } else if (res.status !== expectedStatus) {
    unexpected.add(1, { probe });
    console.warn(`${probe}: expected ${expectedStatus}, got ${res.status} (${res.body})`);
  }
  check(res, { [probe]: () => !leaked && res.status === expectedStatus });
}

function denied(probe, res, status, code) {
  const leaked = res.status >= 200 && res.status < 300;
  const wrong = !leaked && res.status === status && res.json('error.code') !== code;
  verdict(probe, res, leaked, wrong ? -1 : status);
}

function pick(list) {
  return list[Math.floor(Math.random() * list.length)];
}

const PROBES = [
  // The victim's id in X-Tenant-Id answers exactly like a tenant that does not exist.
  function foreignHeader(p) {
    const asVictim = api(
      { ...p.attacker, tenantId: p.victim.tenantId },
      'GET',
      '/v1/monitors',
      'GET /v1/monitors (foreign tenant)',
    );
    const asNobody = api(
      { ...p.attacker, tenantId: crypto.randomUUID() },
      'GET',
      '/v1/monitors',
      'GET /v1/monitors (unknown tenant)',
    );
    denied('foreign X-Tenant-Id: 403 not_a_member', asVictim, 403, 'not_a_member');
    const oracle =
      asVictim.status !== asNobody.status ||
      asVictim.json('error.message') !== asNobody.json('error.message');
    verdict(
      'foreign and unknown tenant answer alike',
      asVictim,
      oracle && asVictim.status === 403,
      403,
    );
  },
  function monitorDetail(p) {
    denied(
      'foreign monitor: 404',
      api(p.attacker, 'GET', `/v1/monitors/${pick(p.victimMonitors)}`, 'GET /v1/monitors/:id'),
      404,
      'not_found',
    );
  },
  function monitorChecks(p) {
    denied(
      'foreign checks: 404',
      api(
        p.attacker,
        'GET',
        `/v1/monitors/${pick(p.victimMonitors)}/checks?limit=10`,
        'GET /v1/monitors/:id/checks',
      ),
      404,
      'not_found',
    );
  },
  function monitorMetrics(p) {
    denied(
      'foreign metrics: 404',
      api(
        p.attacker,
        'GET',
        `/v1/monitors/${pick(p.victimMonitors)}/metrics?range=24h`,
        'GET /v1/monitors/:id/metrics',
      ),
      404,
      'not_found',
    );
  },
  function monitorUpdate(p) {
    const body = {
      name: 'lt-isolation-overwrite',
      type: 'http',
      target: ECHO_URL,
      interval_s: 300,
      timeout_s: 10,
    };
    denied(
      'foreign update: 404',
      api(
        p.attacker,
        'PUT',
        `/v1/monitors/${pick(p.victimMonitors)}`,
        'PUT /v1/monitors/:id',
        body,
      ),
      404,
      'not_found',
    );
  },
  function monitorPause(p) {
    denied(
      'foreign pause: 404',
      api(
        p.attacker,
        'POST',
        `/v1/monitors/${pick(p.victimMonitors)}/pause`,
        'POST /v1/monitors/:id/pause',
      ),
      404,
      'not_found',
    );
  },
  function monitorDelete(p) {
    denied(
      'foreign delete: 404',
      api(
        p.attacker,
        'DELETE',
        `/v1/monitors/${pick(p.victimMonitors)}`,
        'DELETE /v1/monitors/:id',
      ),
      404,
      'not_found',
    );
  },
  function incidents(p) {
    denied(
      'foreign incident: 404',
      api(p.attacker, 'GET', `/v1/incidents/${pick(p.victimIncidents)}`, 'GET /v1/incidents/:id'),
      404,
      'not_found',
    );
    const filtered = api(
      p.attacker,
      'GET',
      `/v1/incidents?status=all&monitor_id=${pick(p.victimMonitors)}`,
      'GET /v1/incidents',
    );
    verdict(
      'incidents filtered by a foreign monitor: empty',
      filtered,
      filtered.status === 200 && filtered.json().length > 0,
      200,
    );
  },
  function apiKey(p) {
    const headers = { Authorization: `Bearer ${p.key.key}` };
    const header = http.get(`${API_URL}/v1/monitors`, {
      headers: { ...headers, 'X-Tenant-Id': p.victim.tenantId },
      tags: { name: 'GET /v1/monitors (api key, foreign tenant)' },
    });
    denied('api key with a foreign X-Tenant-Id: 403', header, 403, 'forbidden');
    const detail = http.get(`${API_URL}/v1/monitors/${pick(p.victimMonitors)}`, {
      headers,
      tags: { name: 'GET /v1/monitors/:id (api key)' },
    });
    denied('api key on a foreign monitor: 404', detail, 404, 'not_found');
  },
  function ownViews(p) {
    const foreign = new Set(p.victimMonitors);
    const list = listMonitors(p.attacker);
    verdict(
      'own monitor list: nothing foreign',
      list,
      list.status === 200 && ids(list).some((id) => foreign.has(id)),
      200,
    );
    const page = http.get(`${STATUS_URL}/s/${p.attacker.slug}`, { tags: { name: 'GET /s/:slug' } });
    const names = page.status === 200 ? page.json().monitors.map((m) => m.name) : [];
    const victimSlug = p.victim.slug;
    verdict(
      'own status page: nothing foreign',
      page,
      !SELFTEST && names.some((n) => n.startsWith(victimSlug)),
      200,
    );
  },
];

// Probes that change data: a leak there would really modify the victim, so SELFTEST skips them.
const DESTRUCTIVE = new Set(['monitorUpdate', 'monitorPause', 'monitorDelete']);
const ACTIVE = SELFTEST ? PROBES.filter((probe) => !DESTRUCTIVE.has(probe.name)) : PROBES;

export function attack(data) {
  const iteration = exec.scenario.iterationInTest;
  const pair = data.pairs[iteration % data.pairs.length];
  const p = { ...pair, attacker: fresh(pair.attacker) };
  ACTIVE[Math.floor(iteration / data.pairs.length) % ACTIVE.length](p);
}

export function victims(data) {
  const pair = data.pairs[exec.scenario.iterationInTest % data.pairs.length];
  const session = fresh(pair.victim);
  const id = pick(pair.victimMonitors);
  check(listMonitors(session), { 'victim list: 200': (r) => r.status === 200 });
  check(api(session, 'GET', `/v1/monitors/${id}`, 'GET /v1/monitors/:id'), {
    'victim detail: 200': (r) => r.status === 200,
  });
  check(
    api(session, 'GET', `/v1/monitors/${id}/metrics?range=24h`, 'GET /v1/monitors/:id/metrics'),
    {
      'victim metrics: 200': (r) => r.status === 200,
    },
  );
}

export function teardown(data) {
  for (const pair of data.pairs) revokeApiKey(fresh(pair.attacker), pair.key.id);
}
