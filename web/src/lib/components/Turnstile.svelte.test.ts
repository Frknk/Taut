// The widget itself (and its keyboard handling) lives in Cloudflare's iframe; these tests
// replace `window.turnstile` with a stub so they run without the network and check the part
// Taut owns: the options, the bound token, reset, cleanup and the error message.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';

import TurnstileHarness from '#lib/test/TurnstileHarness.svelte';
import { expectAccessible } from '#lib/test/axe.js';
import type { RenderOptions } from '#lib/turnstile.js';

function stubTurnstile() {
  let options: RenderOptions | undefined;
  const api = {
    render: vi.fn((container: HTMLElement, given: RenderOptions) => {
      options = given;
      container.append(Object.assign(document.createElement('p'), { textContent: 'Widget' }));
      return 'widget-1';
    }),
    reset: vi.fn(),
    remove: vi.fn(),
  };
  window.turnstile = api;
  return { api, options: () => options };
}

afterEach(() => {
  delete window.turnstile;
  delete document.documentElement.dataset.theme;
});

describe('Turnstile', () => {
  it('renders the widget and binds, resets and removes its token', async () => {
    const { api, options } = stubTurnstile();
    document.documentElement.dataset.theme = 'dark';
    await render(TurnstileHarness);

    await expect.element(page.getByText('Widget')).toBeInTheDocument();
    expect(options()).toMatchObject({
      sitekey: 'test-sitekey',
      action: 'login',
      theme: 'dark',
      size: 'flexible',
      language: 'es',
    });

    options()?.callback?.('token-1');
    await expect.element(page.getByText('Token: token-1')).toBeInTheDocument();
    await expectAccessible();

    await userEvent.click(page.getByRole('button', { name: 'Reiniciar' }));
    expect(api.reset).toHaveBeenCalledWith('widget-1');
    await expect.element(page.getByText('Token: ninguno')).toBeInTheDocument();

    options()?.callback?.('token-2');
    options()?.['expired-callback']?.();
    await expect.element(page.getByText('Token: ninguno')).toBeInTheDocument();

    await userEvent.click(page.getByRole('button', { name: 'Quitar' }));
    expect(api.remove).toHaveBeenCalledWith('widget-1');
  });

  it('says so when the challenge fails', async () => {
    const { options } = stubTurnstile();
    await render(TurnstileHarness);
    await expect.element(page.getByText('Widget')).toBeInTheDocument();
    expect(options()?.theme).toBe('auto');

    options()?.['error-callback']?.();
    await expect
      .element(page.getByText('No se pudo completar la verificación. Recarga la página.'))
      .toBeInTheDocument();
    await expect.element(page.getByText('Token: ninguno')).toBeInTheDocument();
    await expectAccessible();
  });

  it('renders nothing without a sitekey', async () => {
    const { api } = stubTurnstile();
    await render(TurnstileHarness, { sitekey: '' });
    await expect.element(page.getByText('Token: ninguno')).toBeInTheDocument();
    expect(api.render).not.toHaveBeenCalled();
  });
});
