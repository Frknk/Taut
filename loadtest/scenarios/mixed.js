// mixed: everything a small Taut receives at once, sized in people instead of requests per
// second. Every other scenario loads one service alone; this one asks how many users a server
// holds when status pages, the panel, logins, heartbeats and the checks share it. Made for the
// VPS of the demo (loadtest/vps-demo.sh mixta), where one small CPU runs every service.
//
// One unit of users, multiplied by each step (SCALES, 3 min each):
//   VIEWERS        status-page tabs open; each polls GET /s/{slug} every 30 s
//                  (web/src/routes/status/[slug]/+page.svelte)
//   NEW_PER_MIN    first visits per minute: index.html, the assets it preloads and the page
//   PANEL_USERS    people in the panel: half on the monitor list (every 10 s), half on a monitor
//                  detail (detail, checks and incidents every 10 s, metrics every 60 s), all with
//                  the open-incident counter of the sidebar (every 30 s)
//   LOGINS_PER_MIN real logins (Argon2id) of the seeded owners
//   HB_PER_MIN     heartbeat pings (POST /h/{token}) of the demo's heartbeat monitors
// The checks are a constant background: the monitors of loadtest/seed/mixed.sql (200 at 60 s by
// default), whose lag k6 reads from Prometheus.
//
//   loadtest/vps-demo.sh mixta-sembrar && caffeinate -i loadtest/vps-demo.sh mixta
//   just loadtest mixed -e BASE_URL=... -e SCALES=1,2 -e STEP_S=120
//
// k6 has no HTTP cache, so every first visit downloads every asset, and the route chunks the
// app imports at runtime are not fetched: the asset traffic is a lower bound. The thresholds say
// which step kept the SLO on status pages and panel; a failed one is a result.

import http from 'k6/http';
import { check, fail } from 'k6';
import exec from 'k6/execution';
import { Trend } from 'k6/metrics';
import { fresh, login } from '../lib/auth.js';
import { api } from '../lib/http.js';
import { scalar } from '../lib/prom.js';
import {
  API_URL,
  AUTH_URL,
  PASSWORD,
  STATUS_TENANTS,
  STATUS_URL,
  intVar,
  ownerEmail,
  statusSlug,
} from '../lib/config.js';

/** Reads a comma-separated list of positive numbers (`SCALES=0.5,1,2`). */
function numberListVar(name, fallback) {
  const raw = __ENV[name];
  if (raw === undefined || raw === '') return fallback;
  const values = raw.split(',').map((part) => Number(part.trim()));
  if (values.some((value) => !Number.isFinite(value) || value <= 0)) {
    throw new Error(`${name} must be positive numbers separated by commas, got "${raw}"`);
  }
  return values;
}

const SCALES = numberListVar('SCALES', [0.5, 1, 2, 3]);
const STEP_S = intVar('STEP_S', 180);
const VIEWERS = intVar('VIEWERS', 1000);
const NEW_PER_MIN = intVar('NEW_PER_MIN', 60);
const PANEL_USERS = intVar('PANEL_USERS', 20);
const LOGINS_PER_MIN = intVar('LOGINS_PER_MIN', 2);
const HB_PER_MIN = intVar('HB_PER_MIN', 20);
/** Seed sizes of mixed.sql and of the demo's heartbeat monitors (lt-hb-token-0001…). */
const MIXED_TENANTS = intVar('LT_MIXED_TENANTS', 20);
const HB_TOKENS = intVar('HB_TOKENS', 20);
const MAX_VUS = intVar('MAX_VUS', 300);
/** Page whose HTML gives the assets a first visit downloads. */
const PAGE_PATH = `/status/${statusSlug(1)}`;
const ACCEPT_ENCODING = 'gzip, deflate, br, zstd';
/** Window of the Prometheus rates: shorter than a step, so each step reads its own checks. */
const WINDOW = '1m';

const lagUnder10s = new Trend('lag_under_10s_ratio');
const lagP99 = new Trend('lag_p99_seconds');
const checksPerSecond = new Trend('checks_per_second');

function mixedSlug(n) {
  return `lt-m-${String(n).padStart(2, '0')}`;
}

/** Tag of a step: `x0_5`, `x1`… (no dots, they would read as part of a threshold selector). */
function stepTag(scale) {
  return `x${String(scale).replace('.', '_')}`;
}

/** Arrivals per minute for a step; at least one so a small unit never drops a kind. */
function perMinute(value) {
  return Math.max(1, Math.round(value));
}

