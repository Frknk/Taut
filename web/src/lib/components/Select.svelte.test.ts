import { describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import SelectHarness from '#lib/test/SelectHarness.svelte';
import { expectAccessible } from '#lib/test/axe.js';

describe('Select', () => {
  it('is a combobox named by its label that shows the chosen option', async () => {
    await render(SelectHarness);
    const combobox = page.getByRole('combobox', { name: 'Frecuencia' });
    await expect.element(combobox).toHaveTextContent('Cada 1 min');
    await expect.element(combobox).toHaveAttribute('aria-expanded', 'false');
    await expectAccessible();
  });

  it('chooses with the keyboard, skips disabled options and keeps the value type', async () => {
    const onchange = vi.fn();
    await render(SelectHarness, { onchange });
    const combobox = page.getByRole('combobox', { name: 'Frecuencia' });
    (combobox.element() as HTMLElement).focus();
    await userEvent.keyboard('{Enter}');
    await expect.element(page.getByRole('listbox')).toBeVisible();
    await expect
      .element(page.getByRole('option', { name: 'Cada 1 min' }))
      .toHaveAttribute('aria-selected', 'true');
    await expectAccessible();
    // Down from «1 min» skips the disabled «5 min».
    await userEvent.keyboard('{ArrowDown}{Enter}');
    await expect.element(page.getByRole('listbox')).not.toBeInTheDocument();
    await expect.element(combobox).toHaveTextContent('Cada 10 min');
    await expect.element(combobox).toHaveFocus();
    await expect.element(page.getByRole('status')).toHaveTextContent('600');
    expect(onchange).toHaveBeenCalledExactlyOnceWith(600);
  });

  it('closes with Escape without changing the value', async () => {
    await render(SelectHarness);
    const combobox = page.getByRole('combobox', { name: 'Frecuencia' });
    await userEvent.click(combobox);
    await userEvent.keyboard('{ArrowUp}{Escape}');
    await expect.element(page.getByRole('listbox')).not.toBeInTheDocument();
    await expect.element(combobox).toHaveTextContent('Cada 1 min');
  });
});
