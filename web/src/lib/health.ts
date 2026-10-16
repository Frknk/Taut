/** Body of `GET /health` from a taut service. */
export interface Health {
  status: string;
  service: string;
  version: string;
  commit: string;
  env: string;
}

/** One-line, user-facing description of the API status. */
export function describeHealth(health: Health | null): string {
  if (!health || health.status !== 'ok') {
    return 'API no disponible';
  }
  return `API en línea · ${health.service} v${health.version} (${health.commit})`;
}