/** The traffic of one person or event, in arrivals per minute of one unit of users. */
const TRAFFIC = {
  statusPoll: { kind: 'status', perMin: VIEWERS * 2 },
  firstVisit: { kind: 'assets', perMin: NEW_PER_MIN },
  panelList: { kind: 'panel', perMin: PANEL_USERS * 0.5 * 6 },
  panelDetail: { kind: 'panel', perMin: PANEL_USERS * 0.5 * 6 },
  panelMetrics: { kind: 'panel', perMin: PANEL_USERS * 0.5 },
  panelSidebar: { kind: 'panel', perMin: PANEL_USERS * 2 },
  logins: { kind: 'login', perMin: LOGINS_PER_MIN },
  heartbeats: { kind: 'heartbeat', perMin: HB_PER_MIN },
};

function scenarios() {
  const all = {};
  SCALES.forEach((scale, i) => {
    const step = stepTag(scale);
    const startTime = `${i * STEP_S}s`;
    for (const [fn, { kind, perMin }] of Object.entries(TRAFFIC)) {
      const rate = perMinute(perMin * scale);
      all[`${step}_${fn}`] = {
        executor: 'constant-arrival-rate',
        exec: fn,
        rate,
        timeUnit: '1m',
        duration: `${STEP_S}s`,
        startTime,
        preAllocatedVUs: Math.min(Math.ceil(rate / 60) + 2, 50),
        maxVUs: MAX_VUS,
        tags: { step, kind },
      };
    }
    all[`${step}_observe`] = {
      executor: 'constant-arrival-rate',
      exec: 'observe',
      rate: 4,
      timeUnit: '1m',
      duration: `${STEP_S}s`,
      startTime,
      preAllocatedVUs: 1,
      maxVUs: 2,
      tags: { step, kind: 'observe' },
    };
  });
  return all;
}

function thresholds() {
  const all = {};
  for (const scale of SCALES) {
    const step = stepTag(scale);
    for (const kind of ['status', 'panel']) {
      all[`http_req_duration{step:${step},kind:${kind}}`] = ['p(95)<300'];
      all[`http_req_failed{step:${step},kind:${kind}}`] = ['rate<0.01'];
    }
    // Informational: a first visit downloads ~20 files over the internet, and a login hashes
    // Argon2id on a small CPU; their latencies are reported, only failures are judged.
    for (const kind of ['assets', 'login', 'heartbeat']) {
      all[`http_req_duration{step:${step},kind:${kind}}`] = ['p(95)<5000'];
      all[`http_req_failed{step:${step},kind:${kind}}`] = ['rate<0.01'];
    }
    all[`lag_under_10s_ratio{step:${step}}`] = ['min>=0.995'];
  }
  return all;
}

export const options = {
  setupTimeout: '5m',
  scenarios: scenarios(),
  thresholds: thresholds(),
};

