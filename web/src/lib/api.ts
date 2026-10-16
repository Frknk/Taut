/** Base URL of taut-api, taken from `PUBLIC_API_URL` at build time. */
export const API_URL: string = (
  (import.meta.env.PUBLIC_API_URL as string | undefined) ?? 'http://localhost:8080'
).replace(/\/$/, '');

/** Base URL of taut-status-bff (public status pages), from `PUBLIC_STATUS_URL`. */
export const STATUS_URL: string = (
  (import.meta.env.PUBLIC_STATUS_URL as string | undefined) ?? 'http://localhost:8085'
).replace(/\/$/, '');

/** Failure of an API call: HTTP status (0 when the API was unreachable) and error code. */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
  }
}

interface RequestOptions {
  /** Tenant id sent in `X-Tenant-Id`. */
  tenant?: string;
  /** JSON body. */
  body?: unknown;
}

/** Calls the API and returns the parsed JSON body (`undefined` for `204 No Content`). */
export async function request<T>(
  method: 'GET' | 'POST' | 'PUT' | 'DELETE',
  path: string,
  { tenant, body }: RequestOptions = {},
): Promise<T> {
  const headers: Record<string, string> = {};
  if (tenant) headers['x-tenant-id'] = tenant;
  if (body !== undefined) headers['content-type'] = 'application/json';

  const init: RequestInit = { method, headers };
  if (body !== undefined) init.body = JSON.stringify(body);

  let response: Response;
  try {
    response = await fetch(`${API_URL}${path}`, init);
  } catch {
    throw new ApiError(0, 'network', 'API unreachable');
  }
  if (response.status === 204) return undefined as T;

  const json: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = (json as { error?: { code?: string; message?: string } } | null)?.error;
    throw new ApiError(
      response.status,
      error?.code ?? 'unknown',
      error?.message ?? `HTTP ${response.status}`,
    );
  }
  return json as T;
}

/** User-facing description of a failed call, in Spanish. API messages stay in English. */
export function describeError(error: unknown): string {
  if (!(error instanceof ApiError)) return 'Ocurrió un error inesperado.';
  switch (error.code) {
    case 'network':
      return 'No se pudo conectar con la API. ¿Está corriendo?';
    case 'unauthorized':
      return 'El tenant seleccionado ya no existe; elige otro.';
    case 'not_found':
      return 'No existe o fue eliminado.';
    case 'forbidden':
      return `Tu plan no lo permite (${error.message}).`;
    case 'conflict':
      return `Ya existe (${error.message}).`;
    case 'bad_request':
      return `La API rechazó los datos: ${error.message}.`;
    default:
      return `Error de la API (${error.status}): ${error.message}.`;
  }
}
