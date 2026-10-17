// Where the panel goes when the user switches tenant, and which tenant `/` opens on its own.

import type { TenantWithRole } from './types.js';

// Sections whose second segment is a page of its own, not a resource of the old tenant.
const NESTED_SECTIONS = new Set(['settings']);

/**
 * The page to show after switching tenant: the same section, but never a resource of the old
 * tenant. `/incidents` stays, `/settings/members` stays, `/monitors/abc/edit` goes to `/monitors`.
 */
export function sectionAfterSwitch(pathname: string): string {
  const [first, second] = pathname.split('/').filter(Boolean);
  if (!first) return '/monitors';
  if (NESTED_SECTIONS.has(first)) return second ? `/${first}/${second}` : '/settings/profile';
  return `/${first}`;
}

/**
 * The tenant to open without asking: the remembered one while the user still belongs to it, or
 * the only one. `null` means the user has to choose (several tenants) or create one (none).
 */
export function pickTenant(
  tenants: TenantWithRole[],
  rememberedId: string | null,
): TenantWithRole | null {
  const remembered = tenants.find((tenant) => tenant.id === rememberedId);
  if (remembered) return remembered;
  return tenants.length === 1 ? (tenants[0] ?? null) : null;
}
