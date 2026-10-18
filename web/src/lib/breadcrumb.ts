// The panel's top bar: where the current page sits, «Monitoreo › Monitores › Caído».

/** One step of the breadcrumb; the last one is the current page and has no link. */
export interface Crumb {
  label: string;
  href?: string;
}

// Each section's sidebar group and label; the groups are not pages, so they have no link.
const SECTIONS: Record<string, [group: string, label: string]> = {
  '/monitors': ['Monitoreo', 'Monitores'],
  '/incidents': ['Monitoreo', 'Incidentes'],
  '/channels': ['Monitoreo', 'Canales'],
  '/settings/members': ['Organización', 'Miembros'],
  '/settings/tenant': ['Organización', 'Ajustes'],
  '/settings/api-keys': ['Organización', 'API keys'],
  '/settings/profile': ['Cuenta', 'Perfil'],
  '/settings/security': ['Cuenta', 'Seguridad'],
};

/**
 * The breadcrumb of a panel page. `name` is the resource of a nested page (the monitor of
 * `/monitors/{id}`) once it has loaded; until then it reads «Monitor».
 */
export function crumbsFor(pathname: string, name: string | null): Crumb[] {
  const segments = pathname.split('/').filter(Boolean);
  const depth = segments[0] === 'settings' ? 2 : 1;
  const base = `/${segments.slice(0, depth).join('/')}`;
  const section = SECTIONS[base];
  if (!section) return [];
  const [group, label] = section;
  const [resource, action] = segments.slice(depth);
  if (!resource) return [{ label: group }, { label }];

  const parents: Crumb[] = [{ label: group }, { label, href: base }];
  if (resource === 'new') return [...parents, { label: 'Nuevo monitor' }];
  const title = name ?? 'Monitor';
  if (action === 'edit') {
    return [...parents, { label: title, href: `${base}/${resource}` }, { label: 'Editar' }];
  }
  return [...parents, { label: title }];
}
