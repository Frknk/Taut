import { describe, expect, it } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import Landing from '#lib/components/Landing.svelte';
import { expectAccessible } from '#lib/test/axe.js';

describe('Landing', () => {
  it('invites to sign up and tells the story of a monitor, then is accessible', async () => {
    await render(Landing);
    await expect
      .element(page.getByRole('heading', { level: 1 }))
      .toHaveTextContent('Entérate antes que tus usuarios.');
    await expect
      .element(page.getByRole('link', { name: 'Crear cuenta' }).first())
      .toHaveAttribute('href', '/register');
    await expect
      .element(page.getByRole('link', { name: 'Entrar' }).first())
      .toHaveAttribute('href', '/login');

    // The story: the monitor starts without data and ends, three seconds in, with the alert.
    await expect.element(page.getByText('Sin datos aún')).toBeInTheDocument();
    await expect.element(page.getByText('Operativo'), { timeout: 5000 }).toBeInTheDocument();
    await expect
      .element(page.getByText('Caída de 3 min, resuelta.'), { timeout: 5000 })
      .toBeVisible();
    await expectAccessible();
  });
});
