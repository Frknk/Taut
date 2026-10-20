// status-page: public status pages read by many visitors. It is the baseline of week 6 (no
// cache) and week 13 reruns it unchanged with the Redis cache: keep the profile and the mix the
// same, or the comparison means nothing. k6 has no HTTP cache, so Cache-Control does not apply:
// every request reaches status-bff.
//
//   just loadtest status-page                      500 req/s over the 20 seeded pages
//   just loadtest status-page -e SLUGS=hot         everyone on one page (an incident going viral)
//   just loadtest status-page -e RATE=1000
//   just loadtest status-page -e PROFILE=stress     steps from 100 to 1 500 req/s, 2 min each
//   just loadtest status-page -e PROFILE=spike      50 req/s, a minute at 2 000, back to 50
//     (-e SPIKE_BASE=20 -e SPIKE_PEAK=200 for a small server)
//   just loadtest status-page -e PROFILE=breakpoint 0 to 3 000 req/s in 15 min, stops when it breaks
//
// Only the baseline has pass/fail thresholds on the SLO. The other profiles look for where and
// how the page degrades, so their thresholds say which step or phase kept the SLO: a failed
// threshold there is a result, not a broken test.

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
/**
 * What the visitor's browser accepts. Over HTTPS every browser sends at least gzip and br, and
 * k6 sends nothing unless asked; `-e ACCEPT_ENCODING=` measures the uncompressed page.
 */
const ACCEPT_ENCODING = __ENV.ACCEPT_ENCODING ?? 'gzip, deflate, br';
/** One in this many responses is parsed and checked; parsing all of them would load k6 itself. */
const PARSE_EVERY = intVar('PARSE_EVERY', 100);
const PROFILE = enumVar('PROFILE', ['baseline', 'stress', 'spike', 'breakpoint'], 'baseline');

/** Steps of the stress profile, in requests per second; the probe of 19-10 broke near 950. */
const STRESS_RATES = [100, 250, 500, 750, 1000, 1250, 1500];
const STRESS_STEP_S = intVar('STEP_S', 120);
/** VUs k6 may use: at ~1 s per response the open model needs about one VU per req/s. */
const MAX_VUS = intVar('MAX_VUS', 2000);
/** Rates of the spike profile: the normal traffic before and after, and the peak. */
const SPIKE_BASE = intVar('SPIKE_BASE', 50);
const SPIKE_PEAK = intVar('SPIKE_PEAK', 2000);

function constant(rate, duration, startTime, phase) {
  return {
    executor: 'constant-arrival-rate',
    rate,
    timeUnit: '1s',
    duration,
    startTime,
    preAllocatedVUs: Math.min(rate, 200),
    maxVUs: MAX_VUS,
    tags: { phase },
  };
}

/** The SLO (p95 < 300 ms, errors < 1 %) on one phase, without aborting the run. */
function sloOf(phase) {
  return {
    [`http_req_duration{phase:${phase}}`]: ['p(95)<300'],
    [`http_req_failed{phase:${phase}}`]: ['rate<0.01'],
  };
}

const PROFILES = {
  baseline: () => ({
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
      steady: { ...constant(RATE, DURATION, '30s', 'steady'), preAllocatedVUs: 200, maxVUs: 1000 },
    },
    thresholds: steadyThresholds(['GET /s/:slug']),
  }),
  // Steps without ramps in between, one scenario each, so the summary has every step apart.
  stress: () => ({
    scenarios: Object.fromEntries(
      STRESS_RATES.map((rate, i) => [
        `r${rate}`,
        constant(rate, `${STRESS_STEP_S}s`, `${i * STRESS_STEP_S}s`, `r${rate}`),
      ]),
    ),
    thresholds: Object.assign({}, ...STRESS_RATES.map((rate) => sloOf(`r${rate}`))),
  }),
  // The question is whether the page recovers on its own once the spike is over.
  spike: () => ({
    scenarios: {
      before: constant(SPIKE_BASE, '1m', '0s', 'before'),
      spike: {
        executor: 'ramping-arrival-rate',
        startRate: SPIKE_BASE,
        timeUnit: '1s',
        stages: [
          { target: SPIKE_PEAK, duration: '10s' },
          { target: SPIKE_PEAK, duration: '1m' },
          { target: SPIKE_BASE, duration: '10s' },
        ],
        startTime: '1m',
        preAllocatedVUs: 200,
        maxVUs: MAX_VUS,
        tags: { phase: 'spike' },
      },
      after: constant(SPIKE_BASE, '2m', '2m20s', 'after'),
    },
    thresholds: { ...sloOf('before'), ...sloOf('after') },
  }),
  // A slow, steady ramp that stops as soon as the page clearly stops keeping up.
  breakpoint: () => ({
    scenarios: {
      ramp: {
        executor: 'ramping-arrival-rate',
        startRate: 0,
        timeUnit: '1s',
        stages: [{ target: 3000, duration: '15m' }],
        preAllocatedVUs: 200,
        maxVUs: MAX_VUS,
        tags: { phase: 'ramp' },
      },
    },
    thresholds: {
      http_req_duration: [{ threshold: 'p(95)<1000', abortOnFail: true, delayAbortEval: '30s' }],
      http_req_failed: [{ threshold: 'rate<0.05', abortOnFail: true, delayAbortEval: '30s' }],
    },
  }),
};

export const options = PROFILES[PROFILE]();

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
    headers: ACCEPT_ENCODING ? { 'Accept-Encoding': ACCEPT_ENCODING } : {},
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
