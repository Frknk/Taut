// Instant queries to Prometheus, for scenarios whose verdict depends on a server-side counter
// (errors of the rate limiter, the checker's lag). Prometheus scrapes every 5 s, so a value read
// right after the load can be up to one scrape old: wait with `settle()` first.

import http from 'k6/http';
import { sleep } from 'k6';
import { PROM_URL } from './config.js';

/** Seconds to wait so the last scrape covers what just happened (scrape_interval + margin). */
const SETTLE_S = 7;

/** Waits for Prometheus to scrape the latest values. */
export function settle() {
  sleep(SETTLE_S);
}

/** Value of a PromQL expression that yields one series (or none, read as 0). */
export function scalar(expr) {
  const res = http.get(`${PROM_URL}/api/v1/query?query=${encodeURIComponent(expr)}`, {
    tags: { name: 'GET prometheus query' },
  });
  if (res.status !== 200) {
    throw new Error(`Prometheus answered ${res.status} to ${expr}: is the obs profile up?`);
  }
  const result = res.json().data.result;
  return result.length === 0 ? 0 : Number(result[0].value[1]);
}
