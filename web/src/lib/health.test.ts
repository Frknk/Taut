import { describe, expect, it } from 'vitest';

import { describeHealth } from './health';

describe('describeHealth', () => {
  it('describes a healthy API', () => {
    const health = {
      status: 'ok',
      service: 'taut-api',
      version: '0.1.0',
      commit: 'abc1234',
      env: 'dev',
    };
    expect(describeHealth(health)).toBe('API en línea · taut-api v0.1.0 (abc1234)');
  });

  it('reports an unreachable API', () => {
    expect(describeHealth(null)).toBe('API no disponible');
  });

  it('treats any status other than ok as unavailable', () => {
    const health = {
      status: 'degraded',
      service: 'taut-api',
      version: '0.1.0',
      commit: 'abc1234',
      env: 'dev',
    };
    expect(describeHealth(health)).toBe('API no disponible');
  });
});
