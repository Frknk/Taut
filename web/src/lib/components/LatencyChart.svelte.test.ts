import { describe, expect, it } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import LatencyChart from '#lib/components/LatencyChart.svelte';
import { expectAccessible } from '#lib/test/axe.js';

const from = Date.parse('2026-10-16T10:00:00Z');
const points = [0, 1, 2].map((i) => ({
  t: new Date(from + i * 900_000).toISOString(),
  checks: 30,
  failures: i === 2 ? 3 : 0,
  avg_ms: 40 + i,
  p95_ms: 80 + i,
}));

describe('LatencyChart table toggle', () => {
  it('is a toggle button with a fixed name that shows the data as a table', async () => {
    await render(LatencyChart, { points, bucketS: 900, from, to: from + 2_700_000 });
    const toggle = page.getByRole('button', { name: 'Ver tabla' });
    await expect.element(toggle).toHaveAttribute('aria-pressed', 'false');
    await userEvent.click(toggle);
    await expect.element(toggle).toHaveAttribute('aria-pressed', 'true');
    await expect.element(page.getByRole('table')).toBeVisible();
    await expect.element(page.getByRole('columnheader', { name: 'Fallas' })).toBeVisible();
    await expectAccessible();
    await userEvent.click(toggle);
    await expect.element(page.getByRole('table')).not.toBeInTheDocument();
  });

  it('lets the keyboard reach the table once it scrolls', async () => {
    const many = Array.from({ length: 60 }, (_, i) => ({
      t: new Date(from + i * 900_000).toISOString(),
      checks: 30,
      failures: 0,
      avg_ms: 40,
      p95_ms: 80,
    }));
    await render(LatencyChart, { points: many, bucketS: 900, from, to: from + 60 * 900_000 });
    await userEvent.click(page.getByRole('button', { name: 'Ver tabla' }));
    const region = page.getByRole('region', { name: 'Latencia por intervalo, en tabla' });
    await expect.element(region).toHaveAttribute('tabindex', '0');
    await userEvent.keyboard('{Tab}');
    await expect.element(region).toHaveFocus();
    await expectAccessible();
  });
});
