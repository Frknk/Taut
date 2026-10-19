// status-page: public status pages read by many visitors. It is the baseline of week 6 (no
// cache) and week 13 reruns it unchanged with the Redis cache: keep the profile and the mix the
// same, or the comparison means nothing. k6 has no HTTP cache, so Cache-Control does not apply:
// every request reaches status-bff.
//
//   just loadtest status-page                  500 req/s over the 20 seeded pages
//   just loadtest status-page -e SLUGS=hot     everyone on one page (an incident going viral)
//   just loadtest status-page -e RATE=1000

import http from 'k6/http';
import { check, fail } from 'k6';
import exec from 'k6/execution';
import { steadyThresholds } from '../lib/checks.js';
import {
  STATUS_DAYS,
  STATUS_MONITORS,
  STATUS_TENANTS,
  STATUS_URL,
  enumVar,
  intVar,
  statusSlug,
} from '../lib/config.js';

const RATE = intVar('RATE', 500);
const DURATION = __ENV.DURATION || '5m';
const SLUGS = enumVar('SLUGS', ['spread', 'hot'], 'spread');
/** One in this many responses is parsed and checked; parsing all of them would load k6 itself. */
const PARSE_EVERY = intVar('PARSE_EVERY', 100);

export const options = {
  scenarios: {
    ramp: {
      executor: 'ramping-arrival-rate',
      startRate: Math.min(50, RATE),
      timeUnit: '1s',
      stages: [{ target: RATE, duration: '30s' }],
      preAllocatedVUs: 100,
      maxVUs: 1000,
      tags: { phase: 'warmup' },
    },
    steady: {
      executor: 'constant-arrival-rate',
      rate: RATE,
      timeUnit: '1s',
      duration: DURATION,
      startTime: '30s',
      preAllocatedVUs: 200,
      maxVUs: 1000,
      tags: { phase: 'steady' },
    },
  },
  thresholds: steadyThresholds(['GET /s/:slug']),
};

export function setup() {
  for (let n = 1; n <= STATUS_TENANTS; n++) {
    const res = http.get(`${STATUS_URL}/s/${statusSlug(n)}`, { tags: { name: 'GET /s/:slug' } });
    if (res.status !== 200)
      fail(`${statusSlug(n)} answered ${res.status}: is the seed loaded? (just loadtest-seed)`);
  }
}

function slug() {
  if (SLUGS === 'hot') return statusSlug(1);
  return statusSlug(1 + (exec.scenario.iterationInTest % STATUS_TENANTS));
}

export default function () {
  const parse = exec.scenario.iterationInTest % PARSE_EVERY === 0;
  const res = http.get(`${STATUS_URL}/s/${slug()}`, {
    tags: { name: 'GET /s/:slug' },
    responseType: parse ? 'text' : 'none',
  });
  check(res, { 'status page: 200': (r) => r.status === 200 });
  if (parse && res.status === 200) {
    check(res, {
      'status page: complete': (r) => {
        const monitors = r.json().monitors;
        return (
          monitors.length === STATUS_MONITORS &&
          monitors.every((m) => m.days.length === STATUS_DAYS)
        );
      },
    });
  }
}
