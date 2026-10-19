// Requests to the api on behalf of a tenant, and the metrics every scenario shares.
//
// Every request carries a `name` tag with the route pattern (GET /v1/monitors/:id), never the
// raw URL: ids in the tag would create one time series per monitor in Prometheus. Patterns use
// `:id` instead of `{id}` because braces would break k6 threshold selectors.

import http from 'k6/http';
import { Counter } from 'k6/metrics';
import { API_URL } from './config.js';

/** Responses 429 seen by scenarios that must stay under the budget. */
export const rateLimited = new Counter('rate_limited');

/** Counts a response as rate limited when it is a 429. */
export function track(res) {
  if (res.status === 429) rateLimited.add(1);
  return res;
}

/**
 * Sends `method path` to the api as the session's user in its tenant. `body` is serialized as
 * JSON when present; `params` is merged into the request params (for `responseCallback`).
 */
export function api(session, method, path, name, body, params = {}) {
  const res = http.request(
    method,
    `${API_URL}${path}`,
    body === undefined ? null : JSON.stringify(body),
    {
      ...params,
      headers: {
        Authorization: `Bearer ${session.token}`,
        'X-Tenant-Id': session.tenantId,
        'Content-Type': 'application/json',
        ...params.headers,
      },
      tags: { name, ...params.tags },
    },
  );
  return track(res);
}

/** Monitors of the session's tenant. */
export function listMonitors(session) {
  return api(session, 'GET', '/v1/monitors', 'GET /v1/monitors');
}

/** Body of a monitor the tests create: an HTTP check against the echo server. */
export function tempMonitor(name, target) {
  return { name, type: 'http', target, interval_s: 60, timeout_s: 10 };
}
