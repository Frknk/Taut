import { describe, expect, it } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import DialogHarness from '#lib/test/DialogHarness.svelte';
import { expectAccessible } from '#lib/test/axe.js';

// The scrim, as bits-ui marks it in each variant.
function clickOverlay() {
  const overlay = document.querySelector<HTMLElement>(
    '[data-dialog-overlay], [data-alert-dialog-overlay]',
  );
  if (!overlay) throw new Error('no overlay');
  return userEvent.click(overlay, { position: { x: 5, y: 5 } });
}

async function openDialog(alert = false) {
  const screen = await render(DialogHarness, { alert });
  const trigger = screen.getByRole('button', { name: 'Abrir' });
  await userEvent.click(trigger);
  return { screen, trigger };
}

describe('Dialog', () => {
  it('is a labelled modal dialog that starts on the first field', async () => {
    await openDialog();
    const dialog = page.getByRole('dialog', { name: 'Borrar el monitor' });
    await expect.element(dialog).toBeVisible();
    await expect.element(dialog).toHaveAccessibleDescription('No se puede deshacer.');
    await expect.element(page.getByLabelText('Nombre')).toHaveFocus();
    await expectAccessible();
  });

  it('keeps the focus inside while tabbing', async () => {
    await openDialog();
    const dialog = page.getByRole('dialog').element();
    for (let i = 0; i < 6; i += 1) {
      await userEvent.tab();
      expect(dialog.contains(document.activeElement)).toBe(true);
    }
  });

  it('closes with Escape and gives the focus back to the trigger', async () => {
    const { trigger } = await openDialog();
    await userEvent.keyboard('{Escape}');
    await expect.element(page.getByRole('dialog')).not.toBeInTheDocument();
    await expect.element(trigger).toHaveFocus();
  });

  it('closes with a click outside', async () => {
    await openDialog();
    await clickOverlay();
    await expect.element(page.getByRole('dialog')).not.toBeInTheDocument();
  });

  it('closes with the X button', async () => {
    await openDialog();
    await userEvent.click(page.getByRole('button', { name: 'Cerrar' }));
    await expect.element(page.getByRole('dialog')).not.toBeInTheDocument();
  });
});

describe('Dialog alert', () => {
  it('is an alertdialog that starts on the safe button', async () => {
    await openDialog(true);
    await expect
      .element(page.getByRole('alertdialog', { name: 'Borrar el monitor' }))
      .toBeVisible();
    await expect.element(page.getByRole('button', { name: 'Cancelar' })).toHaveFocus();
    await expectAccessible();
  });

  it('ignores a click outside but still closes with Escape', async () => {
    await openDialog(true);
    const alert = page.getByRole('alertdialog');
    await clickOverlay();
    await expect.element(alert).toBeVisible();
    await userEvent.keyboard('{Escape}');
    await expect.element(alert).not.toBeInTheDocument();
  });
});
