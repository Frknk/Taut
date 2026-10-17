import { describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import IncidentList from '#lib/components/IncidentList.svelte';
import { expectAccessible } from '#lib/test/axe.js';
import type { Incident, IncidentDetail } from '#lib/types.js';

const { request } = vi.hoisted(() => ({ request: vi.fn() }));
vi.mock('#lib/api.js', async (original) => ({
  ...(await original<typeof import('#lib/api.js')>()),
  request,
}));

const incident: Incident = {
  id: 'inc-1',
  monitor_id: 'mon-1',
  monitor_name: 'Web',
  started_at: '2026-10-16T10:00:00Z',
  resolved_at: null,
  cause: 'timeout',
  duration_s: 120,
};

const detail: IncidentDetail = {
  ...incident,
  deliveries: [
    {
      channel_id: 'ch-1',
      channel_name: 'Guardia',
      channel_type: 'email',
      event: 'opened',
      status: 'sent',
      attempts: 1,
      last_error: null,
      updated_at: '2026-10-16T10:00:05Z',
    },
  ],
};

describe('IncidentList', () => {
  it('is a list of headings whose trigger expands the alerts it sent', async () => {
    request.mockResolvedValue(detail);
    await render(IncidentList, { incidents: [incident], tenantId: 't-1', headingLevel: 2 });
    const heading = page.getByRole('heading', { level: 2 });
    await expect.element(heading).toBeVisible();
    expect(heading.element().textContent).toContain('Abierto');
    // The monitor link sits beside the trigger, not inside it.
    const trigger = page.getByRole('button', { name: /timeout/ });
    await expect.element(page.getByRole('link', { name: 'Web' })).toBeVisible();
    expect((trigger.element() as HTMLElement).querySelector('a')).toBeNull();
    await expect.element(trigger).toHaveAttribute('aria-expanded', 'false');
    await expectAccessible();

    (trigger.element() as HTMLElement).focus();
    await userEvent.keyboard('{Enter}');
    await expect.element(trigger).toHaveAttribute('aria-expanded', 'true');
    const region = page.getByRole('region', { name: /timeout/ });
    await expect.element(region.getByText(/Guardia/)).toBeVisible();
    await expect
      .element(trigger)
      .toHaveAttribute('aria-controls', (region.element() as HTMLElement).id);
    expect(request).toHaveBeenCalledWith('GET', '/v1/incidents/inc-1', { tenant: 't-1' });
    await expectAccessible();

    await userEvent.keyboard('{Enter}');
    await expect.element(trigger).toHaveAttribute('aria-expanded', 'false');
  });
});
