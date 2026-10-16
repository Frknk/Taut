// Tenant selected in the panel. Until authentication exists (weeks 13–14) the panel picks a
// tenant and sends its id in `X-Tenant-Id`; the choice survives reloads in localStorage.

import type { Tenant } from './types.js';

const STORAGE_KEY = 'taut.tenant';

type SelectedTenant = Pick<Tenant, 'id' | 'slug' | 'name'>;

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

  select(tenant: SelectedTenant): void {
    this.current = { id: tenant.id, slug: tenant.slug, name: tenant.name };
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
