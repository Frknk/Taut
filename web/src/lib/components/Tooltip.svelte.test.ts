import { describe, expect, it } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import TooltipHarness from '#lib/test/TooltipHarness.svelte';
import { expectAccessible } from '#lib/test/axe.js';

describe('Tooltip', () => {
  it('opens on keyboard focus, describes its text and closes with Escape', async () => {
    await render(TooltipHarness);
    (page.getByRole('button', { name: 'Antes' }).element() as HTMLElement).focus();
    await userEvent.tab();
    const trigger = page.getByText('hace 2 min');
    await expect.element(trigger).toHaveFocus();
    await expect.element(page.getByRole('tooltip')).toHaveTextContent('16/10/26, 21:09');
    await expect.element(trigger).toHaveAccessibleDescription('16/10/26, 21:09');
    await expectAccessible();
    await userEvent.keyboard('{Escape}');
    await expect.element(page.getByRole('tooltip')).not.toBeInTheDocument();
  });

  it('opens on hover', async () => {
    await render(TooltipHarness);
    await userEvent.hover(page.getByText('hace 2 min'));
    await expect.element(page.getByRole('tooltip')).toBeVisible();
  });
});