export function setup() {
  const html = http.get(`${STATUS_URL}${PAGE_PATH}`, { tags: { name: 'setup' } });
  if (html.status !== 200) fail(`${PAGE_PATH} answered ${html.status}: is the panel deployed?`);
  const assets = [...new Set([...html.body.matchAll(/href="(\/_app\/[^"]+)"/g)].map((m) => m[1]))];
  if (assets.length === 0) fail(`${PAGE_PATH} preloads no assets: did the build change?`);

  for (let n = 1; n <= STATUS_TENANTS; n++) {
    const res = http.get(`${STATUS_URL}/s/${statusSlug(n)}`, { tags: { name: 'setup' } });
    if (res.status !== 200) fail(`${statusSlug(n)} answered ${res.status}: is the seed loaded?`);
  }

  const tenants = [];
  for (let n = 1; n <= MIXED_TENANTS; n++) {
    const session = login(mixedSlug(n));
    const monitors = api(session, 'GET', '/v1/monitors', 'setup').json();
    if (monitors.length === 0) fail(`${mixedSlug(n)} has no monitors: run mixta-sembrar`);
    tenants.push({ session, monitors: monitors.map((m) => m.id) });
  }

  const hb = http.post(`${API_URL}/h/lt-hb-token-0001`, null, { tags: { name: 'setup' } });
  if (hb.status !== 200) fail(`heartbeat lt-hb-token-0001 answered ${hb.status}`);

  console.log(
    `${assets.length} assets per first visit; steps ${SCALES.join(', ')}× of ${VIEWERS} viewers, ` +
      `${NEW_PER_MIN} first visits/min, ${PANEL_USERS} panel users, ${LOGINS_PER_MIN} logins/min`,
  );
  return { assets, tenants };
}

const pick = (list) => list[Math.floor(Math.random() * list.length)];
const statusPage = () =>
  `${STATUS_URL}/s/${statusSlug(1 + Math.floor(Math.random() * STATUS_TENANTS))}`;
const encoded = { headers: { 'Accept-Encoding': ACCEPT_ENCODING }, responseType: 'none' };

export function statusPoll() {
  const res = http.get(statusPage(), { ...encoded, tags: { name: 'GET /s/:slug' } });
  check(res, { 'status page: 200': (r) => r.status === 200 });
}

export function firstVisit(data) {
  const html = http.get(`${STATUS_URL}${PAGE_PATH}`, {
    ...encoded,
    tags: { name: 'GET /status/:slug' },
  });
  if (!check(html, { 'html: 200': (r) => r.status === 200 })) return;
  // The browser fetches the preloaded files in parallel, then the app asks for the page.
  const responses = http.batch(
    data.assets.map((path) => [
      'GET',
      `${STATUS_URL}${path}`,
      null,
      { ...encoded, tags: { name: 'GET /_app/*' } },
    ]),
  );
  check(responses, { 'assets: 200': (all) => all.every((r) => r.status === 200) });
  const page = http.get(statusPage(), { ...encoded, tags: { name: 'GET /s/:slug' } });
  check(page, { 'status page: 200': (r) => r.status === 200 });
}

function panelUser(data) {
  const tenant = data.tenants[exec.scenario.iterationInTest % data.tenants.length];
  return { session: fresh(tenant.session), id: pick(tenant.monitors) };
}

export function panelList(data) {
  const { session } = panelUser(data);
  check(api(session, 'GET', '/v1/monitors', 'GET /v1/monitors'), {
    'list: 200': (r) => r.status === 200,
  });
}

export function panelDetail(data) {
  const { session, id } = panelUser(data);
  const headers = { Authorization: `Bearer ${session.token}`, 'X-Tenant-Id': session.tenantId };
  // The detail page loads these three at once (Promise.all in monitors/[id]/+page.svelte).
  const responses = http.batch([
    [
      'GET',
      `${API_URL}/v1/monitors/${id}`,
      null,
      { headers, tags: { name: 'GET /v1/monitors/:id' } },
    ],
    [
      'GET',
      `${API_URL}/v1/monitors/${id}/checks?limit=20`,
      null,
      { headers, tags: { name: 'GET /v1/monitors/:id/checks' } },
    ],
    [
      'GET',
      `${API_URL}/v1/incidents?monitor_id=${id}&limit=10`,
      null,
      { headers, tags: { name: 'GET /v1/incidents' } },
    ],
  ]);
  check(responses, { 'detail: 200': (all) => all.every((r) => r.status === 200) });
}

export function panelMetrics(data) {
  const { session, id } = panelUser(data);
  const res = api(
    session,
    'GET',
    `/v1/monitors/${id}/metrics?range=24h`,
    'GET /v1/monitors/:id/metrics',
  );
  check(res, { 'metrics: 200': (r) => r.status === 200 });
}

export function panelSidebar(data) {
  const { session } = panelUser(data);
  check(api(session, 'GET', '/v1/incidents?status=open', 'GET /v1/incidents'), {
    'open incidents: 200': (r) => r.status === 200,
  });
}

export function logins() {
  const email = ownerEmail(mixedSlug(1 + Math.floor(Math.random() * MIXED_TENANTS)));
  const res = http.post(
    `${AUTH_URL}/v1/auth/login`,
    JSON.stringify({ email, password: PASSWORD }),
    {
      headers: { 'Content-Type': 'application/json' },
      tags: { name: 'POST /v1/auth/login' },
      responseType: 'none',
    },
  );
  check(res, { 'login: 200': (r) => r.status === 200 });
}

export function heartbeats() {
  const token = `lt-hb-token-${String(1 + Math.floor(Math.random() * HB_TOKENS)).padStart(4, '0')}`;
  const res = http.post(`${API_URL}/h/${token}`, null, { tags: { name: 'POST /h/:token' } });
  check(res, { 'heartbeat: 200': (r) => r.status === 200 });
}

const rate = (metric, filter = '') => `sum(rate(${metric}${filter}[${WINDOW}]))`;

/** Reads the checker from Prometheus: share of checks with lag under 10 s, p99 and checks/s. */
export function observe() {
  const step = { step: exec.scenario.name.split('_observe')[0] };
  const lag = scalar(
    `${rate('uptime_check_lag_seconds_bucket', '{le=~"10|10.0"}')} / ${rate('uptime_check_lag_seconds_count')}`,
  );
  // NaN (no checks in the window) would pass `min>=` silently; count it as a miss.
  lagUnder10s.add(Number.isFinite(lag) ? lag : 0, step);
  lagP99.add(
    scalar(
      `histogram_quantile(0.99, sum by (le) (rate(uptime_check_lag_seconds_bucket[${WINDOW}])))`,
    ),
    step,
  );
  checksPerSecond.add(scalar(rate('uptime_checks_total')), step);
}
