// Public surface of `#lib`; feature modules are also importable directly (`#lib/health.js`).
export { describeHealth, type Health } from './health.js';
export { API_URL, AUTH_URL, ApiError, authRequest, describeError, request } from './api.js';
