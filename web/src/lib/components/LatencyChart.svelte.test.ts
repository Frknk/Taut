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
});
