// Thresholds shared by the load scenarios. They apply only to the `phase:steady` tag, so the
// warm-up (JIT of connections, empty caches) does not count against the SLO of docs/taut.md:
// p95 of the api under 300 ms.

/** SLO thresholds for the steady phase, plus one p95 per route in `routes` (name tags). */
export function steadyThresholds(routes = []) {
  const thresholds = {
    'http_req_duration{phase:steady}': ['p(95)<300', 'p(99)<1000'],
    'http_req_failed{phase:steady}': ['rate<0.01'],
    'checks{phase:steady}': ['rate>0.99'],
    // With arrival-rate executors a dropped iteration means k6 ran out of VUs because the
    // system answered too slowly: the offered load was not applied.
    dropped_iterations: ['count==0'],
  };
  for (const route of routes) {
    thresholds[`http_req_duration{phase:steady,name:${route}}`] = ['p(95)<300'];
  }
  return thresholds;
}

/** Picks a value from `[value, weight]` pairs whose weights add up to 1. */
export function weighted(pairs) {
  let r = Math.random();
  for (const [value, weight] of pairs) {
    r -= weight;
    if (r < 0) return value;
  }
  return pairs[pairs.length - 1][0];
}
