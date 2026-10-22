// Pure helpers of the charts: scales, ticks, line segments and uptime levels.

/** Smallest "nice" number (1, 2 or 5 × 10^n) at or above `value`; at least 1. */
export function niceCeil(value: number): number {
  if (!(value > 0)) return 1;
  const magnitude = 10 ** Math.floor(Math.log10(value));
  for (const step of [1, 2, 5, 10]) {
    if (step * magnitude >= value) return step * magnitude;
  }
  return 10 * magnitude;
}

/** `count + 1` evenly spaced ticks from 0 to `max`. */
export function ticks(max: number, count = 4): number[] {
  return Array.from({ length: count + 1 }, (_, index) => (max * index) / count);
}

/**
 * How many intervals split an axis that ends at a nice maximum so every tick lands on a round
 * value: 4 for 2 × 10^n (steps of 0,5, 5, 50…) and 5 for 1 and 5 × 10^n (steps of 2, 20… and
 * 1, 10…). With four intervals an axis up to 10 ms would read 3, 5, 8, 10.
 */
export function tickCount(max: number): number {
  if (!(max > 0)) return 4;
  const mantissa = max / 10 ** Math.floor(Math.log10(max) + 1e-9);
  return Math.round(mantissa) === 2 ? 4 : 5;
}

/** Linear map from `[d0, d1]` to `[r0, r1]`. */
export function scale(d0: number, d1: number, r0: number, r1: number): (value: number) => number {
  const span = d1 - d0 || 1;
  return (value) => r0 + ((value - d0) / span) * (r1 - r0);
}

/**
 * Runs of consecutive points that have a value: a missing bucket or a `null` value breaks
 * the line, so a gap in the data never looks like a straight interpolation.
 */
export function segments<T extends { t: string }>(
  points: T[],
  value: (point: T) => number | null,
  bucketMs: number,
): { t: number; v: number }[][] {
  const runs: { t: number; v: number }[][] = [];
  let current: { t: number; v: number }[] = [];
  let previous: number | null = null;
  for (const point of points) {
    const t = Date.parse(point.t);
    const v = value(point);
    const contiguous = previous !== null && t - previous <= bucketMs;
    if (v === null || !contiguous) {
      if (current.length > 0) runs.push(current);
      current = [];
    }
    if (v !== null) current.push({ t, v });
    previous = t;
  }
  if (current.length > 0) runs.push(current);
  return runs;
}

/** Index of the point whose time is closest to `t`; -1 for an empty list. */
export function nearestIndex(times: number[], t: number): number {
  let best = -1;
  let distance = Infinity;
  times.forEach((time, index) => {
    const d = Math.abs(time - t);
    if (d < distance) {
      distance = d;
      best = index;
    }
  });
  return best;
}

export type UptimeLevel = 'good' | 'warning' | 'serious' | 'critical' | 'none';

/** Status level of a day or period by its uptime percentage. */
export function uptimeLevel(uptime: number | null): UptimeLevel {
  if (uptime === null) return 'none';
  if (uptime >= 99.9) return 'good';
  if (uptime >= 99) return 'warning';
  if (uptime >= 95) return 'serious';
  return 'critical';
}

/** Legend of the uptime levels, best first. */
export const UPTIME_LEVELS: { level: UptimeLevel; label: string }[] = [
  { level: 'good', label: '≥ 99,9 %' },
  { level: 'warning', label: '≥ 99 %' },
  { level: 'serious', label: '≥ 95 %' },
  { level: 'critical', label: '< 95 %' },
  { level: 'none', label: 'Sin datos' },
];

/** `99,95 %` with up to two decimals, or a dash without data. */
export function formatPercent(value: number | null): string {
  if (value === null) return '—';
  return `${value.toLocaleString('es', { maximumFractionDigits: 2 })} %`;
}

/**
 * `123 ms` or `1,2 s`, or a dash without data. `decimals` is for axis ticks under a millisecond
 * apart (an axis up to 1 or 2 ms), which rounded to whole milliseconds would all read the same.
 */
export function formatLatency(ms: number | null, decimals = 0): string {
  if (ms === null) return '—';
  if (ms < 1000) return `${ms.toLocaleString('es', { maximumFractionDigits: decimals })} ms`;
  return `${(ms / 1000).toLocaleString('es', { maximumFractionDigits: 1 })} s`;
}
