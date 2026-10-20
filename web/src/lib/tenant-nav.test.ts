import { describe, expect, it } from 'vitest';

import { pickTenant, sectionAfterSwitch } from './tenant-nav';
import type { TenantWithRole } from './types';

describe('sectionAfterSwitch', () => {
  it('keeps the section of a list page', () => {
    expect(sectionAfterSwitch('/monitors')).toBe('/monitors');
    expect(sectionAfterSwitch('/incidents')).toBe('/incidents');
    expect(sectionAfterSwitch('/channels')).toBe('/channels');
  });

  it('drops the resource of the old tenant', () => {
    expect(sectionAfterSwitch('/monitors/0192/edit')).toBe('/monitors');
    expect(sectionAfterSwitch('/monitors/0192')).toBe('/monitors');
    expect(sectionAfterSwitch('/monitors/new')).toBe('/monitors');
  });

  it('keeps the settings page', () => {
    expect(sectionAfterSwitch('/settings/members')).toBe('/settings/members');
    expect(sectionAfterSwitch('/settings/api-keys/')).toBe('/settings/api-keys');
    expect(sectionAfterSwitch('/settings')).toBe('/settings/profile');
  });

  it('falls back to the monitors', () => {
    expect(sectionAfterSwitch('/')).toBe('/monitors');
  });
});

function tenant(id: string): TenantWithRole {
  const limits = {
    monitors: 10,
    heartbeats: 10,
    api_keys: 20,
    api_key_requests_per_minute: 60,
    panel_requests_per_minute: 600,
  };
  return { id, slug: id, name: id, plan: 'free', created_at: '', role: 'owner', limits };
}

describe('pickTenant', () => {
  it('opens the remembered tenant while the user belongs to it', () => {
    expect(pickTenant([tenant('a'), tenant('b')], 'b')?.id).toBe('b');
  });

  it('opens the only tenant', () => {
    expect(pickTenant([tenant('a')], null)?.id).toBe('a');
    expect(pickTenant([tenant('a')], 'gone')?.id).toBe('a');
  });

  it('asks when there are several or none', () => {
    expect(pickTenant([tenant('a'), tenant('b')], null)).toBeNull();
    expect(pickTenant([tenant('a'), tenant('b')], 'gone')).toBeNull();
    expect(pickTenant([], null)).toBeNull();
  });
});
