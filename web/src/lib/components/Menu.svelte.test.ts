import { describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import MenuHarness from '#lib/test/MenuHarness.svelte';
import { expectAccessible } from '#lib/test/axe.js';

async function openWithKeyboard(onaction = vi.fn()) {
  await render(MenuHarness, { onaction });
  const trigger = page.getByRole('button', { name: 'Abrir' });
  (trigger.element() as HTMLElement).focus();
  await userEvent.keyboard('{Enter}');
  await expect.element(page.getByRole('menu', { name: 'Frutas' })).toBeVisible();
  return { trigger, onaction };
}

describe('Menu', () => {
  it('opens from the keyboard on the first item, labelled, with a radio group', async () => {
    const { trigger } = await openWithKeyboard();
    await expect.element(trigger).toHaveAttribute('aria-expanded', 'true');
    await expect.element(page.getByRole('menuitemradio', { name: 'Manzana' })).toHaveFocus();
    await expect
      .element(page.getByRole('menuitemradio', { name: 'Manzana' }))
      .toHaveAttribute('aria-checked', 'true');
    await expect.element(page.getByRole('group', { name: 'Fruta' })).toBeVisible();
    await expectAccessible();
  });

  it('moves with the arrows, Home, End and typeahead', async () => {
    await openWithKeyboard();
    await userEvent.keyboard('{ArrowDown}');
    await expect.element(page.getByRole('menuitemradio', { name: 'Banana' })).toHaveFocus();
    await userEvent.keyboard('{End}');
    await expect.element(page.getByRole('menuitem', { name: 'Salir' })).toHaveFocus();
    await userEvent.keyboard('{Home}');
    await expect.element(page.getByRole('menuitemradio', { name: 'Manzana' })).toHaveFocus();
    await userEvent.keyboard('p');
    await expect.element(page.getByRole('menuitem', { name: 'Perfil' })).toHaveFocus();
  });

  it('closes with Escape and gives the focus back to the trigger', async () => {
    const { trigger } = await openWithKeyboard();
    await userEvent.keyboard('{Escape}');
    await expect.element(page.getByRole('menu')).not.toBeInTheDocument();
    await expect.element(trigger).toHaveFocus();
  });

  it('checks a radio item and closes, unless the item keeps it open', async () => {
    await openWithKeyboard();
    await userEvent.click(page.getByRole('menuitemradio', { name: 'Banana' }));
    await expect.element(page.getByRole('menu')).not.toBeInTheDocument();
    await expect.element(page.getByRole('status')).toHaveTextContent('banana');

    await userEvent.click(page.getByRole('button', { name: 'Abrir' }));
    await userEvent.click(page.getByRole('menuitemradio', { name: 'Cereza' }));
    await expect.element(page.getByRole('menu')).toBeVisible();
    await expect
      .element(page.getByRole('menuitemradio', { name: 'Cereza' }))
      .toHaveAttribute('aria-checked', 'true');
  });

  it('runs an action item with Enter', async () => {
    const { onaction } = await openWithKeyboard();
    await userEvent.keyboard('{End}{Enter}');
    expect(onaction).toHaveBeenCalledOnce();
    await expect.element(page.getByRole('menu')).not.toBeInTheDocument();
  });

  it('renders a link item as a link', async () => {
    await openWithKeyboard();
    await expect
      .element(page.getByRole('menuitem', { name: 'Perfil' }))
      .toHaveAttribute('href', '#perfil');
  });
});
