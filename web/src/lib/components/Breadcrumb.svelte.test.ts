import { describe, expect, it } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import Breadcrumb from '#lib/components/Breadcrumb.svelte';
import { crumbsFor } from '#lib/breadcrumb.js';
import { expectAccessible } from '#lib/test/axe.js';

describe('Breadcrumb', () => {
  it('links the parents, marks the current page and moves between links with Tab', async () => {
    await page.viewport(1100, 600);
    await render(Breadcrumb, { crumbs: crumbsFor('/monitors/0192/edit', 'Sitio') });
    const nav = page.getByRole('navigation', { name: 'Ruta' });
    await expect.element(nav).toHaveTextContent('Monitoreo Monitores Sitio Editar');
    await expect.element(page.getByText('Editar')).toHaveAttribute('aria-current', 'page');
    await expect.element(page.getByText('Monitoreo')).not.toHaveAttribute('href');

    // Tab goes from one parent to the next; the current page is not a stop.
    (page.getByRole('link', { name: 'Monitores' }).element() as HTMLElement).focus();
    await userEvent.tab();
    const monitor = page.getByRole('link', { name: 'Sitio' });
    await expect.element(monitor).toHaveFocus();
    await expect.element(monitor).toHaveAttribute('href', '/monitors/0192');
    await expectAccessible();
  });

  it('keeps only the parent and the current page on narrow screens', async () => {
    await page.viewport(414, 800);
    await render(Breadcrumb, { crumbs: crumbsFor('/monitors/0192/edit', 'Sitio') });
    await expect.element(page.getByRole('link', { name: 'Sitio' })).toBeVisible();
    await expect.element(page.getByText('Editar')).toBeVisible();
    await expect.element(page.getByText('Monitores', { exact: true })).not.toBeVisible();
    await expect.element(page.getByText('Monitoreo')).not.toBeVisible();
  });

  it('renders nothing without steps', async () => {
    await render(Breadcrumb, { crumbs: [] });
    await expect.element(page.getByRole('navigation')).not.toBeInTheDocument();
  });
});
