import { describe, expect, it } from 'vitest';

import { crumbsFor } from './breadcrumb';

describe('crumbsFor', () => {
  it('names the group and the section of a list page', () => {
    expect(crumbsFor('/monitors', null)).toEqual([{ label: 'Monitoreo' }, { label: 'Monitores' }]);
    expect(crumbsFor('/settings/members', null)).toEqual([
      { label: 'Organización' },
      { label: 'Miembros' },
    ]);
    expect(crumbsFor('/settings/security', null)).toEqual([
      { label: 'Cuenta' },
      { label: 'Seguridad' },
    ]);
  });

  it('links the parents of a nested page and names its resource', () => {
    const parents = [{ label: 'Monitoreo' }, { label: 'Monitores', href: '/monitors' }];
    expect(crumbsFor('/monitors/new', null)).toEqual([...parents, { label: 'Nuevo monitor' }]);
    expect(crumbsFor('/monitors/0192', 'Sitio')).toEqual([...parents, { label: 'Sitio' }]);
    expect(crumbsFor('/monitors/0192/edit', 'Sitio')).toEqual([
      ...parents,
      { label: 'Sitio', href: '/monitors/0192' },
      { label: 'Editar' },
    ]);
  });

  it('reads «Monitor» until the monitor has loaded', () => {
    expect(crumbsFor('/monitors/0192', null).at(-1)).toEqual({ label: 'Monitor' });
  });

  it('is empty outside the panel sections', () => {
    expect(crumbsFor('/', null)).toEqual([]);
    expect(crumbsFor('/settings', null)).toEqual([]);
  });
});
