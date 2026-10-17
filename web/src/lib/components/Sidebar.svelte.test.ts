import { beforeEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import Sidebar from '#lib/components/Sidebar.svelte';
import { session } from '#lib/session.svelte.js';
import { expectAccessible } from '#lib/test/axe.js';
import { selectedTenant } from '#lib/tenant.svelte.js';
import type { TenantWithRole, User } from '#lib/types.js';

// The sidebar talks to the api and the router; the test only looks at what it renders.
const { request } = vi.hoisted(() => ({ request: vi.fn() }));
vi.mock('#lib/api.js', async (original) => ({
  ...(await original<typeof import('#lib/api.js')>()),
  request,
}));
vi.mock('$app/navigation', () => ({ afterNavigate: vi.fn(), goto: vi.fn() }));

const tenants: TenantWithRole[] = [
  { id: 't-1', slug: 'beta', name: 'Beta Labs', role: 'owner' },
  { id: 't-2', slug: 'casa', name: 'Casa', role: 'member' },
] as TenantWithRole[];

beforeEach(() => {
  request.mockImplementation((_method: string, path: string) =>
    Promise.resolve(path === '/v1/tenants' ? tenants : []),
  );
  session.user = { id: 'u-1', name: 'Beto', email: 'beto@example.com' } as User;
  session.status = 'authenticated';
  selectedTenant.select(tenants[0]);
  selectedTenant.list = tenants;
  selectedTenant.loaded = true;
});

describe('Sidebar', () => {
  it('passes axe with its navigation and both menus', async () => {
    await render(Sidebar);
    await expect.element(page.getByRole('navigation', { name: 'Secciones' })).toBeVisible();
    await expectAccessible();

    await userEvent.click(page.getByRole('button', { name: /Organización:/ }));
    await expect
      .element(page.getByRole('menuitemradio', { name: /Beta Labs/ }))
      .toHaveAttribute('aria-checked', 'true');
    await expectAccessible();
    await userEvent.keyboard('{Escape}');

    await userEvent.click(page.getByRole('button', { name: /Cuenta:/ }));
    await expect.element(page.getByRole('group', { name: 'Tema' })).toBeVisible();
    await expectAccessible();
  });
});
