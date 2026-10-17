import { describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import MonitorForm from '#lib/components/MonitorForm.svelte';
import { emptyForm } from '#lib/monitor-form.js';
import { expectAccessible } from '#lib/test/axe.js';

function setup(overrides = {}, editing = false) {
  const onsubmit = vi.fn(() => Promise.resolve());
  const initial = { ...emptyForm(), name: 'Web', target: 'https://example.com', ...overrides };
  return render(MonitorForm, { initial, editing, submitLabel: 'Crear', onsubmit });
}

describe('MonitorForm type', () => {
  it('is a labelled radio group that moves with the arrows', async () => {
    await setup();
    const group = page.getByRole('radiogroup', { name: 'Tipo' });
    await expect.element(group).toBeVisible();
    const http = page.getByRole('radio', { name: 'HTTP' });
    await expect.element(http).toHaveAttribute('aria-checked', 'true');
    (http.element() as HTMLElement).focus();
    await userEvent.keyboard('{ArrowRight}');
    await expect.element(page.getByRole('radio', { name: 'Palabra clave' })).toHaveFocus();
    await expect
      .element(page.getByRole('radio', { name: 'Palabra clave' }))
      .toHaveAttribute('aria-checked', 'true');
    // The keyword type adds its own field.
    await expect.element(page.getByLabelText('Texto esperado')).toBeVisible();
    await expectAccessible();
  });

  it('is disabled and explained while editing', async () => {
    await setup({}, true);
    const group = page.getByRole('radiogroup', { name: 'Tipo' });
    await expect.element(group).toHaveAccessibleDescription(/no se puede cambiar/);
    await expect.element(page.getByRole('radio', { name: 'Heartbeat' })).toBeDisabled();
  });
});

describe('MonitorForm periods', () => {
  it('fold and unfold from the keyboard', async () => {
    await setup();
    const trigger = page.getByRole('button', { name: 'Confirmación y recuperación' });
    await expect.element(trigger).toHaveAttribute('aria-expanded', 'false');
    await expect.element(page.getByLabelText('Periodo de confirmación (s)')).not.toBeVisible();
    (trigger.element() as HTMLElement).focus();
    await userEvent.keyboard('{Enter}');
    await expect.element(trigger).toHaveAttribute('aria-expanded', 'true');
    await expect.element(page.getByLabelText('Periodo de confirmación (s)')).toBeVisible();
    await expectAccessible();
  });

  it('unfold by themselves when one of them has an error', async () => {
    await setup({ confirmation_period_s: 99_999 });
    await userEvent.click(page.getByRole('button', { name: 'Crear' }));
    await expect.element(page.getByText('Entre 0 y 3600 segundos.')).toBeVisible();
    await expect
      .element(page.getByRole('button', { name: 'Confirmación y recuperación' }))
      .toHaveAttribute('aria-expanded', 'true');
  });
});
