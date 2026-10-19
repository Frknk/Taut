// rate-limit: the per-tenant budget under load (services/api/src/ratelimit.rs). One tenant gets
// more than its budget, both through the panel (600/min) and through an API key (60/min), while a
// second tenant stays under its own. The limiter counts in fixed windows aligned to the minute, so
// setup() waits for the next minute and the run covers three whole windows.
//
//   just loadtest rate-limit
//
// Passes when every window lets through exactly the budget (within a couple of requests, for
// requests sent right at a window's edge), every 429 carries a Retry-After that matches the
// seconds left in the minute, the control tenant never sees a 429 and the limiter never fails
// open (api_rate_limit_errors_total does not grow).

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Rate } from 'k6/metrics';
import { createApiKey, fresh, login, revokeApiKey } from '../lib/auth.js';
import { api } from '../lib/http.js';
import { scalar, settle } from '../lib/prom.js';
import {
  API_KEY_BUDGET_PER_MINUTE,
  API_URL,
  PANEL_BUDGET_PER_MINUTE,
  RATE_LIMIT_SLUGS,
} from '../lib/config.js';

/** Requests per second over the panel budget (10/s): two thirds of every window get through. */
const PANEL_RATE = 15;
/** Requests per second over the API-key budget (1/s): half of every window gets through. */
const KEY_RATE = 2;
/** Three whole windows: the run starts one second into a minute and ends one second before. */
const WINDOWS = 3;
const DURATION = `${WINDOWS * 60 - 2}s`;
/** Requests sent within this many ms of a window's edge may land in the neighbouring window. */
const EDGE_TOLERANCE = 2;

const panelOk = new Counter('panel_ok');
const keyOk = new Counter('key_ok');
const retryAfterOk = new Rate('retry_after_ok');
const controlLimited = new Counter('control_limited');
const limiterErrors = new Counter('limiter_errors');

function windowThresholds(metric, budget) {
  const thresholds = {};
  for (let w = 0; w < WINDOWS; w++) {
    thresholds[`${metric}{window:${w}}`] = [
      `count>=${budget - EDGE_TOLERANCE}`,
      `count<=${budget + EDGE_TOLERANCE}`,
    ];
  }
  return thresholds;
}

export const options = {
  setupTimeout: '2m',
  scenarios: {
    panel: {
      executor: 'constant-arrival-rate',
      exec: 'panel',
      rate: PANEL_RATE,
      timeUnit: '1s',
      duration: DURATION,
      preAllocatedVUs: 10,
      maxVUs: 40,
    },
    apiKey: {
      executor: 'constant-arrival-rate',
      exec: 'apiKey',
      rate: KEY_RATE,
      timeUnit: '1s',
      duration: DURATION,
      preAllocatedVUs: 2,
      maxVUs: 10,
    },
    control: {
      executor: 'constant-arrival-rate',
      exec: 'control',
      rate: 2,
      timeUnit: '1s',
      duration: DURATION,
      preAllocatedVUs: 2,
      maxVUs: 10,
    },
  },
  thresholds: {
    ...windowThresholds('panel_ok', PANEL_BUDGET_PER_MINUTE),
    ...windowThresholds('key_ok', API_KEY_BUDGET_PER_MINUTE),
    retry_after_ok: ['rate==1'],
    control_limited: ['count==0'],
    limiter_errors: ['count==0'],
    // 429 is the expected answer here: only other errors count as failures.
    http_req_failed: ['rate==0'],
    dropped_iterations: ['count==0'],
  },
};

http.setResponseCallback(http.expectedStatuses({ min: 200, max: 399 }, 429));

const limiterErrorsExpr = 'sum(api_rate_limit_errors_total) or vector(0)';

export function setup() {
  const flooded = login(RATE_LIMIT_SLUGS.flooded);
  const control = login(RATE_LIMIT_SLUGS.control);
  const key = createApiKey(flooded, `lt-rate-limit-${Date.now()}`);
  const limiterErrorsBefore = scalar(limiterErrorsExpr);
  // Start one second into the next minute: everything above counted in the previous window.
  sleep((60_000 - (Date.now() % 60_000) + 1_000) / 1000);
  return {
    flooded,
    control,
    key,
    limiterErrorsBefore,
    firstMinute: Math.floor(Date.now() / 60_000),
  };
}

/** Window (0, 1, 2) a request sent now falls in, by the clock the api shares with k6. */
function windowOf(data, sentAt) {
  return String(Math.floor(sentAt / 60_000) - data.firstMinute);
}

/** Checks a 429: its code and that Retry-After counts the seconds left in the minute. */
function checkLimited(res, sentAt) {
  const expected = 60 - (Math.floor(sentAt / 1000) % 60);
  const retryAfter = Number(res.headers['Retry-After']);
  const ok = res.json('error.code') === 'too_many_requests' && Math.abs(retryAfter - expected) <= 1;
  retryAfterOk.add(ok);
  if (!ok) console.warn(`429 with Retry-After ${retryAfter}, expected ${expected}`);
}

export function panel(data) {
  const sentAt = Date.now();
  const res = api(fresh(data.flooded), 'GET', '/v1/monitors', 'GET /v1/monitors');
  const window = windowOf(data, sentAt);
  if (res.status === 200) panelOk.add(1, { window });
  else if (res.status === 429) checkLimited(res, sentAt);
  check(res, { 'panel: 200 or 429': (r) => r.status === 200 || r.status === 429 });
}

export function apiKey(data) {
  const sentAt = Date.now();
  const res = http.get(`${API_URL}/v1/monitors`, {
    headers: { Authorization: `Bearer ${data.key.key}` },
    tags: { name: 'GET /v1/monitors (api key)' },
  });
  const window = windowOf(data, sentAt);
  if (res.status === 200) keyOk.add(1, { window });
  else if (res.status === 429) checkLimited(res, sentAt);
  check(res, { 'api key: 200 or 429': (r) => r.status === 200 || r.status === 429 });
}

export function control(data) {
  const res = api(fresh(data.control), 'GET', '/v1/monitors', 'GET /v1/monitors (control)');
  if (res.status === 429) controlLimited.add(1);
  check(res, { 'control: 200': (r) => r.status === 200 });
}

export function teardown(data) {
  revokeApiKey(fresh(data.flooded), data.key.id);
  settle();
  const grown = scalar(limiterErrorsExpr) - data.limiterErrorsBefore;
  limiterErrors.add(grown);
  if (grown > 0) console.error(`the limiter failed open ${grown} times (Redis errors)`);
}
