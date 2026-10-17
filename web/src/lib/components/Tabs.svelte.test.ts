import { describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import TabsHarness from '#lib/test/TabsHarness.svelte';
import { expectAccessible } from '#lib/test/axe.js';

describe('Tabs', () => {
  it('is a labelled tablist whose selected tab controls the panel', async () => {
    await render(TabsHarness);
    await expect.element(page.getByRole('tablist', { name: 'Rango' })).toBeVisible();
    const selected = page.getByRole('tab', { name: '24 h' });
    await expect.element(selected).toHaveAttribute('aria-selected', 'true');
    const panel = page.getByRole('tabpanel', { name: '24 h' });
    await expect.element(panel).toHaveTextContent('Datos de 24h');
    await expect
      .element(selected)
      .toHaveAttribute('aria-controls', (panel.element() as HTMLElement).id);
    await expectAccessible();
  });

  it('moves the selection with the arrows, Home and End', async () => {
    const onchange = vi.fn();
    await render(TabsHarness, { onchange });
    (page.getByRole('tab', { name: '24 h' }).element() as HTMLElement).focus();
    await userEvent.keyboard('{ArrowRight}');
    await expect.element(page.getByRole('tab', { name: '7 días' })).toHaveFocus();
    await expect
      .element(page.getByRole('tabpanel', { name: '7 días' }))
      .toHaveTextContent('Datos de 7d');
    await userEvent.keyboard('{End}');
    await expect
      .element(page.getByRole('tab', { name: '30 días' }))
      .toHaveAttribute('aria-selected', 'true');
    await userEvent.keyboard('{Home}');
    await expect.element(page.getByRole('tabpanel', { name: '24 h' })).toBeVisible();
    // Only the selected tab is in the tab order.
    await expect
      .element(page.getByRole('tab', { name: '7 días' }))
      .toHaveAttribute('tabindex', '-1');
    expect(onchange.mock.calls.map(([value]) => value)).toEqual(['7d', '30d', '24h']);
  });
});
