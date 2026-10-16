// Public surface of `#lib`; feature modules are also importable directly (`#lib/health.js`).
export { describeHealth, type Health } from './health.js';
export { API_URL, ApiError, describeError, request } from './api.js';
