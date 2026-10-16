import { describe, expect, it } from 'vitest';

import { expiresAtMs, refreshDelayMs } from './session-timing';

describe('refreshDelayMs', () => {
  it('refreshes a minute before expiry', () => {
    const now = 1_000_000;
    expect(refreshDelayMs(now + 15 * 60_000, now)).toBe(14 * 60_000);
  });

  it('never schedules sooner than a few seconds', () => {
    const now = 1_000_000;
    expect(refreshDelayMs(now + 30_000, now)).toBe(5_000);
    expect(refreshDelayMs(now - 1, now)).toBe(5_000);
  });
});

describe('expiresAtMs', () => {
  it('converts the lifetime to an instant', () => {
    expect(expiresAtMs(900, 1_000)).toBe(901_000);
  });
});
