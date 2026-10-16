// When to refresh the access token: a minute before it expires, never sooner than a few
// seconds from now (a very short token in development would otherwise spin).

export const REFRESH_MARGIN_MS = 60_000;
export const MIN_REFRESH_DELAY_MS = 5_000;

/** Milliseconds until the next silent refresh for a token that expires at `expiresAt`. */
export function refreshDelayMs(expiresAt: number, now: number = Date.now()): number {
  return Math.max(expiresAt - now - REFRESH_MARGIN_MS, MIN_REFRESH_DELAY_MS);
}

/** When a token that lives `expiresIn` seconds from `now` expires, in epoch milliseconds. */
export function expiresAtMs(expiresIn: number, now: number = Date.now()): number {
  return now + expiresIn * 1000;
}
