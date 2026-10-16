import { describe, expect, it } from 'vitest';

import {
  formatLatency,
  formatPercent,
  nearestIndex,
  niceCeil,
  scale,
  segments,
  ticks,
  uptimeLevel,
} from './charts';

describe('niceCeil and ticks', () => {
  it('rounds the axis maximum up to 1, 2 or 5 times a power of ten', () => {
    expect(niceCeil(0)).toBe(1);
    expect(niceCeil(87)).toBe(100);
    expect(niceCeil(130)).toBe(200);
    expect(niceCeil(420)).toBe(500);
    expect(niceCeil(1000)).toBe(1000);
    expect(ticks(200)).toEqual([0, 50, 100, 150, 200]);
  });
});

describe('scale', () => {
  it('maps a domain onto a range, inverted for SVG y', () => {
    const y = scale(0, 100, 200, 0);
    expect(y(0)).toBe(200);
    expect(y(50)).toBe(100);
    expect(y(100)).toBe(0);
  });
});

const at = (minute: number) => new Date(Date.UTC(2026, 9, 16, 0, minute)).toISOString();

describe('segments', () => {
  const bucket = 60_000;

  it('breaks the line at missing buckets and null values', () => {
    const points = [
      { t: at(0), v: 1 },
      { t: at(1), v: 2 },
      { t: at(3), v: 3 },
      { t: at(4), v: null },
      { t: at(5), v: 5 },
    ];
    const runs = segments(points, (p) => p.v, bucket);
    expect(runs.map((run) => run.map((p) => p.v))).toEqual([[1, 2], [3], [5]]);
  });
});

describe('nearestIndex', () => {
  it('finds the closest time', () => {
    expect(nearestIndex([0, 10, 20], 14)).toBe(1);
    expect(nearestIndex([0, 10, 20], 16)).toBe(2);
    expect(nearestIndex([], 5)).toBe(-1);
  });
});

describe('uptime levels and formats', () => {
  it('classifies uptime like the status page legend', () => {
    expect(uptimeLevel(null)).toBe('none');
    expect(uptimeLevel(100)).toBe('good');
    expect(uptimeLevel(99.9)).toBe('good');
    expect(uptimeLevel(99.5)).toBe('warning');
    expect(uptimeLevel(97)).toBe('serious');
    expect(uptimeLevel(50)).toBe('critical');
  });

  it('formats percentages and latencies in Spanish', () => {
    expect(formatPercent(99.954)).toBe('99,95 %');
    expect(formatPercent(100)).toBe('100 %');
    expect(formatPercent(null)).toBe('—');
    expect(formatLatency(123.4)).toBe('123 ms');
    expect(formatLatency(1234)).toBe('1,2 s');
  });
});
