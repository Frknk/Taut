import { describe, expect, it } from 'vitest';

import { STATUS_GLYPHS, STATUS_TONES, formatSeconds, timeAgo } from './format';

describe('formatSeconds', () => {
  it('uses the largest sensible unit', () => {
    expect(formatSeconds(30)).toBe('30 s');
    expect(formatSeconds(60)).toBe('1 min');
    expect(formatSeconds(90)).toBe('1 min 30 s');
    expect(formatSeconds(3600)).toBe('1 h');
    expect(formatSeconds(5400)).toBe('1 h 30 min');
  });
});

describe('timeAgo', () => {
  it('describes elapsed time in Spanish', () => {
    const now = new Date('2026-10-15T12:00:00Z');
    expect(timeAgo('2026-10-15T11:59:55Z', now)).toBe('hace 5 s');
    expect(timeAgo('2026-10-15T11:57:00Z', now)).toBe('hace 3 min');
    expect(timeAgo('2026-10-15T09:00:00Z', now)).toBe('hace 3 h');
    expect(timeAgo('2026-10-11T12:00:00Z', now)).toBe('hace 4 d');
  });
});

describe('status badges', () => {
  it('colors only the states that need attention', () => {
    expect(STATUS_TONES.up).toBe('success');
    expect(STATUS_TONES.down).toBe('danger');
    expect(STATUS_TONES.paused).toBe('neutral');
    expect(STATUS_TONES.pending).toBe('outline');
  });

  it('gives statuses that share a tone a different shape', () => {
    expect(STATUS_GLYPHS.up).toBe(STATUS_GLYPHS.down);
    expect(STATUS_TONES.up).not.toBe(STATUS_TONES.down);
    expect(STATUS_GLYPHS.paused).not.toBe(STATUS_GLYPHS.pending);
  });
});
