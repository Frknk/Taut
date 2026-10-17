// Tenant selected in the panel: the user picks one of their organizations and every call to
// the api sends its id in `X-Tenant-Id`. The choice survives reloads in localStorage (it is
// not a secret: the api checks the membership on every request). The list of the user's
// tenants feeds the switcher in the sidebar.

import { request } from './api.js';
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
  /** The user's tenants, as of the last `refresh`. */
  list = $state<TenantWithRole[]>([]);
  /** Whether `list` came from the api at least once. */
  loaded = $state(false);
  /**
   * True while the panel moves to another tenant. The layout unmounts the page meanwhile, so
   * a page of the old tenant never asks for its data with the new tenant's id.
   */
  switching = $state(false);

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

  /**
   * Fetches the user's tenants. The selection follows what the api says: a renamed tenant or
   * a changed role updates it, a tenant the user left clears it. Throws what `request` throws.
   */
  async refresh(): Promise<TenantWithRole[]> {
    const tenants = await request<TenantWithRole[]>('GET', '/v1/tenants');
    this.list = tenants;
    this.loaded = true;
    const current = this.current;
    if (current) {
      const fresh = tenants.find((tenant) => tenant.id === current.id);
      if (!fresh) this.clear();
      else if (
        fresh.name !== current.name ||
        fresh.role !== current.role ||
        fresh.slug !== current.slug
      ) {
        this.select(fresh);
      }
    }
    return tenants;
  }

  /** Forgets the list too: on logout. */
  reset(): void {
    this.clear();
    this.list = [];
    this.loaded = false;
  }
}

export const selectedTenant = new TenantSelection();
