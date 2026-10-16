// Tenant selected in the panel: the user picks one of their organizations and every call to
// the api sends its id in `X-Tenant-Id`. The choice survives reloads in localStorage (it is
// not a secret: the api checks the membership on every request).

import type { TenantWithRole } from './types.js';

const STORAGE_KEY = 'taut.tenant';

type SelectedTenant = Pick<TenantWithRole, 'id' | 'slug' | 'name' | 'role'>;

function load(): SelectedTenant | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as SelectedTenant) : null;
  } catch {
    return null;
  }
}

class TenantSelection {
  current = $state<SelectedTenant | null>(load());

  /** Whether the selected tenant lets the user manage channels, members and the tenant. */
  get isOwner(): boolean {
    return this.current?.role === 'owner';
  }

  select(tenant: SelectedTenant): void {
    this.current = { id: tenant.id, slug: tenant.slug, name: tenant.name, role: tenant.role };
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.current));
    } catch {
      // Storage unavailable (private window): the choice lasts until reload.
    }
  }

  clear(): void {
    this.current = null;
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {
      // Nothing stored.
    }
  }
}

export const selectedTenant = new TenantSelection();
