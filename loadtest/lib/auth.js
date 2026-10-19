// Sessions of the seeded users. Logging in hashes the password with Argon2id on purpose, so the
// scenarios log in once in setup() and reuse the token; a VU logs in again only when the token
// is about to expire (900 s by default), which only matters for long runs against staging.
//
// Never send a wrong password: auth counts failed logins per IP (20 every 15 min) and every k6
// request comes from the same address.

import http from 'k6/http';
import { fail } from 'k6';
import { AUTH_URL, API_URL, PASSWORD, ownerEmail } from './config.js';

/** Seconds before expiry at which a VU renews its token. */
const RENEW_MARGIN_S = 60;

/** Logs in the owner of `slug` and resolves its tenant: { slug, email, token, expiresAt, tenantId }. */
export function login(slug) {
  const email = ownerEmail(slug);
  const res = http.post(
    `${AUTH_URL}/v1/auth/login`,
    JSON.stringify({ email, password: PASSWORD }),
    {
      headers: { 'Content-Type': 'application/json' },
      tags: { name: 'POST /v1/auth/login' },
    },
  );
  if (res.status !== 200) {
    fail(`login of ${email} answered ${res.status}: is the seed loaded? (just loadtest-seed)`);
  }
  const body = res.json();
  if (!body.access_token) {
    fail(`login of ${email} did not return a token (MFA enabled?)`);
  }
  const session = {
    slug,
    email,
    token: body.access_token,
    expiresAt: Date.now() + body.expires_in * 1000,
    tenantId: null,
  };
  session.tenantId = tenantIdOf(session);
  return session;
}

function tenantIdOf(session) {
  const res = http.get(`${API_URL}/v1/tenants`, {
    headers: { Authorization: `Bearer ${session.token}` },
    tags: { name: 'GET /v1/tenants' },
  });
  if (res.status !== 200) fail(`GET /v1/tenants answered ${res.status}`);
  const tenant = res.json().find((t) => t.slug === session.slug);
  if (!tenant) fail(`${session.email} is not a member of ${session.slug}`);
  return tenant.id;
}

const renewed = {};

/**
 * Returns a usable session for `seeded` (one from setup data): the same one while its token is
 * valid, or a fresh login cached per VU once it is about to expire.
 */
export function fresh(seeded) {
  const current = renewed[seeded.slug] || seeded;
  if (current.expiresAt - Date.now() > RENEW_MARGIN_S * 1000) return current;
  const next = login(seeded.slug);
  renewed[seeded.slug] = next;
  return next;
}
