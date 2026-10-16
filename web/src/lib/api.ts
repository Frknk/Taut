/** Base URL of taut-api, taken from `PUBLIC_API_URL` at build time. */
export const API_URL: string = (
  (import.meta.env.PUBLIC_API_URL as string | undefined) ?? 'http://localhost:8080'
).replace(/\/$/, '');
