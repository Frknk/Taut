// checker-soak: the checking pipeline (scheduler → NATS → checker → NATS → incidents → database)
// with many monitors for a long time. k6 sends no traffic to Taut: the seeded monitors
// (`just loadtest-seed soak`, 2 000 at 30 s against the echo server) are the load, and k6 only
// reads Prometheus every 15 s and turns the server-side counters into thresholds.
//
//   just loadtest-seed soak
//   caffeinate -i just loadtest checker-soak                 3 min warm-up, then 30 min
//   caffeinate -i just loadtest checker-soak -e DURATION=10m
//
// The SLO of docs/taut.md: 99.5 % of the checks start less than 10 s after their scheduled time.
// The lag histogram leaves out the checks that expired before a checker took them, so expirations
// and the queue in front of the checker are thresholds of their own.

import { sleep } from 'k6';
import { Trend } from 'k6/metrics';
import { scalar } from '../lib/prom.js';
import { intVar } from '../lib/config.js';

const TENANTS = intVar('LT_SOAK_TENANTS', 200);
const MONITORS = intVar('LT_SOAK_MONITORS', 10);
const INTERVAL_S = intVar('LT_SOAK_INTERVAL_S', 30);
const DURATION = __ENV.DURATION || '30m';
/** The seeded monitors need a full interval to spread and every rate() a full window: 3 min. */
const WARMUP = __ENV.WARMUP || '3m';
/** Seconds between two readings of Prometheus. */
const SAMPLE_EVERY_S = 15;
/** Window of every rate(): long enough to smooth a 30 s schedule, short enough to see a dip. */
const WINDOW = '2m';

const SOAK_MONITORS = TENANTS * MONITORS;
/** Checks per second the seeded monitors should produce. */
const EXPECTED_RATE = SOAK_MONITORS / INTERVAL_S;
/** A queue longer than this many seconds of checks means the checker is falling behind. */
const MAX_BACKLOG_S = 10;
const MAX_PENDING = Math.ceil(EXPECTED_RATE * MAX_BACKLOG_S);

const lagUnder10s = new Trend('lag_under_10s_ratio');
const lagP99 = new Trend('lag_p99_seconds');
const expiredPerSecond = new Trend('expired_per_second');
const scheduledRatio = new Trend('scheduled_ratio');
const completedRatio = new Trend('completed_ratio');
const recordedRatio = new Trend('recorded_ratio');
/**
 * Failed checks over all checks. Informational: the metric has no tenant label, so monitors of
 * the dev database that point at a real target that is down also count. The report checks the
 * soak monitors alone with SQL (none of them should fail against the echo server).
 */
const failedRatio = new Trend('failed_ratio');
const checkerPending = new Trend('checker_pending');
const incidentsPending = new Trend('incidents_pending');

export const options = {
  setupTimeout: '3m',
  scenarios: {
    observe: {
      executor: 'constant-arrival-rate',
      rate: 1,
      timeUnit: `${SAMPLE_EVERY_S}s`,
      duration: DURATION,
      startTime: WARMUP,
      preAllocatedVUs: 1,
      maxVUs: 2,
    },
  },
  thresholds: {
    // The SLO, in every 2-minute window of the run.
    lag_under_10s_ratio: ['min>=0.995'],
    expired_per_second: ['max==0'],
    // The scheduler publishes what the monitors ask for, the checker completes what was
    // published and the incident service records what was completed.
    scheduled_ratio: ['min>=0.95'],
    completed_ratio: ['min>=0.99'],
    recorded_ratio: ['min>=0.99'],
    checker_pending: [`max<${MAX_PENDING}`],
    incidents_pending: [`max<${MAX_PENDING}`],
  },
};

const rate = (metric, filter = '') => `sum(rate(${metric}${filter}[${WINDOW}]))`;

/** Waits until the scheduler holds the seeded monitors (they arrive with its next resync). */
export function setup() {
  for (let waited = 0; waited <= 150; waited += 5) {
    const monitors = scalar('sum(scheduler_monitors)');
    if (monitors >= SOAK_MONITORS) {
      console.log(
        `scheduler holds ${monitors} monitors; expecting ${EXPECTED_RATE.toFixed(1)} checks/s`,
      );
      return { startedAt: Date.now() };
    }
    sleep(5);
  }
  throw new Error(
    `the scheduler never reached ${SOAK_MONITORS} monitors: is the soak seed loaded?`,
  );
}

export default function () {
  const completed = scalar(rate('uptime_checks_total'));
  const scheduled = scalar(rate('uptime_checks_scheduled_total'));
  const reading = {
    lag: scalar(
      `${rate('uptime_check_lag_seconds_bucket', '{le=~"10|10.0"}')} / ${rate('uptime_check_lag_seconds_count')}`,
    ),
    p99: scalar(
      `histogram_quantile(0.99, sum by (le) (rate(uptime_check_lag_seconds_bucket[${WINDOW}])))`,
    ),
    expired: scalar(`${rate('uptime_checks_expired_total')} or vector(0)`),
    failed: scalar(
      `(${rate('uptime_checks_total', '{result!="up"}')} or vector(0)) / ${rate('uptime_checks_total')}`,
    ),
    recorded: scalar(rate('check_results_recorded_total')),
    checkerQueue: scalar(
      `max(max_over_time(jetstream_consumer_num_pending{stream_name="CHECKS",consumer_name="checker"}[${WINDOW}]))`,
    ),
    incidentsQueue: scalar(
      `max(max_over_time(jetstream_consumer_num_pending{stream_name="CHECKS",consumer_name="incidents"}[${WINDOW}]))`,
    ),
  };
  lagUnder10s.add(reading.lag);
  lagP99.add(reading.p99);
  expiredPerSecond.add(reading.expired);
  scheduledRatio.add(scheduled / EXPECTED_RATE);
  completedRatio.add(scheduled > 0 ? completed / scheduled : 0);
  recordedRatio.add(completed > 0 ? reading.recorded / completed : 0);
  failedRatio.add(reading.failed);
  checkerPending.add(reading.checkerQueue);
  incidentsPending.add(reading.incidentsQueue);
  console.log(
    `checks/s ${completed.toFixed(1)} of ${EXPECTED_RATE.toFixed(1)} | lag<10s ${(reading.lag * 100).toFixed(2)} % ` +
      `p99 ${reading.p99.toFixed(2)} s | expired/s ${reading.expired.toFixed(3)} | ` +
      `queue checker ${reading.checkerQueue} incidents ${reading.incidentsQueue}`,
  );
}
